//! 事件总线：宿主服务与插件之间的异步数据通道。
//!
//! 主题约定（`docs/ARCHITECTURE.md` 有完整目录）：
//! - `device.found` / `device.lost` / `device.update`：设备注册表变化
//! - `serial.rx`：串口接收块（`{session, hex}`）
//! - `uf2.progress`：刷写进度
//! - `ble.found`：BLE 扫描到的广播
//! - 插件自定义主题（如 `zannen.debugger/imu.batch`）原样透传到前端
//!
//! 高频主题由 shell 层按 30/60Hz 批量转发给前端，bus 本身不做节流。

use serde_json::Value;
use tokio::sync::broadcast;

/// 一条总线事件。
#[derive(Debug, Clone)]
pub struct Event {
    pub topic: String,
    pub payload: Value,
}

/// 多生产者多消费者事件总线（tokio broadcast，有界容量，慢消费者丢旧事件并记录）。
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// 发布事件。无订阅者时静默丢弃。
    pub fn publish(&self, topic: impl Into<String>, payload: Value) {
        let event = Event {
            topic: topic.into(),
            payload,
        };
        // Err(SendError) 仅表示没有任何接收者，属正常情况。
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn pub_sub() {
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        bus.publish("device.found", json!({"id": "serial:/dev/ttyUSB0"}));
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.topic, "device.found");
        assert_eq!(ev.payload["id"], "serial:/dev/ttyUSB0");
    }

    #[tokio::test]
    async fn no_subscriber_is_ok() {
        let bus = EventBus::new(8);
        bus.publish("noop", json!(null));
    }
}
