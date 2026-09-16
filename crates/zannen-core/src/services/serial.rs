//! 串口服务：端口枚举、会话管理、读线程 → `serial.rx` 事件。
//!
//! - 会话以 u32 id 标识，写与关闭随时可用；读在独立阻塞线程中进行，
//!   超时读（200ms）轮询取消标志。
//! - 接收数据以 hex 字符串发布（`{session, hex}`），编码解释交给插件。
//! - 热插拔监听：每 1s diff 一次系统端口集合，新增发 `serial.plugged {path}`、
//!   消失发 `serial.unplugged {path}`；被拔口上有活动会话时补发
//!   `serial.error {session, error: "unplugged"}`。mock 虚拟口常驻不参与 diff。
//! - `mock-transport` feature 下提供 `mock://zannen-*` 虚拟端口（见 mock.rs）。

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use parking_lot::RwLock;
use serde::Serialize;
use serde_json::{json, Value};

use crate::events::EventBus;

#[cfg(feature = "mock-transport")]
use super::mock::MockSession;

const READ_TIMEOUT_MS: u64 = 200;
const READ_BUF: usize = 4096;
/// 热插拔轮询周期。
const PLUG_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// 轮询周期的睡眠切片：Drop 置 cancel 后监听线程最多一个切片内退出。
const PLUG_POLL_SLICE: Duration = Duration::from_millis(100);

/// 进程级守卫：热插拔监听线程全局至多一条（SerialService 可多次创建）。
static PLUG_WATCHER_ACTIVE: AtomicBool = AtomicBool::new(false);

/// 端口枚举条目。
#[derive(Debug, Clone, Serialize)]
pub struct PortInfo {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// 是否为虚拟 mock 端口。
    pub mock: bool,
}

struct RealSession {
    writer: Mutex<Box<dyn serialport::SerialPort>>,
    cancel: Arc<AtomicBool>,
    /// 打开时的端口路径：热插拔监听据此把 unplugged 关联到会话。
    path: String,
}

enum Session {
    Real(RealSession),
    #[cfg(feature = "mock-transport")]
    Mock(MockSession),
}

impl Session {
    /// 会话占用的真实端口路径；mock 会话是常驻虚拟口，不参与热插拔 diff。
    fn real_path(&self) -> Option<&str> {
        match self {
            Session::Real(real) => Some(&real.path),
            #[cfg(feature = "mock-transport")]
            Session::Mock(_) => None,
        }
    }
}

pub struct SerialService {
    bus: EventBus,
    sessions: Arc<RwLock<HashMap<u32, Session>>>,
    next_id: AtomicU32,
    plug_cancel: Arc<AtomicBool>,
    /// 热插拔监听线程句柄：None 表示本实例未持有（进程内已有另一条）。
    plug_thread: Option<std::thread::JoinHandle<()>>,
}

impl SerialService {
    pub fn new(bus: EventBus) -> Self {
        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let plug_cancel = Arc::new(AtomicBool::new(false));

        // 热插拔监听线程只启动一次（进程级守卫）；持有者在 Drop 时回收线程并复位守卫。
        let mut plug_thread = None;
        if !PLUG_WATCHER_ACTIVE.swap(true, Ordering::SeqCst) {
            let handle = std::thread::Builder::new()
                .name("serial-plug-watch".into())
                .spawn({
                    let bus = bus.clone();
                    let sessions = sessions.clone();
                    let cancel = plug_cancel.clone();
                    move || plug_watch_loop(bus, sessions, cancel)
                })
                .ok();
            if handle.is_some() {
                plug_thread = handle;
            } else {
                // 线程创建失败：归还守卫，下次 new 再试。
                PLUG_WATCHER_ACTIVE.store(false, Ordering::SeqCst);
                log::warn!("serial plug watcher spawn failed");
            }
        }

        Self {
            bus,
            sessions,
            next_id: AtomicU32::new(1),
            plug_cancel,
            plug_thread,
        }
    }

    /// 枚举可用端口（含 mock 虚拟端口）。
    pub fn list(&self) -> Result<Value, String> {
        let mut ports: Vec<PortInfo> = Vec::new();
        let found = serialport::available_ports().map_err(|e| e.to_string())?;
        for p in found {
            let mut info = PortInfo {
                path: p.port_name,
                vid: None,
                pid: None,
                product: None,
                manufacturer: None,
                serial_number: None,
                mock: false,
            };
            if let serialport::SerialPortType::UsbPort(usb) = p.port_type {
                info.vid = Some(format!("0x{:04X}", usb.vid));
                info.pid = Some(format!("0x{:04X}", usb.pid));
                info.product = usb.product;
                info.manufacturer = usb.manufacturer;
                info.serial_number = usb.serial_number;
            }
            ports.push(info);
        }
        #[cfg(feature = "mock-transport")]
        ports.extend(super::mock::virtual_ports());
        serde_json::to_value(ports).map_err(|e| e.to_string())
    }

