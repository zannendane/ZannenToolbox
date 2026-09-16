//! DashScope 实时语音识别 WebSocket 协议（run-task 任务协议）的消息构造与解析。
//!
//! 协议要点（Qwen-Audio-3.0-ASR-Flash-Streaming / Fun-ASR-Realtime 共用）：
//! - 连接 `wss://<endpoint>/api-ws/v1/inference`，请求头 `Authorization: bearer <key>`；
//! - 客户端发 `run-task`（task_group=audio / task=asr / function=recognition），
//!   等 `task-started` 后以二进制帧推 PCM s16le 16kHz 单声道裸流（约 100ms 一帧）；
//! - 服务端事件：`result-generated`（sentence.text + sentence_end 区分中间/最终）、
//!   `task-finished`、`task-failed`（header.error_message 带原因）；
//! - 结束发 `finish-task`。

use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TASK_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成 32 位 hex 任务 id（时间戳低位 + 进程内计数，无 uuid 依赖）。
pub fn gen_task_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TASK_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:016x}{seq:016x}")
}

/// run-task 指令文本帧。
///
/// `heartbeat: true`：即使持续静音音频也保持连接（服务端默认在音频
/// 无有效语音信号一段时间后超时断开，"request timeout"）。
/// `language_hints`：指定输入语言（ISO 码，至多 4 个）；空数组 = 自动识别。
pub fn build_run_task(task_id: &str, model: &str, language_hints: &[String]) -> String {
    let mut parameters = json!({
        "sample_rate": 16000,
        "format": "pcm",
        "heartbeat": true,
    });
    if !language_hints.is_empty() {
        parameters["language_hints"] = json!(language_hints);
    }
    json!({
        "header": {
            "action": "run-task",
            "task_id": task_id,
            "streaming": "duplex",
        },
        "payload": {
            "task_group": "audio",
            "task": "asr",
            "function": "recognition",
            "model": model,
            "parameters": parameters,
            "input": {},
        },
    })
    .to_string()
}

/// finish-task 指令文本帧。
pub fn build_finish_task(task_id: &str) -> String {
    json!({
        "header": {
            "action": "finish-task",
            "task_id": task_id,
            "streaming": "duplex",
        },
        "payload": { "input": {} },
    })
    .to_string()
}

/// 服务端事件分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// 任务已开始，可以推流。
    TaskStarted,
    /// 识别结果：sentence_end=false 为滚动修订的中间结果，true 为最终断句。
    Result { text: String, sentence_end: bool },
    /// 任务正常完成。
    TaskFinished,
    /// 任务失败（header.error_message）。
    TaskFailed(String),
    /// 其余事件（未知/保活等），忽略。
    Other,
}

