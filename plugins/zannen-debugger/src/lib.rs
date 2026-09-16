//! # Zannen 调试模块插件
//!
//! 面向 ZannenSmol / ZannenSmolAir / ZannenDongle 的调试能力：
//!
//! - `ping` / `fingerprints.list` / `status.rules`：元信息
//! - `device.scan`：串口 identify 探测 + BLE 广播指纹 → 统一设备上报
//! - `device.connect` / `device.disconnect` / `device.list`：会话管理
//! - `serial.write`：串口终端写通道（rx 经 `serial.rx` 总线事件直达前端）
//! - `uf2.enter_bootloader` / `uf2.flash` / `uf2.volumes` / `uf2.validate`：固件刷写
//!
//! 数据面：`serial.rx` 字节流 → 行切割 → 协议解析 →
//! IMU/RF 样本 33ms 批量发 `zannen.debugger/imu.batch`、`zannen.debugger/rf.batch`；
//! status 包即时解读为卡片发 `zannen.debugger/status`。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use zannen_plugin_api::{
    export_plugin, FrontendDecl, HostHandle, PluginError, PluginManifest, PluginRoute,
    ZannenPlugin, ZANNEN_ABI_VERSION,
};

mod fingerprint;
mod protocol;
mod rules;

use fingerprint::{Fingerprint, FingerprintDb};
use protocol::LineParser;
use rules::RulesDoc;

const DEVICES_JSON: &str = include_str!("../devices.json");
const RULES_JSON: &str = include_str!("../status_rules.json");

/// 高频数据批量发送周期（≈30Hz）。
const FLUSH_INTERVAL_MS: u64 = 33;
/// 批量缓冲上限：超出后丢弃最旧样本（前端掉队时保护内存）。
const BATCH_CAP: usize = 4000;
const IDENTIFY_TIMEOUT_MS: u64 = 800;
const IDENTIFY_POLL_MS: u64 = 25;

struct SessionState {
    parser: LineParser,
    device_label: String,
    /// 会话来源路径（串口路径或 ble:<address>），热插拔清理用。
    path: String,
    hello: Option<Value>,
    /// 真实固件控制台 `info` 应答的 `Board:` 行（文本协议识别用）。
    console_board: Option<String>,
    streaming: bool,
}

/// BLE 会话 id 下限（与 zannen-core BleService 约定一致）。
const BLE_SESSION_BASE: u32 = 10_000;

#[derive(Default)]
struct Shared {
    sessions: HashMap<u32, SessionState>,
    imu_buf: Vec<Value>,
    rf_buf: Vec<Value>,
    flusher_started: bool,
}

pub struct DebuggerPlugin {
    host: HostHandle,
    fingerprints: FingerprintDb,
    rules: RulesDoc,
    shared: Arc<Mutex<Shared>>,
}