    /// 打开端口，返回 `{session}`。读线程随即开始发布 `serial.rx`。
    pub fn open(&self, path: &str, baud: u32) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        #[cfg(feature = "mock-transport")]
        if let Some(session) = MockSession::open(id, path, self.bus.clone()) {
            self.sessions.write().insert(id, Session::Mock(session));
            return Ok(json!({ "session": id, "path": path }));
        }

        let port = serialport::new(path, baud)
            .timeout(Duration::from_millis(READ_TIMEOUT_MS))
            .open()
            .map_err(|e| format!("open {path}: {e}"))?;
        let mut reader = port.try_clone().map_err(|e| format!("clone {path}: {e}"))?;
        let cancel = Arc::new(AtomicBool::new(false));

        let session = RealSession {
            writer: Mutex::new(port),
            cancel: cancel.clone(),
            path: path.to_string(),
        };
        self.sessions.write().insert(id, Session::Real(session));

        let bus = self.bus.clone();
        std::thread::Builder::new()
            .name(format!("serial-rx-{id}"))
            .spawn(move || {
                let mut buf = [0u8; READ_BUF];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    match reader.read(&mut buf) {
                        Ok(0) => {}
                        Ok(n) => bus.publish(
                            "serial.rx",
                            json!({ "session": id, "hex": hex_encode(&buf[..n]) }),
                        ),
                        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(e) => {
                            bus.publish(
                                "serial.error",
                                json!({ "session": id, "error": e.to_string() }),
                            );
                            break;
                        }
                    }
                }
                bus.publish("serial.closed", json!({ "session": id }));
                log::debug!("serial reader thread exit: session {id}");
            })
            .map_err(|e| format!("spawn reader: {e}"))?;

        Ok(json!({ "session": id, "path": path }))
    }

    /// 写数据。`encoding`: "text"（UTF-8 字节）| "hex"；`append`: "none"|"lf"|"crlf"。
    pub fn write(
        &self,
        session: u32,
        data: &str,
        encoding: &str,
        append: &str,
    ) -> Result<Value, String> {
        let sessions = self.sessions.read();
        let Some(session_entry) = sessions.get(&session) else {
            return Err(format!("session {session} not open"));
        };
        let mut bytes = match encoding {
            "hex" => hex_decode(data)?,
            "text" => data.as_bytes().to_vec(),
            other => return Err(format!("unknown encoding: {other}")),
        };
        match append {
            "lf" => bytes.push(b'\n'),
            "crlf" => bytes.extend_from_slice(b"\r\n"),
            "none" => {}
            other => return Err(format!("unknown append: {other}")),
        }
        let len = bytes.len();
        match session_entry {
            Session::Real(real) => {
                let mut port = real
                    .writer
                    .lock()
                    .map_err(|_| "writer poisoned".to_string())?;
                use std::io::Write;
                port.write_all(&bytes).map_err(|e| e.to_string())?;
                port.flush().map_err(|e| e.to_string())?;
            }
            #[cfg(feature = "mock-transport")]
            Session::Mock(mock) => mock.write(&bytes),
        }
        Ok(json!({ "sent": len }))
    }

    pub fn close(&self, session: u32) -> Result<Value, String> {
        let removed = self.sessions.write().remove(&session);
        match removed {
            Some(Session::Real(real)) => {
                real.cancel.store(true, Ordering::Relaxed);
                Ok(json!({ "closed": session }))
            }
            #[cfg(feature = "mock-transport")]
            Some(Session::Mock(mock)) => {
                mock.close();
                Ok(json!({ "closed": session }))
            }
            None => Err(format!("session {session} not open")),
        }
    }
}

impl Drop for SerialService {
    fn drop(&mut self) {
        // 停热插拔监听线程；仅持有线程的实例复位进程级守卫（允许后续实例重启监听）。
        self.plug_cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = self.plug_thread.take() {
            let _ = handle.join();
            PLUG_WATCHER_ACTIVE.store(false, Ordering::SeqCst);
        }
        // 停掉仍在运行的会话线程（读线程/mock 设备），避免随服务泄漏。
        for (_, entry) in self.sessions.write().drain() {
            match entry {
                Session::Real(real) => real.cancel.store(true, Ordering::Relaxed),
                #[cfg(feature = "mock-transport")]
                Session::Mock(mock) => mock.close(),
            }
        }
    }
}

/// 当前真实串口路径集合。mock:// 虚拟口常驻，不参与热插拔 diff。
fn real_port_names() -> BTreeSet<String> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.port_name)
        .filter(|name| !name.starts_with("mock://"))
        .collect()
}

