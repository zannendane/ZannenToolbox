//! BLE 服务：扫描 → NUS 连接 → 数据面（notify 入 `ble.rx`、write 出 RX 特征）。
//!
//! btleplug 全面异步，为避免把 async 暴露给同步的 FFI 调度层，
//! 本服务在专属线程上运行独立 current_thread runtime（配 `LocalSet`），
//! 命令经 tokio unbounded mpsc 串行进入线程执行：
//! - 会话句柄（Peripheral 等）只活在该线程内，不跨线程传递；
//! - 每个连接的通知流 `spawn_local` 到同一线程，回调里直接 publish
//!   （EventBus 是 Send+Sync，可跨线程使用）；
//! - 通知流结束（设备断开）经内部命令 `NotifyEnded` 回报，统一清理。
//!
//! 会话 id 从 10000 起递增（串口会话从 1 起，互不重叠）。
//! `mock-transport` feature 下提供虚拟 BLE 外设（见 mock.rs），扫描结果附加
//! mock 条目、`connect` 命中 mock 地址时走虚拟会话，保证无硬件可测。

use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};

use serde_json::{json, Value};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use super::serial::hex_encode;
use crate::events::EventBus;

#[cfg(feature = "mock-transport")]
use super::mock::MockBleSession;

/// Nordic UART Service（NUS）service UUID。
pub const NUS_SERVICE: &str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
/// NUS TX characteristic（设备 → 主机，notify）。
pub const NUS_TX: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";
/// NUS RX characteristic（主机 → 设备，write）。
pub const NUS_RX: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";

/// BLE 会话 id 起点。
pub const BLE_SESSION_BASE: u32 = 10_000;

enum BleCommand {
    Scan {
        timeout_ms: u64,
        respond: Sender<Result<Value, String>>,
    },
    Connect {
        address: String,
        respond: Sender<Result<Value, String>>,
    },
    Write {
        session: u32,
        bytes: Vec<u8>,
        respond: Sender<Result<Value, String>>,
    },
    Close {
        session: u32,
        respond: Sender<Result<Value, String>>,
    },
    /// 内部命令：某会话的通知流结束（设备断开 / 退订），由转发任务回报。
    NotifyEnded { session: u32 },
}

/// 真实外设会话：Peripheral 句柄与 NUS RX（写）特征。
struct RealSession {
    peripheral: btleplug::platform::Peripheral,
    rx_char: btleplug::api::Characteristic,
}

enum Session {
    Real(RealSession),
    #[cfg(feature = "mock-transport")]
    Mock(MockBleSession),
}

/// BLE 线程内的全部可变状态。
struct BleState {
    bus: EventBus,
    sessions: HashMap<u32, Session>,
    next_id: u32,
    cmd_tx: UnboundedSender<BleCommand>,
}

pub struct BleService {
    cmd_tx: UnboundedSender<BleCommand>,
}

impl BleService {
    /// 创建服务并启动 BLE 线程。
    pub fn new(bus: EventBus) -> Self {
        let (cmd_tx, cmd_rx) = unbounded_channel::<BleCommand>();
        let cmd_tx_thread = cmd_tx.clone();
        std::thread::Builder::new()
            .name("ble-rt".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::error!("ble runtime init failed: {e}");
                        return;
                    }
                };
                // LocalSet：通知转发等任务可 spawn_local，无需 Send。
                let local = tokio::task::LocalSet::new();
                let mut cmd_rx = cmd_rx;
                local.block_on(&rt, async move {
                    let mut state = BleState {
                        bus,
                        sessions: HashMap::new(),
                        next_id: BLE_SESSION_BASE,
                        cmd_tx: cmd_tx_thread,
                    };
                    while let Some(cmd) = cmd_rx.recv().await {
                        state.handle(cmd).await;
                    }
                });
            })
            .expect("spawn ble thread");
        Self { cmd_tx }
    }

    /// 发一条命令并同步等待结果。
    fn call(
        &self,
        make: impl FnOnce(Sender<Result<Value, String>>) -> BleCommand,
    ) -> Result<Value, String> {
        let (respond, rx) = channel();
        self.cmd_tx
            .send(make(respond))
            .map_err(|_| "ble thread dead".to_string())?;
        rx.recv()
            .map_err(|_| "ble thread dropped response".to_string())?
    }

    /// 扫描 `timeout_ms` 毫秒，返回发现的设备列表（每台也会发 `ble.found`）。
    /// `mock-transport` 下列表附加虚拟外设条目。
    pub fn scan(&self, timeout_ms: u64) -> Result<Value, String> {
        self.call(|respond| BleCommand::Scan {
            timeout_ms,
            respond,
        })
    }

    /// 按地址连接外围设备并订阅 NUS TX notify，返回 `{session}`。
    /// 之后设备数据经 `ble.rx {session, hex}` 事件到达。
    pub fn connect(&self, address: &str) -> Result<Value, String> {
        let address = address.to_owned();
        self.call(|respond| BleCommand::Connect { address, respond })
    }

    /// 向 NUS RX 特征写数据（write-without-response 优先），返回 `{sent}`。
    pub fn write(&self, session: u32, bytes: &[u8]) -> Result<Value, String> {
        let bytes = bytes.to_vec();
        self.call(|respond| BleCommand::Write {
            session,
            bytes,
            respond,
        })
    }

    /// 断开并清理会话，返回 `{closed}`。
    pub fn close(&self, session: u32) -> Result<Value, String> {
        self.call(|respond| BleCommand::Close { session, respond })
    }
}