impl ZannenPlugin for DebuggerPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "zannen.debugger".into(),
            name: "Zannen SlimeVR Debug".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            api: ZANNEN_ABI_VERSION,
            description: "ZannenSmol / SmolAir / Dongle debug module".into(),
            icon: Some("bug".into()),
            frontend: Some(FrontendDecl {
                entry: "frontend/dist/index.js".into(),
                css: Some("frontend/dist/style.css".into()),
            }),
            backend: Some(zannen_plugin_api::BackendDecl {
                name: "zannen_debugger".into(),
            }),
            capabilities: vec!["serial".into(), "ble".into(), "uf2".into()],
            routes: [
                ("devices", "Devices", "cpu"),
                ("console", "Serial Console", "terminal"),
                ("timeline", "Data Timeline", "activity"),
                ("motion3d", "3D Motion", "box"),
                ("firmware", "Firmware", "arrow-down-to-line"),
                ("status", "Status", "stethoscope"),
            ]
            .into_iter()
            .map(|(path, title, icon)| PluginRoute {
                path: path.into(),
                title: title.into(),
                icon: icon.into(),
            })
            .collect(),
        }
    }

    fn new(host: HostHandle) -> Self {
        let fingerprints =
            FingerprintDb::parse(DEVICES_JSON).expect("embedded devices.json must be valid");
        let rules = RulesDoc::parse(RULES_JSON).expect("embedded status_rules.json must be valid");
        host.log(3, "zannen.debugger initialized");
        Self {
            host,
            fingerprints,
            rules,
            shared: Arc::new(Mutex::new(Shared::default())),
        }
    }

    fn invoke(&mut self, method: &str, args: Value) -> Result<Value, PluginError> {
        match method {
            "ping" => Ok(json!({
                "pong": true,
                "id": "zannen.debugger",
                "version": env!("CARGO_PKG_VERSION"),
            })),
            "fingerprints.list" => serde_json::to_value(&self.fingerprints.devices)
                .map_err(|e| PluginError::Other(e.to_string())),
            "status.rules" => serde_json::to_value(&self.rules.cards)
                .map_err(|e| PluginError::Other(e.to_string())),
            "device.scan" => self.scan_devices(&args),
            "device.connect" => self.connect(&args),
            "device.disconnect" => self.disconnect(&args),
            "device.list" => Ok(self.host.call("devices", "list", &json!({}))?),
            "serial.write" => {
                let session = req_u64(&args, "session")? as u32;
                let data = req_str(&args, "data")?;
                let encoding = args
                    .get("encoding")
                    .and_then(Value::as_str)
                    .unwrap_or("text");
                let append = args.get("append").and_then(Value::as_str).unwrap_or("lf");
                let service = if session >= BLE_SESSION_BASE {
                    "ble"
                } else {
                    "serial"
                };
                Ok(self.host.call(
                    service,
                    "write",
                    &json!({
                        "session": session, "data": data,
                        "encoding": encoding, "append": append,
                    }),
                )?)
            }
            "uf2.enter_bootloader" => self.enter_bootloader(&args),
            "uf2.flash" => Ok(self.host.call("uf2", "flash", &args)?),
            "uf2.volumes" => Ok(self.host.call("uf2", "volumes", &json!({}))?),
            "uf2.validate" => self.validate_uf2(&args),
            // MCUboot SMP 串行 DFU（nRF54 路径），直接透传宿主服务
            "dfu.upload" | "dfu.confirm" | "dfu.reset" => {
                let mut parts = method.split('.');
                Ok(self.host.call(
                    parts.next().unwrap_or("dfu"),
                    parts.next().unwrap_or(""),
                    &args,
                )?)
            }
            other => Err(PluginError::UnknownMethod(other.into())),
        }
    }

    fn on_event(&mut self, topic: &str, payload: Value) {
        match topic {
            "serial.rx" | "ble.rx" => self.on_serial_rx(&payload),
            "serial.closed" | "serial.error" | "ble.closed" | "ble.error" => {
                if let Some(session) = payload.get("session").and_then(Value::as_u64) {
                    if let Ok(mut shared) = self.shared.lock() {
                        shared.sessions.remove(&(session as u32));
                    }
                }
            }
            // 串口热拔出：清理该端口上的会话并从注册表移除设备
            "serial.unplugged" => {
                if let Some(path) = payload.get("path").and_then(Value::as_str) {
                    let mut dead_sessions = Vec::new();
                    if let Ok(mut shared) = self.shared.lock() {
                        shared.sessions.retain(|id, s| {
                            if s.path == path {
                                dead_sessions.push(*id);
                                false
                            } else {
                                true
                            }
                        });
                    }
                    for session in dead_sessions {
                        let _ = self
                            .host
                            .call("serial", "close", &json!({ "session": session }));
                    }
                    let _ = self.host.call(
                        "devices",
                        "remove",
                        &json!({ "id": format!("serial:{path}") }),
                    );
                }
            }
            _ => {}
        }
    }
}

