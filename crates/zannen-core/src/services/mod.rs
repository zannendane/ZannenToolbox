//! 宿主服务调度层：插件经 `host_call` 以 `<service>.<method>` 访问的全部能力。
//!
//! 服务目录（参数与返回值均为 JSON）：
//! - `serial.list {}` / `serial.open {path, baud}` / `serial.write {session, data, encoding, append}` / `serial.close {session}`
//! - `ble.scan {timeout_ms}` / `ble.connect {address}` / `ble.write {session, data, encoding, append}` / `ble.close {session}`
//! - `uf2.volumes {}` / `uf2.validate {path}` / `uf2.flash {path, volume, job?}`（异步，进度走 `uf2.progress` 事件）/ `uf2.wait {timeout_ms?}`
//! - `dfu.upload {path, baud?, image_path, job?}` / `dfu.confirm {path, baud?, hash?}` / `dfu.reset {path, baud?}`（MCUboot SMP 串行，进度走 `dfu.progress` 事件）
//! - `devices.list {}` / `devices.report {device}` / `devices.remove {id}`
//! - `events.emit {topic, payload}`（插件 → 总线 → 前端）
//! - `log.write {level, msg}`
//!
//! 事件主题目录：
//! - `serial.rx {session, hex}` / `serial.error` / `serial.closed` / `serial.plugged {path}` / `serial.unplugged {path}`
//! - `ble.rx {session, hex}` / `ble.error` / `ble.closed` / `ble.found {device}`
//! - `uf2.progress|done|error`、`dfu.progress|done|error`
//! - `device.found|update|lost`
//! - BLE 会话 id ≥ 10000，与串口会话（1 起）区分。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::{DeviceRegistry, EventBus};

pub mod ble;
pub mod dfu;
#[cfg(feature = "mock-transport")]
pub mod mock;
pub mod serial;
pub mod uf2;

/// 全部宿主服务的集合。进程内单例（见 `crate::host`）。
pub struct ServiceDispatcher {
    pub bus: EventBus,
    pub registry: Arc<DeviceRegistry>,
    pub serial: serial::SerialService,
    pub ble: ble::BleService,
    pub dfu: dfu::DfuService,
}

impl ServiceDispatcher {
    pub fn new() -> Arc<Self> {
        let bus = EventBus::default();
        Arc::new(Self {
            serial: serial::SerialService::new(bus.clone()),
            ble: ble::BleService::new(bus.clone()),
            dfu: dfu::DfuService::new(bus.clone()),
            bus,
            registry: Arc::new(DeviceRegistry::new()),
        })
    }

    /// 路由一次宿主调用。
    pub fn call(&self, service: &str, method: &str, args: Value) -> Result<Value, String> {
        // 错误统一带识别码：已含码的透传，否则包服务通用码（docs/ERROR-CODES.md）
        self.dispatch(service, method, args).map_err(|e| {
            if e.contains("[ZTB-") {
                e
            } else {
                format!("[{}] {}", service_code(service), e)
            }
        })
    }