impl BleState {
    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    async fn handle(&mut self, cmd: BleCommand) {
        match cmd {
            BleCommand::Scan {
                timeout_ms,
                respond,
            } => {
                let _ = respond.send(self.handle_scan(timeout_ms).await);
            }
            BleCommand::Connect { address, respond } => {
                let _ = respond.send(self.handle_connect(&address).await);
            }
            BleCommand::Write {
                session,
                bytes,
                respond,
            } => {
                let _ = respond.send(self.handle_write(session, &bytes).await);
            }
            BleCommand::Close { session, respond } => {
                let _ = respond.send(self.handle_close(session).await);
            }
            BleCommand::NotifyEnded { session } => {
                // 设备侧断开：仅当会话仍在表中（非用户 close 路径）才清理并广播。
                if self.sessions.remove(&session).is_some() {
                    log::info!("ble session {session} disconnected");
                    self.bus
                        .publish("ble.closed", json!({ "session": session }));
                }
            }
        }
    }

    async fn handle_scan(&self, timeout_ms: u64) -> Result<Value, String> {
        // 适配器初始化（CoreBluetooth 授权/上电等）可能长时间不返回，
        // 给整个扫描加上界：发现窗口 timeout_ms + 2s 初始化宽限。
        let budget = std::time::Duration::from_millis(timeout_ms.saturating_add(2_000));
        let result = match tokio::time::timeout(budget, scan(timeout_ms, self.bus.clone())).await {
            Ok(r) => r,
            Err(_) => Err(format!("ble scan exceeded budget {}ms", budget.as_millis())),
        };
        #[cfg(feature = "mock-transport")]
        {
            // 真实扫描失败不阻塞 mock 条目返回（无适配器 / 无权限的开发机）。
            let mut devices = match result {
                Ok(v) => v["devices"].as_array().cloned().unwrap_or_default(),
                Err(e) => {
                    log::warn!("ble scan failed, returning mock entries only: {e}");
                    Vec::new()
                }
            };
            for ad in super::mock::mock_ble_advertisements() {
                self.bus.publish("ble.found", ad.clone());
                devices.push(ad);
            }
            Ok(json!({ "devices": devices }))
        }
        #[cfg(not(feature = "mock-transport"))]
        result
    }

    async fn handle_connect(&mut self, address: &str) -> Result<Value, String> {
        let id = self.alloc_id();

        #[cfg(feature = "mock-transport")]
        if let Some(mock) = MockBleSession::open(id, address, self.bus.clone()) {
            self.sessions.insert(id, Session::Mock(mock));
            return Ok(json!({ "session": id }));
        }

        self.connect_real(id, address).await
    }

