//! 模拟传输层（`mock-transport` feature）：虚拟 Zannen 设备。
//!
//! 串口侧提供 `mock://zannen-smol` / `mock://zannen-smol-air` / `mock://zannen-dongle`
//! 三个虚拟串口，BLE 侧提供三台虚拟外设（见 `BLE_DEVICES`，广播含 NUS service
//! UUID，地址形如 `00:11:22:33:FE:0x`）。数据面行为与调试数据协议一致
//! （JSON Lines，见 docs/MODULE-DEBUGGER.md §数据协议）：
//! - 打开/identify → `hello` 行（硬件型号、固件版本）
//! - 100Hz `imu`（四元数 + 加速度 + 陀螺仪，带缓动与噪声）
//! - 10Hz `rf`（RSSI 波动）
//! - 1Hz `status`（电池缓慢下降等）
//! - JSON 命令：`{"cmd":"identify"|"ping"|"dfu"|"calibrate"}`
//!
//! 同时模拟真实固件（SlimeVR-Tracker-nRF 系）的文本控制台子集，便于无硬件验证
//! 双协议识别路径：
//! - `info` → 横幅 + `Board:`/`SOC:`/`Target:` 行（board target 与真实固件一致）
//! - `battery` → `Battery: NN.NN% (Raw ...)` 文本行
//! - `dfu`（纯文本）→ 与 JSON dfu 等效：设备消失（模拟复位进 bootloader）
//!
//! 串口会话数据发 `serial.rx` / `serial.closed`，BLE 会话发 `ble.rx` / `ble.closed`，
//! 由 `run_device` 的 topic 参数区分。
//!
//! 用途：无硬件演示、前端开发、集成测试。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::serial::{hex_encode, PortInfo};
use crate::events::EventBus;

const MODELS: &[(&str, &str, f32, &str, &str)] = &[
    // (路径后缀, hello 里的 hw 名, 初始电量, board target, SOC)
    // board/SOC 与真实固件 Zephyr 配置一致（SlimeVR-Tracker-nRF*/boards/zannen）
    (
        "zannen-smol",
        "zannen-smol",
        87.0,
        "zannensmol_uf2",
        "nrf52840",
    ),
    (
        "zannen-smol-air",
        "zannen-smol-air",
        92.0,
        "zannensmolair_uf2",
        "nrf52833",
    ),
    (
        "zannen-dongle",
        "zannen-dongle",
        100.0,
        "zannendongle_uf2",
        "nrf52840",
    ),
];

/// 虚拟 BLE 外设表：(地址, 广播名, MODELS 下标, 基准 RSSI)。
const BLE_DEVICES: &[(&str, &str, usize, i32)] = &[
    ("00:11:22:33:FE:01", "Zannen-Smol", 0, -48),
    ("00:11:22:33:FE:02", "Zannen-SmolAir", 1, -57),
    ("00:11:22:33:FE:03", "Zannen-Dongle", 2, -66),
];

pub fn virtual_ports() -> Vec<PortInfo> {
    MODELS
        .iter()
        .map(|(suffix, ..)| PortInfo {
            path: format!("mock://{suffix}"),
            vid: Some("0x1915".into()),
            pid: Some("0x520F".into()),
            product: Some(format!("Zannen {suffix} (mock)")),
            manufacturer: Some("Zannen (simulated)".into()),
            serial_number: Some("MOCK0001".into()),
            mock: true,
        })
        .collect()
}

/// 虚拟 BLE 广播记录（结构与真实扫描条目一致，多一个 `mock` 标记）。
pub fn mock_ble_advertisements() -> Vec<Value> {
    BLE_DEVICES
        .iter()
        .enumerate()
        .map(|(i, (addr, name, _, rssi))| {
            json!({
                "id": format!("mock-ble-{}", i + 1),
                "address": addr,
                "name": name,
                "rssi": rssi,
                "services": [super::ble::NUS_SERVICE],
                "mock": true,
            })
        })
        .collect()
}

/// 虚拟设备生成线程句柄：写入通道 + 取消标志。
struct DeviceHandle {
    cancel: Arc<AtomicBool>,
    cmd_tx: Sender<Vec<u8>>,
}

impl DeviceHandle {
    /// 启动 `run_device` 生成线程，数据发到 `rx_topic`，结束发 `closed_topic`。
    fn spawn(
        session_id: u32,
        hw: &'static str,
        battery0: f32,
        board: &'static str,
        soc: &'static str,
        bus: EventBus,
        rx_topic: &'static str,
        closed_topic: &'static str,
    ) -> Option<Self> {
        let (cmd_tx, cmd_rx) = channel::<Vec<u8>>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_thread = cancel.clone();

        std::thread::Builder::new()
            .name(format!("mock-dev-{session_id}"))
            .spawn(move || {
                run_device(
                    session_id,
                    hw,
                    battery0,
                    board,
                    soc,
                    bus,
                    cmd_rx,
                    cancel_thread,
                    rx_topic,
                    closed_topic,
                )
            })
            .ok()?;

        Some(Self { cancel, cmd_tx })
    }