    fn dispatch(&self, service: &str, method: &str, args: Value) -> Result<Value, String> {
        match (service, method) {
            ("serial", "list") => self.serial.list(),
            ("serial", "open") => {
                let path = req_str(&args, "path")?;
                let baud = args.get("baud").and_then(Value::as_u64).unwrap_or(115_200) as u32;
                self.serial.open(&path, baud)
            }
            ("serial", "write") => {
                let session = req_u32(&args, "session")?;
                let data = req_str(&args, "data")?;
                let encoding = opt_str(&args, "encoding", "text");
                let append = opt_str(&args, "append", "none");
                self.serial.write(session, &data, &encoding, &append)
            }
            ("serial", "close") => self.serial.close(req_u32(&args, "session")?),

            ("ble", "scan") => {
                let timeout = args
                    .get("timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(5_000);
                self.ble.scan(timeout)
            }
            ("ble", "connect") => {
                let address = req_str(&args, "address")?;
                self.ble.connect(&address)
            }
            ("ble", "write") => {
                let session = req_u32(&args, "session")?;
                let data = req_str(&args, "data")?;
                let encoding = opt_str(&args, "encoding", "text");
                let append = opt_str(&args, "append", "none");
                let mut bytes = match encoding.as_str() {
                    "hex" => serial::hex_decode(&data)?,
                    "text" => data.into_bytes(),
                    other => return Err(format!("unknown encoding: {other}")),
                };
                match append.as_str() {
                    "lf" => bytes.push(b'\n'),
                    "crlf" => bytes.extend_from_slice(b"\r\n"),
                    "none" => {}
                    other => return Err(format!("unknown append: {other}")),
                }
                self.ble.write(session, &bytes)
            }
            ("ble", "close") => self.ble.close(req_u32(&args, "session")?),

            // MCUboot SMP 串行 DFU：长任务后台线程，进度走 dfu.* 事件
            ("dfu", "upload") => {
                let path = req_str(&args, "path")?;
                let baud = args.get("baud").and_then(Value::as_u64).unwrap_or(115_200) as u32;
                let image_path = req_str(&args, "image_path")?;
                let job = args
                    .get("job")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("dfu-{}", std::process::id()));
                let dfu = self.dfu.clone();
                let job_ret = job.clone();
                std::thread::Builder::new()
                    .name(format!("dfu-upload-{job}"))
                    .spawn(move || {
                        if let Err(e) =
                            dfu.upload(&job, &path, baud, std::path::Path::new(&image_path))
                        {
                            dfu.publish_error(&job, &e.to_string());
                        }
                    })
                    .map_err(|e| format!("spawn dfu thread: {e}"))?;
                Ok(json!({ "job": job_ret, "started": true }))
            }
            ("dfu", "confirm") => {
                let path = req_str(&args, "path")?;
                let baud = args.get("baud").and_then(Value::as_u64).unwrap_or(115_200) as u32;
                let hash = args.get("hash").and_then(Value::as_str).map(str::to_owned);
                self.dfu
                    .confirm(&path, baud, hash.as_deref())
                    .map_err(|e| e.to_string())
            }
            ("dfu", "reset") => {
                let path = req_str(&args, "path")?;
                let baud = args.get("baud").and_then(Value::as_u64).unwrap_or(115_200) as u32;
                self.dfu.reset(&path, baud).map_err(|e| e.to_string())
            }

            ("uf2", "volumes") => {
                serde_json::to_value(uf2::find_volumes()).map_err(|e| e.to_string())
            }
            ("uf2", "wait") => {
                // 阻塞等待 bootloader 卷出现（设备复位进 bootloader 后由插件调用）。
                let timeout = args
                    .get("timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(10_000);
                let poll = args.get("poll_ms").and_then(Value::as_u64).unwrap_or(400);
                let found = uf2::wait_for_volume(
                    std::time::Duration::from_millis(timeout),
                    std::time::Duration::from_millis(poll),
                );
                match found {
                    Some(v) => Ok(json!({ "found": true, "volume": v })),
                    None => Ok(json!({ "found": false })),
                }
            }
            ("uf2", "validate") => {
                let path = req_str(&args, "path")?;
                let bytes = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
                let summary = uf2::validate(&bytes).map_err(|e| e.to_string())?;
                serde_json::to_value(summary).map_err(|e| e.to_string())
            }
            ("uf2", "flash") => {
                // 刷写为长任务：后台线程执行，进度经事件上报。
                let path = req_str(&args, "path")?;
                let volume = req_str(&args, "volume")?;
                let job = args
                    .get("job")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("uf2-{}", std::process::id()));
                let bus = self.bus.clone();
                let job_ret = job.clone();
                std::thread::Builder::new()
                    .name(format!("uf2-flash-{job}"))
                    .spawn(move || {
                        let result = uf2::flash(
                            &bus,
                            &job,
                            std::path::Path::new(&path),
                            &volume,
                        );
                        match result {
                            Ok(summary) => bus.publish(
                                "uf2.done",
                                json!({ "job": job, "summary": serde_json::to_value(summary).unwrap_or(Value::Null) }),
                            ),
                            Err(e) => bus.publish("uf2.error", json!({ "job": job, "error": e.to_string() })),
                        }
                    })
                    .map_err(|e| format!("spawn flash thread: {e}"))?;
                Ok(json!({ "job": job_ret, "started": true }))
            }

            ("devices", "list") => {
                serde_json::to_value(self.registry.list()).map_err(|e| e.to_string())
            }
            ("devices", "report") => {
                let device_value = args
                    .get("device")
                    .cloned()
                    .ok_or_else(|| "missing device".to_string())?;
                let info: crate::devices::DeviceInfo =
                    serde_json::from_value(device_value).map_err(|e| format!("bad device: {e}"))?;
                let id = info.id.clone();
                self.registry.upsert(&self.bus, info);
                Ok(json!({ "reported": id }))
            }
            ("devices", "remove") => {
                let id = req_str(&args, "id")?;
                self.registry.remove(&self.bus, &id);
                Ok(json!({ "removed": id }))
            }

            ("events", "emit") => {
                let topic = req_str(&args, "topic")?;
                let payload = args.get("payload").cloned().unwrap_or(Value::Null);
                self.bus.publish(topic, payload);
                Ok(json!(true))
            }

            ("log", "write") => {
                let level = args.get("level").and_then(Value::as_u64).unwrap_or(3) as u8;
                let msg = opt_str(&args, "msg", "");
                let target = "plugin";
                match level {
                    1 => log::error!(target: target, "{msg}"),
                    2 => log::warn!(target: target, "{msg}"),
                    4 => log::debug!(target: target, "{msg}"),
                    5 => log::trace!(target: target, "{msg}"),
                    _ => log::info!(target: target, "{msg}"),
                }
                Ok(json!(true))
            }

            _ => Err(format!("unknown service method: {service}.{method}")),
        }
    }
}