    /// 真实外设：连接 → 服务发现 → 订阅 NUS TX → 通知转发任务。
    async fn connect_real(&mut self, id: u32, address: &str) -> Result<Value, String> {
        use btleplug::api::{Central as _, Manager as _, Peripheral as _};
        use btleplug::platform::Manager;

        let manager = Manager::new()
            .await
            .map_err(|e| format!("ble manager: {e}"))?;
        let adapters = manager
            .adapters()
            .await
            .map_err(|e| format!("ble adapters: {e}"))?;
        let central = adapters
            .into_iter()
            .next()
            .ok_or_else(|| "no BLE adapter found".to_string())?;

        // 按地址匹配已发现的外设（通常需先 scan 让设备进入缓存）。
        let peripherals = central
            .peripherals()
            .await
            .map_err(|e| format!("list peripherals: {e}"))?;
        let mut target = None;
        for p in peripherals {
            if let Ok(Some(props)) = p.properties().await {
                if props.address.to_string().eq_ignore_ascii_case(address) {
                    target = Some(p);
                    break;
                }
            }
        }
        let peripheral =
            target.ok_or_else(|| format!("peripheral {address} not found (scan first)"))?;

        peripheral
            .connect()
            .await
            .map_err(|e| format!("connect {address}: {e}"))?;
        // 连接后的任一步失败都要断开，避免泄漏半连接状态。
        let nus = async {
            peripheral
                .discover_services()
                .await
                .map_err(|e| format!("discover services: {e}"))?;
            let chars = peripheral.characteristics();
            let find = |uuid: &str| {
                chars
                    .iter()
                    .find(|c| c.uuid.to_string().eq_ignore_ascii_case(uuid))
                    .cloned()
            };
            let tx_char =
                find(NUS_TX).ok_or_else(|| "NUS TX characteristic not found".to_string())?;
            let rx_char =
                find(NUS_RX).ok_or_else(|| "NUS RX characteristic not found".to_string())?;
            peripheral
                .subscribe(&tx_char)
                .await
                .map_err(|e| format!("subscribe NUS TX: {e}"))?;
            let stream = peripheral
                .notifications()
                .await
                .map_err(|e| format!("notification stream: {e}"))?;
            Ok((rx_char, stream))
        }
        .await;
        let (rx_char, mut stream) = match nus {
            Ok(v) => v,
            Err(e) => {
                let _ = peripheral.disconnect().await;
                return Err(e);
            }
        };

        // 通知 → ble.rx；流结束 → 内部 NotifyEnded 命令（统一走线程清理）。
        let bus = self.bus.clone();
        let cmd_tx = self.cmd_tx.clone();
        tokio::task::spawn_local(async move {
            use futures::StreamExt;
            while let Some(n) = stream.next().await {
                bus.publish(
                    "ble.rx",
                    json!({ "session": id, "hex": hex_encode(&n.value) }),
                );
            }
            let _ = cmd_tx.send(BleCommand::NotifyEnded { session: id });
        });

        self.sessions.insert(
            id,
            Session::Real(RealSession {
                peripheral,
                rx_char,
            }),
        );
        log::info!("ble {address} connected as session {id}");
        Ok(json!({ "session": id }))
    }

    async fn handle_write(&mut self, session: u32, bytes: &[u8]) -> Result<Value, String> {
        let Some(entry) = self.sessions.get(&session) else {
            return Err(format!("session {session} not open"));
        };
        match entry {
            Session::Real(real) => {
                use btleplug::api::{CharPropFlags, Peripheral as _, WriteType};
                // write-without-response 优先；特征不支持时退回 write-with-response。
                let write_type = if real
                    .rx_char
                    .properties
                    .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
                {
                    WriteType::WithoutResponse
                } else {
                    WriteType::WithResponse
                };
                match real
                    .peripheral
                    .write(&real.rx_char, bytes, write_type)
                    .await
                {
                    Ok(()) => Ok(json!({ "sent": bytes.len() })),
                    Err(e) => {
                        let msg = format!("ble write: {e}");
                        self.bus
                            .publish("ble.error", json!({ "session": session, "error": msg }));
                        Err(msg)
                    }
                }
            }
            #[cfg(feature = "mock-transport")]
            Session::Mock(mock) => {
                mock.write(bytes);
                Ok(json!({ "sent": bytes.len() }))
            }
        }
    }

    async fn handle_close(&mut self, session: u32) -> Result<Value, String> {
        let Some(entry) = self.sessions.remove(&session) else {
            return Err(format!("session {session} not open"));
        };
        match entry {
            Session::Real(real) => {
                use btleplug::api::Peripheral as _;
                let _ = real.peripheral.disconnect().await;
                // 真实路径无人代发 closed（会话已移除，NotifyEnded 会跳过），在此发布。
                self.bus
                    .publish("ble.closed", json!({ "session": session }));
            }
            #[cfg(feature = "mock-transport")]
            Session::Mock(mock) => {
                // mock 设备线程退出时自行发布 ble.closed。
                mock.close();
            }
        }
        Ok(json!({ "closed": session }))
    }
}