/// 端口集合 diff：返回 (新增, 消失)，两边均按字典序输出（BTreeSet 迭代序）。
fn diff_ports(old: &BTreeSet<String>, new: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    (
        new.difference(old).cloned().collect(),
        old.difference(new).cloned().collect(),
    )
}

/// 热插拔监听循环：周期性 diff 端口集合并发布 plugged/unplugged 事件。
fn plug_watch_loop(
    bus: EventBus,
    sessions: Arc<RwLock<HashMap<u32, Session>>>,
    cancel: Arc<AtomicBool>,
) {
    // 以启动时的存量端口为基线，不为已有端口补发 plugged。
    let mut prev = real_port_names();
    loop {
        // 分片睡眠，保证 Drop 置 cancel 后线程能及时退出。
        let slices = (PLUG_POLL_INTERVAL.as_millis() / PLUG_POLL_SLICE.as_millis()) as u32;
        for _ in 0..slices {
            if cancel.load(Ordering::Relaxed) {
                log::debug!("serial plug watcher exit");
                return;
            }
            std::thread::sleep(PLUG_POLL_SLICE);
        }

        let now = real_port_names();
        let (added, removed) = diff_ports(&prev, &now);
        for path in added {
            log::info!("serial port plugged: {path}");
            bus.publish("serial.plugged", json!({ "path": path }));
        }
        for path in removed {
            log::info!("serial port unplugged: {path}");
            bus.publish("serial.unplugged", json!({ "path": path }));
            // 被拔口上的活动会话补发 serial.error 关联拔出原因；
            // 读线程自身也会报错退出并发 serial.closed。
            for (id, entry) in sessions.read().iter() {
                if entry.real_path() == Some(path.as_str()) {
                    bus.publish(
                        "serial.error",
                        json!({ "session": id, "error": "unplugged" }),
                    );
                }
            }
        }
        prev = now;
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

pub fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err("hex string length must be even".to_string());
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let bytes = [0x00, 0xAB, 0xFF, 0x10];
        let hex = hex_encode(&bytes);
        assert_eq!(hex, "00abff10");
        assert_eq!(hex_decode(&hex).unwrap(), bytes);
        assert_eq!(hex_decode("00 AB ff 10").unwrap(), bytes);
        assert!(hex_decode("abc").is_err());
    }

    #[test]
    fn list_ports_does_not_fail() {
        let bus = EventBus::new(4);
        let svc = SerialService::new(bus);
        let v = svc.list().unwrap();
        assert!(v.is_array());
    }

    fn port_set(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn diff_ports_detects_added_and_removed() {
        let old = port_set(&["/dev/cu.a", "/dev/cu.b"]);
        let new = port_set(&["/dev/cu.b", "/dev/cu.c", "/dev/cu.d"]);
        let (added, removed) = diff_ports(&old, &new);
        assert_eq!(added, vec!["/dev/cu.c", "/dev/cu.d"]);
        assert_eq!(removed, vec!["/dev/cu.a"]);
    }

    #[test]
    fn diff_ports_edge_cases() {
        // 无变化
        let same = port_set(&["/dev/cu.a"]);
        let (added, removed) = diff_ports(&same, &same);
        assert!(added.is_empty() && removed.is_empty());
        // 空 → 有：全部视为新增
        let (added, removed) = diff_ports(&port_set(&[]), &port_set(&["/dev/cu.a", "/dev/cu.b"]));
        assert_eq!(added, vec!["/dev/cu.a", "/dev/cu.b"]);
        assert!(removed.is_empty());
        // 有 → 空：全部视为消失
        let (added, removed) = diff_ports(&port_set(&["/dev/cu.a"]), &port_set(&[]));
        assert!(added.is_empty());
        assert_eq!(removed, vec!["/dev/cu.a"]);
    }

    /// 并发单测也会创建 SerialService（进程级守卫全局唯一）：
    /// 重试等待，直到拿到监听线程的所有权。
    fn wait_for_watcher_owner(bus: &EventBus) -> SerialService {
        for _ in 0..100 {
            let svc = SerialService::new(bus.clone());
            if svc.plug_thread.is_some() {
                return svc;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("failed to own hotplug watcher thread within 5s");
    }

    #[test]
    fn plug_watcher_starts_once_and_stops_on_drop() {
        let bus = EventBus::new(8);
        let owner = wait_for_watcher_owner(&bus);
        // 持有者存活期间，第二个实例不再启动监听线程
        let second = SerialService::new(bus.clone());
        assert!(second.plug_thread.is_none());
        drop(second);
        // Drop：join 监听线程并复位守卫 → 之后可再次启动
        drop(owner);
        let owner2 = wait_for_watcher_owner(&bus);
        drop(owner2);
    }
}