impl DebuggerPlugin {
    /// 设备扫描：串口候选端口 identify 探测 + BLE 广播指纹。
    /// `args.skip_ble`：授权前置告知未完成时跳过 BLE（避免无上下文触发系统授权框）。
    fn scan_devices(&mut self, args: &Value) -> Result<Value, PluginError> {
        let skip_ble = args
            .get("skip_ble")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut found: Vec<Value> = Vec::new();

        // —— 串口探测 ——
        let ports = self.host.call("serial", "list", &json!({}))?;
        let ports = ports.as_array().cloned().unwrap_or_default();
        for port in ports {
            let Some(path) = port.get("path").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            let is_mock = port.get("mock").and_then(Value::as_bool).unwrap_or(false);
            let vid = port.get("vid").and_then(Value::as_str).unwrap_or("");
            let pid = port.get("pid").and_then(Value::as_str).unwrap_or("");
            let product = port
                .get("product")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            let candidate = is_mock
                || self.fingerprints.by_usb(vid, pid).is_some()
                || self.fingerprints.by_usb_product(&product).is_some()
                || product.contains("zannen");
            if !candidate {
                continue;
            }

            match self.probe_port(&path) {
                Ok(Some(device)) => found.push(device),
                Ok(None) => self
                    .host
                    .log(4, &format!("probe {path}: no identify reply")),
                Err(e) => self.host.log(2, &format!("probe {path} failed: {e}")),
            }
        }

        // —— BLE 广播指纹 ——
        let ble_result: Result<Value, PluginError> = if skip_ble {
            Err(PluginError::Other(
                "skipped by pre-permission notice".into(),
            ))
        } else {
            self.host
                .call("ble", "scan", &json!({ "timeout_ms": 3000 }))
        };
        // BLE 错误透传前端（如 [E3201] 权限被拒），由设备页呈现权限通知并中止 BLE 功能
        let mut ble_error: Option<String> = None;
        match ble_result {
            Ok(ble) => {
                for dev in ble
                    .get("devices")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                {
                    let name = dev.get("name").and_then(Value::as_str);
                    let services: Vec<String> = dev
                        .get("services")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    if let Some(fp) = self.fingerprints.by_ble(name, &services) {
                        let device = self.report_ble_device(fp, &dev);
                        found.push(device);
                    }
                }
            }
            Err(e) => {
                if !skip_ble {
                    ble_error = Some(e.to_string());
                }
                self.host.log(2, &format!("ble scan skipped: {e}"));
            }
        }

        Ok(json!({ "devices": found, "ble_error": ble_error }))
    }

    /// 打开候选端口 → identify → 等待 hello → 指纹匹配 → 上报并关闭。
    fn probe_port(&mut self, path: &str) -> Result<Option<Value>, PluginError> {
        let opened =
            match self
                .host
                .call("serial", "open", &json!({ "path": path, "baud": 115200 }))
            {
                Ok(v) => v,
                Err(e) => return Err(PluginError::Host(format!("open {path}: {e}"))),
            };
        let session = opened.get("session").and_then(Value::as_u64).unwrap_or(0) as u32;
        self.shared.lock().unwrap().sessions.insert(
            session,
            SessionState {
                parser: LineParser::new(),
                device_label: path.to_string(),
                path: path.to_string(),
                hello: None,
                console_board: None,
                streaming: false,
            },
        );

        // 双协议探测：JSON identify（mock/调试协议）+ 文本 info（真实固件控制台）。
        // 真实固件（SlimeVR-Tracker-nRF 系）以 `Board: <board_target>` 行应答 info。
        for cmd in [r#"{"cmd":"identify"}"#, "info"] {
            let _ = self.host.call(
                "serial",
                "write",
                &json!({
                    "session": session,
                    "data": cmd,
                    "encoding": "text",
                    "append": "lf",
                }),
            );
        }

        // 轮询等待 hello（JSON）或 Board: 行（文本控制台）
        let mut waited = 0u64;
        let (hello, board) = loop {
            let (hello, board) = {
                let shared = self.shared.lock().unwrap();
                let s = shared.sessions.get(&session);
                (
                    s.and_then(|s| s.hello.clone()),
                    s.and_then(|s| s.console_board.clone()),
                )
            };
            if hello.is_some() || board.is_some() {
                break (hello, board);
            }
            if waited >= IDENTIFY_TIMEOUT_MS {
                break (None, None);
            }
            std::thread::sleep(Duration::from_millis(IDENTIFY_POLL_MS));
            waited += IDENTIFY_POLL_MS;
        };

        let _ = self
            .host
            .call("serial", "close", &json!({ "session": session }));
        self.shared.lock().unwrap().sessions.remove(&session);

        // JSON hello 优先；否则用控制台 Board: 行合成识别结果
        let (hw, port_extra) = match hello {
            Some(hello) => {
                let hw = hello
                    .get("hw")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                (
                    hw,
                    json!({
                        "path": path,
                        "fw": hello.get("fw").cloned().unwrap_or(Value::Null),
                        "proto": hello.get("proto").cloned().unwrap_or(Value::Null),
                    }),
                )
            }
            None => match board {
                Some(board) => (
                    board.clone(),
                    json!({ "path": path, "board": board, "proto": "console" }),
                ),
                None => return Ok(None),
            },
        };
        let device = match self.fingerprints.by_hw_name(&hw) {
            Some(fp) => {
                self.device_entry(fp, &format!("serial:{path}"), vec!["serial"], port_extra)
            }
            None => json!({
                "id": format!("serial:{path}"),
                "kind": Value::Null,
                "label": format!("Unknown device ({hw})"),
                "transports": ["serial"],
                "capabilities": [],
                "extra": port_extra,
            }),
        };
        self.host
            .call("devices", "report", &json!({ "device": device }))?;
        Ok(Some(device))
    }