/// 解析一帧服务端文本消息；非 JSON 或缺 header.event 归 Other。
pub fn parse_server_event(frame: &str) -> ServerEvent {
    let Ok(v) = serde_json::from_str::<Value>(frame) else {
        return ServerEvent::Other;
    };
    let event = v
        .get("header")
        .and_then(|h| h.get("event"))
        .and_then(Value::as_str)
        .unwrap_or("");
    match event {
        "task-started" => ServerEvent::TaskStarted,
        "result-generated" => {
            let sentence = v.pointer("/payload/output/sentence");
            let text = sentence
                .and_then(|s| s.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let sentence_end = sentence
                .and_then(|s| s.get("sentence_end"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            ServerEvent::Result { text, sentence_end }
        }
        "task-finished" => ServerEvent::TaskFinished,
        "task-failed" => {
            let msg = v
                .get("header")
                .and_then(|h| h.get("error_message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown task failure")
                .to_string();
            ServerEvent::TaskFailed(msg)
        }
        _ => ServerEvent::Other,
    }
}

/// f32 采样（[-1,1]）→ PCM s16le 字节流（16k 单声道裸流，WS 二进制帧载荷）。
pub fn f32_to_s16le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &x in samples {
        let v = (x.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_is_32_hex_and_unique() {
        let a = gen_task_id();
        let b = gen_task_id();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn run_task_message_shape() {
        let msg = build_run_task("tid01", "qwen-audio-3.0-asr-flash-streaming", &[]);
        let v: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["header"]["action"], "run-task");
        assert_eq!(v["header"]["task_id"], "tid01");
        assert_eq!(v["header"]["streaming"], "duplex");
        assert_eq!(v["payload"]["task_group"], "audio");
        assert_eq!(v["payload"]["task"], "asr");
        assert_eq!(v["payload"]["function"], "recognition");
        assert_eq!(v["payload"]["model"], "qwen-audio-3.0-asr-flash-streaming");
        assert_eq!(v["payload"]["parameters"]["sample_rate"], 16000);
        assert_eq!(v["payload"]["parameters"]["format"], "pcm");
        // 心跳保活（防服务端空闲断连）；未指定语言时不携带 language_hints
        assert_eq!(v["payload"]["parameters"]["heartbeat"], true);
        assert!(v["payload"]["parameters"].get("language_hints").is_none());
        // 指定输入语言 → language_hints
        let msg = build_run_task("tid01", "m", &["zh".to_string()]);
        let v: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["payload"]["parameters"]["language_hints"][0], "zh");
    }

    #[test]
    fn finish_task_message_shape() {
        let v: Value = serde_json::from_str(&build_finish_task("tid01")).unwrap();
        assert_eq!(v["header"]["action"], "finish-task");
        assert_eq!(v["header"]["task_id"], "tid01");
    }

    #[test]
    fn parse_result_events() {
        // 中间结果
        let frame = r#"{"header":{"event":"result-generated"},"payload":{"output":{"sentence":{"text":"你好","sentence_end":false}}}}"#;
        assert_eq!(
            parse_server_event(frame),
            ServerEvent::Result {
                text: "你好".into(),
                sentence_end: false
            }
        );
        // 最终断句
        let frame = r#"{"header":{"event":"result-generated"},"payload":{"output":{"sentence":{"text":"你好世界。","sentence_end":true}}}}"#;
        assert_eq!(
            parse_server_event(frame),
            ServerEvent::Result {
                text: "你好世界。".into(),
                sentence_end: true
            }
        );
    }

    #[test]
    fn parse_lifecycle_events() {
        assert_eq!(
            parse_server_event(r#"{"header":{"event":"task-started"}}"#),
            ServerEvent::TaskStarted
        );
        assert_eq!(
            parse_server_event(r#"{"header":{"event":"task-finished"}}"#),
            ServerEvent::TaskFinished
        );
        assert_eq!(
            parse_server_event(
                r#"{"header":{"event":"task-failed","error_message":"Invalid api-key"}}"#
            ),
            ServerEvent::TaskFailed("Invalid api-key".into())
        );
        // 缺字段容错
        assert_eq!(parse_server_event("not json"), ServerEvent::Other);
        assert_eq!(parse_server_event(r#"{"header":{}}"#), ServerEvent::Other);
        assert_eq!(
            parse_server_event(r#"{"header":{"event":"task-failed"}}"#),
            ServerEvent::TaskFailed("unknown task failure".into())
        );
    }

    #[test]
    fn pcm_conversion() {
        assert_eq!(f32_to_s16le_bytes(&[]), Vec::<u8>::new());
        // 0.0 → 0x0000；1.0 → 0x7FFF；-1.0 → 0x8001；超幅裁剪
        let out = f32_to_s16le_bytes(&[0.0, 1.0, -1.0, 2.0, -2.0]);
        assert_eq!(out.len(), 10);
        assert_eq!(&out[0..2], &[0x00, 0x00]);
        assert_eq!(&out[2..4], &[0xFF, 0x7F]);
        assert_eq!(i16::from_le_bytes([out[4], out[5]]), -32767);
        assert_eq!(&out[6..8], &[0xFF, 0x7F]);
        assert_eq!(i16::from_le_bytes([out[8], out[9]]), -32767);
    }
}