/// 服务通用错误码（服务内未给出具体码时的兜底）。
fn service_code(service: &str) -> &'static str {
    match service {
        "serial" => "E3100",
        "ble" => "E3200",
        "uf2" => "E3300",
        "dfu" => "E3400",
        "devices" => "E3500",
        "events" | "log" => "E3900",
        _ => "E3000",
    }
}

fn req_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing string arg: {key}"))
}

fn req_u32(args: &Value, key: &str) -> Result<u32, String> {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .ok_or_else(|| format!("missing u32 arg: {key}"))
}

fn opt_str(args: &Value, key: &str, default: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_method_errors() {
        let d = ServiceDispatcher::new();
        let err = d.call("nope", "nada", json!({})).unwrap_err();
        assert!(err.contains("nope.nada"));
    }

    #[tokio::test]
    async fn events_emit_reaches_bus() {
        let d = ServiceDispatcher::new();
        let mut rx = d.bus.subscribe();
        d.call(
            "events",
            "emit",
            json!({"topic": "t.x", "payload": {"v": 1}}),
        )
        .unwrap();
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.topic, "t.x");
        assert_eq!(ev.payload["v"], 1);
    }

    #[tokio::test]
    async fn devices_report_roundtrip() {
        let d = ServiceDispatcher::new();
        d.call(
            "devices",
            "report",
            json!({"device": {"id": "serial:X", "label": "X", "transports": ["serial"]}}),
        )
        .unwrap();
        let list = d.call("devices", "list", json!({})).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        d.call("devices", "remove", json!({"id": "serial:X"}))
            .unwrap();
        let list = d.call("devices", "list", json!({})).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 0);
    }
}