    fn report_ble_device(&self, fp: &Fingerprint, ble: &Value) -> Value {
        let addr = ble
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let device = self.device_entry(
            fp,
            &format!("ble:{addr}"),
            vec!["ble"],
            json!({
                "address": addr,
                "rssi": ble.get("rssi").cloned().unwrap_or(Value::Null),
            }),
        );
        if let Err(e) = self
            .host
            .call("devices", "report", &json!({ "device": device }))
        {
            self.host.log(2, &format!("devices.report failed: {e}"));
        }
        device
    }

    fn device_entry(
        &self,
        fp: &Fingerprint,
        id: &str,
        transports: Vec<&str>,
        extra: Value,
    ) -> Value {
        json!({
            "id": id,
            "kind": fp.kind,
            "label": fp.label,
            "transports": transports,
            "capabilities": fp.capabilities,
            "extra": extra,
        })
    }

    /// 建立持续会话：打开串口（或 BLE，path 为 `ble:<address>`）并启动数据流解析与批量推送。
    fn connect(&mut self, args: &Value) -> Result<Value, PluginError> {
        let path = req_str(args, "path")?;
        let baud = args.get("baud").and_then(Value::as_u64).unwrap_or(115_200);
        let opened = if let Some(address) = path.strip_prefix("ble:") {
            self.host
                .call("ble", "connect", &json!({ "address": address }))?
        } else {
            self.host
                .call("serial", "open", &json!({ "path": path, "baud": baud }))?
        };
        let session = opened.get("session").and_then(Value::as_u64).unwrap_or(0) as u32;

        // 从已注册设备中找显示名（串口按 extra.path，BLE 按 extra.address）
        let label = self
            .host
            .call("devices", "list", &json!({}))
            .ok()
            .and_then(|list| {
                let ble_addr = path.strip_prefix("ble:");
                list.as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .find_map(|d| {
                        let extra = d.get("extra")?;
                        let hit = match ble_addr {
                            Some(addr) => {
                                extra.get("address").and_then(Value::as_str) == Some(addr)
                            }
                            None => {
                                extra.get("path").and_then(Value::as_str) == Some(path.as_str())
                            }
                        };
                        hit.then(|| {
                            d.get("label")
                                .and_then(Value::as_str)
                                .unwrap_or(path.as_str())
                                .to_owned()
                        })
                    })
            })
            .unwrap_or_else(|| path.clone());

        self.shared.lock().unwrap().sessions.insert(
            session,
            SessionState {
                parser: LineParser::new(),
                device_label: label,
                path: path.clone(),
                hello: None,
                console_board: None,
                streaming: true,
            },
        );
        self.ensure_flusher();
        Ok(json!({ "session": session, "path": path }))
    }