async fn scan(timeout_ms: u64, bus: EventBus) -> Result<Value, String> {
    use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter};
    use btleplug::platform::Manager;
    use futures::StreamExt;

    /// 权限被拒单列错误码（系统级拒绝/未授权），其余错误保留上下文原文。
    fn ble_err(context: &str, e: btleplug::Error) -> String {
        if matches!(e, btleplug::Error::PermissionDenied) {
            format!("[E3201] {context}: bluetooth permission denied by system")
        } else {
            format!("{context}: {e}")
        }
    }

    let manager = Manager::new()
        .await
        .map_err(|e| ble_err("ble manager", e))?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|e| ble_err("ble adapters", e))?;
    let central = adapters
        .into_iter()
        .next()
        .ok_or_else(|| "no BLE adapter found".to_string())?;

    central
        .start_scan(ScanFilter::default())
        .await
        .map_err(|e| ble_err("start scan", e))?;
    let mut events = central.events().await.map_err(|e| format!("events: {e}"))?;

    let mut found: Vec<Value> = Vec::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(timeout_ms);

    loop {
        let remain = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remain.is_zero() {
            break;
        }
        tokio::select! {
            maybe = events.next() => {
                match maybe {
                    Some(CentralEvent::DeviceDiscovered(id))
                    | Some(CentralEvent::DeviceUpdated(id)) => {
                        if let Ok(peripheral) = central.peripheral(&id).await {
                            if let Ok(Some(props)) = peripheral.properties().await {
                                let entry = json!({
                                    "id": id.to_string(),
                                    "address": props.address.to_string(),
                                    "name": props.local_name,
                                    "rssi": props.rssi,
                                    "services": props.services.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
                                });
                                if !found.iter().any(|d| d["id"] == entry["id"]) {
                                    bus.publish("ble.found", entry.clone());
                                    found.push(entry);
                                }
                            }
                        }
                    }
                    Some(_) => {}
                    None => break,
                }
            }
            _ = tokio::time::sleep(remain) => break,
        }
    }

    let _ = central.stop_scan().await;
    Ok(json!({ "devices": found }))
}

#[cfg(all(test, feature = "mock-transport"))]
mod tests {
    use super::*;

    /// 在总线上等待匹配 session 的 `ble.rx`；`needle` 非空时还要求解码文本包含它。
    async fn recv_rx(
        rx: &mut tokio::sync::broadcast::Receiver<crate::events::Event>,
        session: u32,
        needle: Option<&str>,
    ) -> crate::events::Event {
        loop {
            let ev = rx.recv().await.unwrap();
            if ev.topic != "ble.rx" || ev.payload["session"] != json!(session) {
                continue;
            }
            if let Some(s) = needle {
                let bytes =
                    crate::services::serial::hex_decode(ev.payload["hex"].as_str().unwrap())
                        .unwrap();
                if !String::from_utf8_lossy(&bytes).contains(s) {
                    continue;
                }
            }
            break ev;
        }
    }

    #[tokio::test]
    async fn mock_scan_lists_virtual_devices() {
        let bus = EventBus::new(64);
        let svc = BleService::new(bus);
        let v = svc.scan(50).unwrap();
        let devices = v["devices"].as_array().unwrap();
        let mock_count = devices.iter().filter(|d| d["mock"] == json!(true)).count();
        assert_eq!(mock_count, 3);
        assert!(devices
            .iter()
            .any(|d| d["address"] == json!("00:11:22:33:FE:01")));
    }

    #[tokio::test]
    async fn mock_connect_write_close() {
        use std::time::Duration;

        let bus = EventBus::new(512);
        let svc = BleService::new(bus.clone());
        let mut rx = bus.subscribe();

        // connect → 会话 id 从 10000 起
        let v = svc.connect("00:11:22:33:FE:01").unwrap();
        let session = v["session"].as_u64().unwrap() as u32;
        assert!(session >= BLE_SESSION_BASE);

        // 500ms 内应收到 ble.rx（hello 行在会话创建时立即发出）
        let first =
            tokio::time::timeout(Duration::from_millis(500), recv_rx(&mut rx, session, None))
                .await
                .expect("no ble.rx within 500ms");
        assert!(!first.payload["hex"].as_str().unwrap().is_empty());

        // write ping → 返回 sent，随后应收到 pong 行
        let ping = br#"{"cmd":"ping"}"#;
        let w = svc.write(session, ping).unwrap();
        assert_eq!(w["sent"].as_u64().unwrap(), ping.len() as u64);
        tokio::time::timeout(
            Duration::from_millis(500),
            recv_rx(&mut rx, session, Some("pong")),
        )
        .await
        .expect("no pong within 500ms");

        // close → 再次写应报错；会话 id 单调递增
        svc.close(session).unwrap();
        assert!(svc.write(session, b"x").is_err());
        let v2 = svc.connect("00:11:22:33:FE:02").unwrap();
        let session2 = v2["session"].as_u64().unwrap() as u32;
        assert!(session2 > session);
        svc.close(session2).unwrap();
    }
}