    fn write(&self, bytes: &[u8]) {
        let _ = self.cmd_tx.send(bytes.to_vec());
    }

    fn close(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct MockSession(DeviceHandle);

impl MockSession {
    /// 若 `path` 是 mock 虚拟端口则创建会话（含生成线程），否则返回 None。
    pub fn open(session_id: u32, path: &str, bus: EventBus) -> Option<Self> {
        let suffix = path.strip_prefix("mock://")?;
        let (_, hw, battery0, board, soc) = *MODELS.iter().find(|(s, ..)| *s == suffix)?;

        let handle = DeviceHandle::spawn(
            session_id,
            hw,
            battery0,
            board,
            soc,
            bus,
            "serial.rx",
            "serial.closed",
        )?;
        log::info!("mock device {path} attached as session {session_id}");
        Some(Self(handle))
    }

    pub fn write(&self, bytes: &[u8]) {
        self.0.write(bytes);
    }

    pub fn close(&self) {
        self.0.close();
    }
}

pub struct MockBleSession(DeviceHandle);

impl MockBleSession {
    /// 若 `address` 命中虚拟 BLE 外设表则创建会话（含生成线程），否则返回 None。
    /// 数据经 `ble.rx` 事件发出，与真实 NUS 会话行为一致。
    pub fn open(session_id: u32, address: &str, bus: EventBus) -> Option<Self> {
        let (_, _, model_idx, _) = *BLE_DEVICES
            .iter()
            .find(|(addr, _, _, _)| addr.eq_ignore_ascii_case(address))?;
        let (_, hw, battery0, board, soc) = MODELS[model_idx];

        let handle = DeviceHandle::spawn(
            session_id,
            hw,
            battery0,
            board,
            soc,
            bus,
            "ble.rx",
            "ble.closed",
        )?;
        log::info!("mock BLE {address} attached as session {session_id}");
        Some(Self(handle))
    }

    pub fn write(&self, bytes: &[u8]) {
        self.0.write(bytes);
    }

    pub fn close(&self) {
        self.0.close();
    }
}

fn emit_line(bus: &EventBus, session: u32, topic: &str, line: &str) {
    bus.publish(
        topic,
        json!({ "session": session, "hex": hex_encode(format!("{line}\n").as_bytes()) }),
    );
}

#[allow(clippy::too_many_arguments)]
fn run_device(
    session: u32,
    hw: &str,
    battery0: f32,
    board: &str,
    soc: &str,
    bus: EventBus,
    cmd_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    cancel: Arc<AtomicBool>,
    rx_topic: &'static str,
    closed_topic: &'static str,
) {
    let start = Instant::now();
    let mut battery = battery0;
    let mut last_imu = Instant::now() - Duration::from_millis(10);
    let mut last_rf = Instant::now() - Duration::from_millis(100);
    let mut last_status = Instant::now() - Duration::from_secs(1);

    let hello = |bus: &EventBus| {
        emit_line(
            bus,
            session,
            rx_topic,
            &json!({"type": "hello", "hw": hw, "fw": "1.4.2", "proto": 1}).to_string(),
        );
    };
    hello(&bus);

    while !cancel.load(Ordering::Relaxed) {
        let now = Instant::now();
        let t = start.elapsed().as_secs_f32();
        let ts = start.elapsed().as_millis() as u64;

        // 命令处理（非阻塞）：JSON 命令 + 真实固件文本控制台子集
        while let Ok(cmd) = cmd_rx.try_recv() {
            let text = String::from_utf8_lossy(&cmd).trim().to_string();
            // 文本控制台命令（真实固件协议子集，大小写不敏感）
            match text.to_lowercase().as_str() {
                "info" => {
                    emit_line(
                        &bus,
                        session,
                        rx_topic,
                        &format!("*** Zannen {hw} (mock) ***"),
                    );
                    emit_line(&bus, session, rx_topic, &format!("Board: {board}"));
                    emit_line(&bus, session, rx_topic, &format!("SOC: {soc}"));
                    emit_line(&bus, session, rx_topic, &format!("Target: {board}/{soc}"));
                    continue;
                }
                "battery" => {
                    emit_line(
                        &bus,
                        session,
                        rx_topic,
                        &format!("Battery: {battery:.2}% (Raw {battery:.2}%, 3890 mV)"),
                    );
                    continue;
                }
                "dfu" => {
                    emit_line(&bus, session, rx_topic, "Entering UF2 bootloader");
                    return; // 模拟进入 bootloader：设备消失
                }
                _ => {}
            }
            let parsed: serde_json::Value =
                serde_json::from_str(&text).unwrap_or(json!({"cmd": "raw"}));
            match parsed.get("cmd").and_then(|c| c.as_str()) {
                Some("identify") => hello(&bus),
                Some("ping") => emit_line(
                    &bus,
                    session,
                    rx_topic,
                    &json!({"type": "pong", "ts": ts}).to_string(),
                ),
                Some("calibrate") => emit_line(
                    &bus,
                    session,
                    rx_topic,
                    &json!({"type": "ack", "cmd": "calibrate", "msg": "zero offset updated"})
                        .to_string(),
                ),
                Some("dfu") => {
                    emit_line(
                        &bus,
                        session,
                        rx_topic,
                        &json!({"type": "dfu", "msg": "entering UF2 bootloader"}).to_string(),
                    );
                    return; // 模拟进入 bootloader：设备消失
                }
                _ => emit_line(
                    &bus,
                    session,
                    rx_topic,
                    &json!({"type": "ack", "echo": text.trim()}).to_string(),
                ),
            }
        }

        if now.duration_since(last_imu) >= Duration::from_millis(10) {
            last_imu = now;
            // 绕固定缓动轴旋转的四元数 + 轻微摆动
            let angle = t * 0.6;
            let wobble = (t * 2.1).sin() * 0.08;
            let half = (angle + wobble) * 0.5;
            let (s, c) = half.sin_cos();
            let ax = [0.15f32, 0.25, 0.956];
            let quat = [c, ax[0] * s, ax[1] * s, ax[2] * s];
            let noise = |i: usize| ((t * 13.7 + i as f32 * 7.3).sin()) * 0.02;
            emit_line(
                &bus,
                session,
                rx_topic,
                &json!({
                    "type": "imu", "ts": ts,
                    "quat": quat,
                    "accel": [noise(0), noise(1), 1.0 + noise(2)],
                    "gyro": [noise(3) * 20.0, noise(4) * 20.0, 34.4 + noise(5) * 4.0],
                })
                .to_string(),
            );
        }

        if now.duration_since(last_rf) >= Duration::from_millis(100) {
            last_rf = now;
            let rssi = -52.0 + (t * 0.9).sin() * 8.0 + (t * 7.7).cos() * 1.5;
            emit_line(
                &bus,
                session,
                rx_topic,
                &json!({"type": "rf", "ts": ts, "rssi": (rssi * 10.0).round() / 10.0, "ch": 37})
                    .to_string(),
            );
        }

        if now.duration_since(last_status) >= Duration::from_secs(1) {
            last_status = now;
            battery = (battery - 0.005).max(5.0);
            emit_line(
                &bus,
                session,
                rx_topic,
                &json!({
                    "type": "status",
                    "battery": (battery * 10.0).round() / 10.0,
                    "charging": false,
                    "fw": "1.4.2",
                    "imu": "ok",
                    "rf_link": if battery > 20.0 { "good" } else { "weak" },
                    "uptime_s": start.elapsed().as_secs(),
                })
                .to_string(),
            );
        }

        std::thread::sleep(Duration::from_millis(2));
    }
    bus.publish(closed_topic, json!({ "session": session }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ble_advertisements_shape() {
        let ads = mock_ble_advertisements();
        assert_eq!(ads.len(), 3);
        for (i, ad) in ads.iter().enumerate() {
            assert_eq!(ad["id"], json!(format!("mock-ble-{}", i + 1)));
            assert_eq!(ad["mock"], json!(true));
            assert!(ad["name"].as_str().unwrap().starts_with("Zannen-"));
            assert!(ad["rssi"].is_i64());
            assert!(ad["services"]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| u == &json!(super::super::ble::NUS_SERVICE)));
        }
        // 地址唯一且尾字节递增
        let mut addrs: Vec<&str> = ads.iter().map(|a| a["address"].as_str().unwrap()).collect();
        addrs.sort();
        addrs.dedup();
        assert_eq!(addrs.len(), 3);
        assert_eq!(ads[0]["address"], json!("00:11:22:33:FE:01"));
        assert_eq!(ads[2]["address"], json!("00:11:22:33:FE:03"));
    }

    #[test]
    fn ble_session_rejects_unknown_address() {
        let bus = EventBus::new(8);
        assert!(MockBleSession::open(10_000, "AA:BB:CC:DD:EE:FF", bus).is_none());
    }

    #[test]
    fn ble_session_matches_address_case_insensitive() {
        let bus = EventBus::new(8);
        let session = MockBleSession::open(10_000, "00:11:22:33:fe:03", bus);
        assert!(session.is_some());
        session.unwrap().close();
    }
}