    fn disconnect(&mut self, args: &Value) -> Result<Value, PluginError> {
        let session = req_u64(args, "session")? as u32;
        self.shared.lock().unwrap().sessions.remove(&session);
        let service = if session >= BLE_SESSION_BASE {
            "ble"
        } else {
            "serial"
        };
        self.host
            .call(service, "close", &json!({ "session": session }))
    }

    /// 进入 UF2 bootloader：下发 dfu 命令 → 断开 → 等待 MSC 卷出现。
    ///
    /// 真实固件（SlimeVR-Tracker-nRF 系）的文本控制台命令为 `dfu`
    /// （GPREGRET=0x57 后冷复位进 Adafruit UF2 bootloader）；mock 同时兼容
    /// JSON `{"cmd":"dfu"}` 与该文本命令。
    fn enter_bootloader(&mut self, args: &Value) -> Result<Value, PluginError> {
        let session = req_u64(args, "session")? as u32;
        for cmd in ["dfu", r#"{"cmd":"dfu"}"#] {
            let _ = self.host.call(
                "serial",
                "write",
                &json!({
                    "session": session,
                    "data": cmd,
                    "encoding": "text",
                    "append": "lf",
                }),
            );
        }
        std::thread::sleep(Duration::from_millis(300));
        let _ = self
            .host
            .call("serial", "close", &json!({ "session": session }));
        self.shared.lock().unwrap().sessions.remove(&session);

        let timeout = args
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(10_000);
        self.host
            .call("uf2", "wait", &json!({ "timeout_ms": timeout }))
    }

    /// 校验 UF2 并附带 family 匹配提示（`args.kind` 提供设备类型时）。
    fn validate_uf2(&mut self, args: &Value) -> Result<Value, PluginError> {
        let mut summary = self.host.call("uf2", "validate", args)?;
        if let (Some(kind), Some(family)) = (
            args.get("kind").and_then(Value::as_str),
            summary
                .get("family_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ) {
            let expected = self
                .fingerprints
                .devices
                .iter()
                .find(|f| f.kind == kind)
                .and_then(|f| f.uf2_family.clone());
            let matches = expected.as_deref() == Some(family.as_str());
            if let Some(o) = summary.as_object_mut() {
                o.insert("expected_family".into(), json!(expected));
                o.insert("family_match".into(), json!(matches));
            }
        }
        Ok(summary)
    }

    /// 串口接收事件 → 行切割 → 协议分发。
    fn on_serial_rx(&mut self, payload: &Value) {
        let Some(session) = payload
            .get("session")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
        else {
            return;
        };
        let Some(hex) = payload.get("hex").and_then(Value::as_str) else {
            return;
        };
        let Ok(bytes) = hex_decode(hex) else { return };

        let mut shared = match self.shared.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let Some(state) = shared.sessions.get_mut(&session) else {
            return;
        };
        let lines = state.parser.push(&bytes);
        let label = state.device_label.clone();
        let streaming = state.streaming;

        // status 卡片先收集，释放锁后再 emit（emit 走 host_call，避免持锁跨 FFI）。
        let mut status_events: Vec<Value> = Vec::new();
        for line in lines {
            let Some(msg) = protocol::parse_line(&line) else {
                // 真实固件控制台文本行（非 JSON）：
                // - `Board: <board_target>`（info 应答）→ 识别证据
                // - `Battery: 87.42% (...)`（battery 命令/日志）→ 状态解读
                if let Some(board) = line.strip_prefix("Board:") {
                    if let Some(s) = shared.sessions.get_mut(&session) {
                        s.console_board = Some(board.trim().to_string());
                    }
                } else if streaming {
                    if let Some(battery) = protocol::parse_battery_line(&line) {
                        let msg = json!({ "type": "status", "battery": battery });
                        let cards = rules::interpret(&self.rules, &msg);
                        status_events.push(json!({
                            "device": label,
                            "raw": msg,
                            "cards": cards,
                        }));
                    }
                }
                continue;
            };
            match msg.get("type").and_then(Value::as_str) {
                Some("hello") => {
                    if let Some(s) = shared.sessions.get_mut(&session) {
                        s.hello = Some(msg);
                    }
                }
                Some("imu") if streaming => {
                    let mut m = msg;
                    m.as_object_mut()
                        .map(|o| o.insert("device".into(), json!(label)));
                    shared.imu_buf.push(m);
                }
                Some("rf") if streaming => {
                    let mut m = msg;
                    m.as_object_mut()
                        .map(|o| o.insert("device".into(), json!(label)));
                    shared.rf_buf.push(m);
                }
                Some("status") => {
                    let cards = rules::interpret(&self.rules, &msg);
                    status_events.push(json!({
                        "device": label,
                        "raw": msg,
                        "cards": cards,
                    }));
                }
                _ => {}
            }
        }
        drop(shared);
        for ev in status_events {
            let _ = self.host.emit("zannen.debugger/status", ev);
        }
    }

    /// 启动批量推送线程（幂等）。
    fn ensure_flusher(&mut self) {
        let mut shared = self.shared.lock().unwrap();
        if shared.flusher_started {
            return;
        }
        shared.flusher_started = true;
        let shared_arc = self.shared.clone();
        let host = self.host;
        drop(shared);

        std::thread::Builder::new()
            .name("zannen-debugger-flusher".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(FLUSH_INTERVAL_MS));
                let (imu, rf) = {
                    let Ok(mut shared) = shared_arc.lock() else {
                        continue;
                    };
                    let imu = take_capped(&mut shared.imu_buf);
                    let rf = take_capped(&mut shared.rf_buf);
                    (imu, rf)
                };
                if !imu.is_empty() {
                    if let Err(e) =
                        host.emit("zannen.debugger/imu.batch", json!({ "samples": imu }))
                    {
                        host.log(2, &format!("emit imu.batch failed: {e}"));
                    }
                }
                if !rf.is_empty() {
                    let _ = host.emit("zannen.debugger/rf.batch", json!({ "samples": rf }));
                }
            })
            .expect("spawn flusher");
    }
}

