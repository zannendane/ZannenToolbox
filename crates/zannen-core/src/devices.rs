//! 设备注册表：聚合各传输层（串口 / BLE / 插件上报）发现的设备，
//! 归并为统一的设备视图供前端顶栏与插件查询。
//!
//! 设备指纹识别（判断是否为 ZannenSmol / SmolAir / Dongle）由插件依据
//! 自己的 `devices.json` 完成；注册表只负责存储与去重。

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::EventBus;

/// 一台已识别（或候选）设备。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// 稳定 id：`serial:<端口路径>` 或 `ble:<MAC>`。
    pub id: String,
    /// 指纹匹配结果，如 `zannen-smol`；未识别为 None。
    #[serde(default)]
    pub kind: Option<String>,
    /// 人类可读名。
    pub label: String,
    /// 可用传输通道，如 `["serial", "ble"]`。
    #[serde(default)]
    pub transports: Vec<String>,
    /// 能力标签，如 `["imu", "rf", "uf2"]`。
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// 传输层原始信息（VID/PID、RSSI 等）。
    #[serde(default)]
    pub extra: Value,
}

#[derive(Default)]
pub struct DeviceRegistry {
    inner: RwLock<HashMap<String, DeviceInfo>>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入或更新设备，并按需发布 `device.found` / `device.update`。
    pub fn upsert(&self, bus: &EventBus, info: DeviceInfo) {
        let is_new = {
            let mut map = self.inner.write();
            let is_new = !map.contains_key(&info.id);
            map.insert(info.id.clone(), info.clone());
            is_new
        };
        bus.publish(
            if is_new {
                "device.found"
            } else {
                "device.update"
            },
            serde_json::to_value(&info).unwrap_or(Value::Null),
        );
        log::debug!("device upserted: {}", info.id);
    }

    pub fn remove(&self, bus: &EventBus, id: &str) {
        if self.inner.write().remove(id).is_some() {
            bus.publish("device.lost", serde_json::json!({ "id": id }));
        }
    }

    pub fn get(&self, id: &str) -> Option<DeviceInfo> {
        self.inner.read().get(id).cloned()
    }

    pub fn list(&self) -> Vec<DeviceInfo> {
        self.inner.read().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn demo(id: &str) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            kind: Some("zannen-smol".into()),
            label: "ZannenSmol".into(),
            transports: vec!["serial".into()],
            capabilities: vec!["imu".into()],
            extra: json!({"vid": "0x1915"}),
        }
    }

    #[tokio::test]
    async fn upsert_emits_found_then_update() {
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        let reg = DeviceRegistry::new();
        reg.upsert(&bus, demo("serial:A"));
        reg.upsert(&bus, demo("serial:A"));
        assert_eq!(reg.list().len(), 1);
        assert_eq!(rx.recv().await.unwrap().topic, "device.found");
        assert_eq!(rx.recv().await.unwrap().topic, "device.update");
        reg.remove(&bus, "serial:A");
        assert_eq!(rx.recv().await.unwrap().topic, "device.lost");
        assert!(reg.list().is_empty());
    }
}
