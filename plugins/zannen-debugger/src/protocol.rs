//! 调试数据协议解析：JSON Lines 行切割与消息分类。
//!
//! 设备 → 主机的调试通道为 UTF-8 JSON Lines（每行一个 JSON 对象，含 `type` 字段）：
//! - `{"type":"hello","hw":"zannen-smol","fw":"1.4.2","proto":1}` — 识别应答
//! - `{"type":"imu","ts":12345,"quat":[w,x,y,z],"accel":[x,y,z],"gyro":[x,y,z]}`
//! - `{"type":"rf","ts":...,"rssi":-55.2,"ch":37}`
//! - `{"type":"status","battery":87.0,"charging":false,"fw":"1.4.2","imu":"ok","rf_link":"good"}`
//! - `{"type":"pong"|"ack"|"dfu", ...}`
//!
//! 非 JSON 行（日志打印等）原样保留给串口终端展示，解析器忽略。

use serde_json::Value;

/// 行切割器：跨 chunk 缓存半行。
#[derive(Default)]
pub struct LineParser {
    buf: Vec<u8>,
}

impl LineParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加数据，返回全部完整行（不含换行符，容忍 CRLF）。
    pub fn push(&mut self, data: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(data);
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
        // 防御：异常长行（>64KB 无换行）直接丢弃缓冲，避免内存膨胀。
        if self.buf.len() > 64 * 1024 {
            self.buf.clear();
        }
        lines
    }
}

/// 解析一行为协议消息；非 JSON 或无 type 字段返回 None。
pub fn parse_line(line: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(line).ok()?;
    value.get("type").and_then(Value::as_str)?;
    Some(value)
}

/// 解析真实固件控制台的电量行（`Battery: 87.42% (Raw ...)`），返回百分比数值。
///
/// 真实固件（SlimeVR-Tracker-nRF）不推送 JSON status，电量来自
/// `battery` 命令应答与后台日志的文本行。
pub fn parse_battery_line(line: &str) -> Option<f64> {
    let rest = line.strip_prefix("Battery:")?;
    let pct = rest.split('%').next()?.trim();
    pct.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn line_splitting_across_chunks() {
        let mut p = LineParser::new();
        assert!(p.push(b"{\"type\":\"hello\"").is_empty());
        let lines = p.push(b",\"hw\":\"x\"}\r\nnext\n");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("hello"));
        assert_eq!(lines[1], "next");
    }

    #[test]
    fn parse_line_classification() {
        let v = parse_line(r#"{"type":"imu","ts":1}"#).unwrap();
        assert_eq!(v["type"], "imu");
        assert!(parse_line("plain log line").is_none());
        assert!(parse_line(r#"{"no_type":1}"#).is_none());
    }

    #[test]
    fn hello_fields() {
        let v =
            parse_line(r#"{"type":"hello","hw":"zannen-smol","fw":"1.4.2","proto":1}"#).unwrap();
        assert_eq!(v["hw"], json!("zannen-smol"));
    }

    #[test]
    fn battery_line_console_format() {
        // 真实固件 battery 命令应答格式（SlimeVR-Tracker-nRF console.c）
        assert_eq!(
            parse_battery_line("Battery: 87.42% (Raw 87.42%, 3890 mV)"),
            Some(87.42)
        );
        assert_eq!(parse_battery_line("Battery: 5%"), Some(5.0));
        assert_eq!(parse_battery_line("ADC: 3890 mV"), None);
        assert_eq!(parse_battery_line("Tracker initialized: 87.42%"), None);
    }
}