fn take_capped(buf: &mut Vec<Value>) -> Vec<Value> {
    if buf.len() > BATCH_CAP {
        let keep = BATCH_CAP / 2;
        buf.drain(..buf.len() - keep);
    }
    std::mem::take(buf)
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, ()> {
    let cleaned: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err(());
    }
    cleaned
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let s = std::str::from_utf8(pair).map_err(|_| ())?;
            u8::from_str_radix(s, 16).map_err(|_| ())
        })
        .collect()
}

fn req_str(args: &Value, key: &str) -> Result<String, PluginError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| PluginError::BadArgs(format!("missing string arg: {key}")))
}

fn req_u64(args: &Value, key: &str) -> Result<u64, PluginError> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| PluginError::BadArgs(format!("missing u64 arg: {key}")))
}

export_plugin!(DebuggerPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_consistent() {
        let m = DebuggerPlugin::manifest();
        assert_eq!(m.id, "zannen.debugger");
        assert_eq!(m.api, ZANNEN_ABI_VERSION);
        assert_eq!(m.routes.len(), 6);
        // 与 plugin.toml 交叉一致性由宿主加载时校验；此处保底前端入口声明。
        assert!(m.frontend.unwrap().entry.ends_with("index.js"));
    }

    #[test]
    fn hex_decode_works() {
        assert_eq!(hex_decode("00abff").unwrap(), vec![0x00, 0xAB, 0xFF]);
        assert!(hex_decode("abc").is_err());
    }

    #[test]
    fn take_capped_drops_oldest_when_over_cap() {
        let mut buf: Vec<Value> = (0..10_000).map(|i| json!(i)).collect();
        let out = take_capped(&mut buf);
        // 超上限（4000）后只保留最新一半（2000）
        assert_eq!(out.len(), 2000);
        assert_eq!(out[0], json!(8000));
        assert!(buf.is_empty());
        // 未超上限全量取出
        let mut buf: Vec<Value> = (0..10).map(|i| json!(i)).collect();
        assert_eq!(take_capped(&mut buf).len(), 10);
    }
}
