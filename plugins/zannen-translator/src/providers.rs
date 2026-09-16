//! STT 与翻译提供源：配置解析（E3604 校验）与请求实现。
//!
//! - STT：`mock`（离线轮询示例句）/ `openai`（OpenAI 兼容 audio/transcriptions，
//!   可改 endpoint 接 Groq / 本地 whisper.cpp server）/ `dashscope`（千问 DashScope
//!   OpenAI 兼容模式，qwen3-asr-flash，input_audio base64）
//! - 翻译：`mock`（前缀演示）/ `openai-compatible`（chat/completions）/ `deepl`
//!   （v2/translate）/ `dashscope`（qwen-mt 系列，translation_options）

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde_json::{json, Value};
use zannen_plugin_api::PluginError;

use crate::http::{
    multipart_content_type, multipart_encode, new_boundary, url_encode, MultipartField,
};

/// 各提供源默认 endpoint（前端留空时后端兜底）。
pub const DEFAULT_OPENAI_ENDPOINT: &str = "https://api.openai.com/v1";
pub const DEFAULT_DEEPL_ENDPOINT: &str = "https://api-free.deepl.com";
pub const DEFAULT_DASHSCOPE_ENDPOINT: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
pub const DEFAULT_STT_MODEL: &str = "whisper-1";
pub const DEFAULT_CHAT_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_DASHSCOPE_STT_MODEL: &str = "qwen3-asr-flash";
pub const DEFAULT_DASHSCOPE_MT_MODEL: &str = "qwen-mt-turbo";
pub const DEFAULT_DASHSCOPE_WS_ENDPOINT: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference";
pub const DEFAULT_DASHSCOPE_WS_MODEL: &str = "qwen-audio-3.0-asr-flash-streaming";

/// HTTP 请求超时（STT/翻译单段请求）。
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

// ---------- 错误构造（事件侧 code 字段独立携带，invoke 错误文本内嵌识别码） ----------

fn config_err(msg: impl Into<String>) -> PluginError {
    PluginError::Other(format!("[E3604] invalid config: {}", msg.into()))
}

fn stt_err(msg: impl Into<String>) -> PluginError {
    PluginError::Other(format!("[E3602] stt request failed: {}", msg.into()))
}

fn translate_err(msg: impl Into<String>) -> PluginError {
    PluginError::Other(format!("[E3603] translate request failed: {}", msg.into()))
}

// ---------- 配置 ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttProvider {
    Mock,
    OpenAi,
    DashScope,
    /// DashScope 实时流式（WebSocket run-task 协议，qwen-audio-*-streaming 系列）。
    DashScopeRealtime,
}

#[derive(Debug, Clone)]
pub struct SttConfig {
    pub provider: SttProvider,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    /// `auto` 或 ISO 语言码（en/ja/zh…）；auto 时不向 STT 传 language。
    pub language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslateProvider {
    Mock,
    OpenAiCompatible,
    DeepL,
    DashScope,
}

#[derive(Debug, Clone)]
pub struct TranslateConfig {
    pub provider: TranslateProvider,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    /// 目标语言码（zh/en/ja…）。
    pub target: String,
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// ISO 语言码宽松校验：2-3 小写字母，可选 `-Region` 后缀。
fn is_valid_lang(code: &str) -> bool {
    let mut parts = code.split('-');
    let Some(base) = parts.next() else {
        return false;
    };
    let base_ok = (2..=3).contains(&base.len()) && base.bytes().all(|b| b.is_ascii_lowercase());
    let rest_ok =
        parts.all(|p| (2..=4).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()));
    base_ok && rest_ok
}

impl SttConfig {
    pub fn parse(v: &Value) -> Result<Self, PluginError> {
        let provider = match opt_str(v, "provider").as_deref().unwrap_or("mock") {
            "mock" => SttProvider::Mock,
            "openai" => SttProvider::OpenAi,
            "dashscope" => SttProvider::DashScope,
            "dashscope-realtime" => SttProvider::DashScopeRealtime,
            other => return Err(config_err(format!("unknown stt provider: {other}"))),
        };
        let default_endpoint = match provider {
            SttProvider::DashScope => DEFAULT_DASHSCOPE_ENDPOINT,
            SttProvider::DashScopeRealtime => DEFAULT_DASHSCOPE_WS_ENDPOINT,
            _ => DEFAULT_OPENAI_ENDPOINT,
        };
        let default_model = match provider {
            SttProvider::DashScope => DEFAULT_DASHSCOPE_STT_MODEL,
            SttProvider::DashScopeRealtime => DEFAULT_DASHSCOPE_WS_MODEL,
            _ => DEFAULT_STT_MODEL,
        };
        let endpoint = opt_str(v, "endpoint").unwrap_or_else(|| default_endpoint.into());
        let api_key = opt_str(v, "apiKey").unwrap_or_default();
        let model = opt_str(v, "model").unwrap_or_else(|| default_model.into());
        let language = opt_str(v, "language").unwrap_or_else(|| "auto".into());
        if provider != SttProvider::Mock && api_key.is_empty() {
            return Err(config_err("stt.apiKey is required for this provider"));
        }
        // 常见误配：把 WebSocket 实时模型填进 OpenAI 兼容 HTTP 通道（必然 404）
        if provider != SttProvider::DashScopeRealtime && model.contains("-streaming") {
            return Err(config_err(format!(
                "model `{model}` is a realtime (WebSocket) model; use the dashscope-realtime provider instead"
            )));
        }
        if language != "auto" && !is_valid_lang(&language) {
            return Err(config_err(format!("invalid stt language code: {language}")));
        }
        Ok(Self {
            provider,
            endpoint,
            api_key,
            model,
            language,
        })
    }

    /// 是否为实时流式提供源（会话走 WebSocket 推流路径而非 VAD 批量识别）。
    pub fn is_realtime(&self) -> bool {
        self.provider == SttProvider::DashScopeRealtime
    }
}

impl TranslateConfig {
    pub fn parse(v: &Value) -> Result<Self, PluginError> {
        let provider = match opt_str(v, "provider").as_deref().unwrap_or("mock") {
            "mock" => TranslateProvider::Mock,
            "openai-compatible" => TranslateProvider::OpenAiCompatible,
            "deepl" => TranslateProvider::DeepL,
            "dashscope" => TranslateProvider::DashScope,
            other => return Err(config_err(format!("unknown translate provider: {other}"))),
        };
        let default_endpoint = match provider {
            TranslateProvider::DeepL => DEFAULT_DEEPL_ENDPOINT,
            TranslateProvider::DashScope => DEFAULT_DASHSCOPE_ENDPOINT,
            _ => DEFAULT_OPENAI_ENDPOINT,
        };
        let default_model = match provider {
            TranslateProvider::DashScope => DEFAULT_DASHSCOPE_MT_MODEL,
            _ => DEFAULT_CHAT_MODEL,
        };
        let endpoint = opt_str(v, "endpoint").unwrap_or_else(|| default_endpoint.into());
        let api_key = opt_str(v, "apiKey").unwrap_or_default();
        let model = opt_str(v, "model").unwrap_or_else(|| default_model.into());
        let target = opt_str(v, "target").unwrap_or_else(|| "zh".into());
        if provider != TranslateProvider::Mock && api_key.is_empty() {
            return Err(config_err("translate.apiKey is required for this provider"));
        }
        if !is_valid_lang(&target) {
            return Err(config_err(format!(
                "invalid translate target code: {target}"
            )));
        }
        Ok(Self {
            provider,
            endpoint,
            api_key,
            model,
            target,
        })
    }
}

// ---------- STT ----------

/// mock STT 轮询器：英/日/中示例句循环（离线演示数据）。
#[derive(Default)]
pub struct MockStt {
    idx: usize,
}

const MOCK_UTTERANCES: &[(&str, &str)] = &[
    ("en", "Hello, this is a live translation demo."),
    ("ja", "こんにちは、これはリアルタイム翻訳のデモです。"),
    ("zh", "你好，这是一条实时翻译演示文本。"),
];

impl MockStt {
    pub fn next(&mut self) -> (String, Option<String>) {
        let (lang, text) = MOCK_UTTERANCES[self.idx % MOCK_UTTERANCES.len()];
        self.idx += 1;
        (text.to_string(), Some(lang.to_string()))
    }
}

/// 识别一段 16kHz 单声道 WAV，返回 (text, lang?)。
pub fn transcribe(cfg: &SttConfig, wav: &[u8]) -> Result<(String, Option<String>), PluginError> {
    match cfg.provider {
        SttProvider::OpenAi => transcribe_openai(cfg, wav),
        SttProvider::DashScope => transcribe_dashscope(cfg, wav),
        // mock 提供源不走真实音频（离线演示由会话线程轮询驱动）
        SttProvider::Mock => Ok((String::new(), None)),
        // 实时流式提供源不经批量识别（会话线程走 WebSocket 推流路径）
        SttProvider::DashScopeRealtime => Err(stt_err(
            "realtime provider does not support batch transcribe",
        )),
    }
}

/// 共享 HTTP agent：连接池复用（省每请求 TCP+TLS 握手，显著降低首 token 延迟）。
fn http_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build())
}

/// ureq 错误 → 可读文本（HTTP 状态 + 响应体摘录）。
fn http_err_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let snippet: String = body.chars().take(200).collect();
            format!("HTTP {code}: {snippet}")
        }
        e => e.to_string(),
    }
}

/// 解析响应 JSON 并在缺字段时报错。
fn parse_json(resp: ureq::Response, err: fn(String) -> PluginError) -> Result<Value, PluginError> {
    let text = resp.into_string().map_err(|e| err(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| err(format!("bad response json: {e}")))
}

fn transcribe_openai(cfg: &SttConfig, wav: &[u8]) -> Result<(String, Option<String>), PluginError> {
    let boundary = new_boundary();
    let mut fields = vec![
        MultipartField::file("file", "audio.wav", "audio/wav", wav.to_vec()),
        MultipartField::text("model", &cfg.model),
        // verbose_json 携带识别出的语言码（auto 模式用于界面徽标）
        MultipartField::text("response_format", "verbose_json"),
    ];
    if cfg.language != "auto" {
        fields.push(MultipartField::text("language", &cfg.language));
    }
    let body = multipart_encode(&boundary, &fields);
    let url = format!(
        "{}/audio/transcriptions",
        cfg.endpoint.trim_end_matches('/')
    );
    let resp = http_agent()
        .post(&url)
        .set("Authorization", &format!("Bearer {}", cfg.api_key))
        .set("Content-Type", &multipart_content_type(&boundary))
        .send_bytes(&body)
        .map_err(|e| stt_err(http_err_text(e)))?;
    let json = parse_json(resp, stt_err)?;
    let text = json
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let lang = json
        .get("language")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| (cfg.language != "auto").then(|| cfg.language.clone()));
    Ok((text, lang))
}

/// 千问 DashScope STT（OpenAI 兼容模式）：qwen3-asr-flash，
/// user 消息携带 input_audio（base64 data URI），asr_options 控制 ITN 与语言。
fn transcribe_dashscope(
    cfg: &SttConfig,
    wav: &[u8],
) -> Result<(String, Option<String>), PluginError> {
    let data_uri = format!("data:audio/wav;base64,{}", B64.encode(wav));
    let mut payload = json!({
        "model": cfg.model,
        "messages": [
            {
                "role": "user",
                "content": [
                    { "type": "input_audio", "input_audio": { "data": data_uri } },
                ],
            },
        ],
        "stream": false,
        "asr_options": { "enable_itn": true },
    });
    if cfg.language != "auto" {
        payload["asr_options"]["language"] = json!(cfg.language);
    }
    let body = serde_json::to_string(&payload).map_err(|e| stt_err(e.to_string()))?;
    let url = format!("{}/chat/completions", cfg.endpoint.trim_end_matches('/'));
    let resp = http_agent()
        .post(&url)
        .set("Authorization", &format!("Bearer {}", cfg.api_key))
        .set("Content-Type", "application/json")
        .send_string(&body)
        .map_err(|e| stt_err(http_err_text(e)))?;
    let json = parse_json(resp, stt_err)?;
    let text = json
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    // qwen3-asr 不回传识别语言：auto 模式语言徽标留空，指定语言时回显
    let lang = (cfg.language != "auto").then(|| cfg.language.clone());
    Ok((text, lang))
}

// ---------- 翻译 ----------

/// 同传系统提示词：专业口译人设 + 只输出译文 + 流式片段按原样翻译不补全。
fn translate_system_prompt(target_name: &str) -> String {
    format!(
        "You are a simultaneous interpreter. Translate the user's text into {target_name}. \
         Output only the translation: no quotes, notes or explanations. \
         Preserve numbers, names and formatting; translate fragments as-is."
    )
}

/// 按提供源端点注入"关闭思考"参数（缩短首 token 与总耗时）：
/// - DashScope/千问：顶层 `enable_thinking: false`（官方实测总耗时降 60-75%）
/// - DeepSeek：`thinking: {"type": "disabled"}`
///
/// 其余提供源不注入（严格校验的服务端可能拒绝未知参数）。
pub fn thinking_off_params(endpoint: &str) -> Option<(&'static str, Value)> {
    let e = endpoint.to_lowercase();
    if e.contains("dashscope") {
        Some(("enable_thinking", json!(false)))
    } else if e.contains("deepseek") {
        Some(("thinking", json!({ "type": "disabled" })))
    } else {
        None
    }
}

/// 流式翻译（带近期上下文，连贯性）：history 为最近的 (源文, 译文) 对。
///
/// SSE 逐 token 到达，`on_chunk` 收到截至目前的累计译文（chat 提供源首 token 即返回，显著降低感知延迟）。
///
/// - openai-compatible：注入历史 user/assistant 消息语境；增量 delta 追加；
/// - dashscope（qwen-mt）：注入 translation_options.tm_list 翻译记忆；flash/lite 增量、plus/turbo 全量回放（前缀包含则替换）；
/// - mock/deepl：忽略上下文；不支持流式，完整结果回调一次。
pub fn translate_with_context_stream(
    cfg: &TranslateConfig,
    text: &str,
    history: &[(String, String)],
    on_chunk: &mut dyn FnMut(&str),
) -> Result<String, PluginError> {
    match cfg.provider {
        TranslateProvider::Mock => {
            let out = format!("[→{}] {text}", cfg.target);
            on_chunk(&out);
            Ok(out)
        }
        TranslateProvider::OpenAiCompatible => translate_chat(cfg, text, history, on_chunk),
        TranslateProvider::DeepL => {
            let out = translate_deepl(cfg, text)?;
            on_chunk(&out);
            Ok(out)
        }
        TranslateProvider::DashScope => translate_dashscope(cfg, text, history, on_chunk),
    }
}

/// 目标语言码 → 自然语言名（注入 system prompt）。
fn lang_display_name(code: &str) -> &str {
    match code {
        "zh" => "Simplified Chinese",
        "zh-CN" => "Simplified Chinese",
        "zh-TW" => "Traditional Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        _ => "the target language",
    }
}

fn translate_chat(
    cfg: &TranslateConfig,
    text: &str,
    history: &[(String, String)],
    on_chunk: &mut dyn FnMut(&str),
) -> Result<String, PluginError> {
    let url = format!("{}/chat/completions", cfg.endpoint.trim_end_matches('/'));
    let target_name = lang_display_name(&cfg.target);
    let mut messages = vec![json!({
        "role": "system",
        "content": translate_system_prompt(target_name),
    })];
    // 近期译文对作为多轮语境（连贯性，最多 3 对）
    for (src, dst) in history.iter().rev().take(3).rev() {
        messages.push(json!({ "role": "user", "content": src }));
        messages.push(json!({ "role": "assistant", "content": dst }));
    }
    messages.push(json!({ "role": "user", "content": text }));
    let mut payload = json!({
        "model": cfg.model,
        "messages": messages,
        "temperature": 0.2,
        "stream": true,
    });
    if let Some((key, value)) = thinking_off_params(&cfg.endpoint) {
        payload[key] = value;
    }
    let body = serde_json::to_string(&payload).map_err(|e| translate_err(e.to_string()))?;
    let resp = http_agent()
        .post(&url)
        .set("Authorization", &format!("Bearer {}", cfg.api_key))
        .set("Content-Type", "application/json")
        .send_string(&body)
        .map_err(|e| translate_err(http_err_text(e)))?;
    // SSE 流式读取：增量 delta 追加为累计文本，每个 chunk 上屏一次
    let mut acc = String::new();
    let reader = std::io::BufReader::new(resp.into_reader());
    crate::http::for_each_sse_data(reader, |data| {
        if let Some(delta) = crate::http::sse_delta_content(data) {
            acc.push_str(&delta);
            on_chunk(&acc);
        }
    })
    .map_err(|e| translate_err(format!("sse stream: {e}")))?;
    let trimmed = acc.trim().to_string();
    if trimmed.is_empty() {
        return Err(translate_err("stream ended without content"));
    }
    Ok(trimmed)
}

/// DeepL 目标语言码映射（v2 API 的大写区域码）。
fn deepl_target_lang(code: &str) -> String {
    match code {
        "zh" | "zh-CN" => "ZH".into(),
        "zh-TW" => "ZH-HANT".into(),
        "en" => "EN-US".into(),
        "ja" => "JA".into(),
        other => other.to_uppercase(),
    }
}

fn translate_deepl(cfg: &TranslateConfig, text: &str) -> Result<String, PluginError> {
    let url = format!("{}/v2/translate", cfg.endpoint.trim_end_matches('/'));
    let body = format!(
        "auth_key={}&text={}&target_lang={}",
        url_encode(&cfg.api_key),
        url_encode(text),
        url_encode(&deepl_target_lang(&cfg.target)),
    );
    let resp = http_agent()
        .post(&url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body)
        .map_err(|e| translate_err(http_err_text(e)))?;
    let json = parse_json(resp, translate_err)?;
    let translated = json
        .pointer("/translations/0/text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| translate_err("response missing translations[0].text"))?;
    Ok(translated.to_string())
}

/// qwen-mt 目标语言：ISO 码 → DashScope 英文全称（文档「支持的语言」表）。
fn dashscope_target_lang(code: &str) -> Result<&'static str, PluginError> {
    match code {
        "zh" | "zh-CN" => Ok("Chinese"),
        "zh-TW" => Ok("Traditional Chinese"),
        "en" => Ok("English"),
        "ja" => Ok("Japanese"),
        "ko" => Ok("Korean"),
        "de" => Ok("German"),
        "fr" => Ok("French"),
        "es" => Ok("Spanish"),
        "ru" => Ok("Russian"),
        "pt" => Ok("Portuguese"),
        "it" => Ok("Italian"),
        "th" => Ok("Thai"),
        "vi" => Ok("Vietnamese"),
        "ar" => Ok("Arabic"),
        "id" => Ok("Indonesian"),
        other => Err(translate_err(format!(
            "unsupported dashscope target language: {other}"
        ))),
    }
}

/// 千问 DashScope 翻译（OpenAI 兼容模式）：qwen-mt 系列，
/// 仅 User Message 携带原文，translation_options 指定源/目标语言（源 auto 自动识别），
/// 近期译文对注入 tm_list 翻译记忆（最多 3 对，提升连贯性）。
fn translate_dashscope(
    cfg: &TranslateConfig,
    text: &str,
    history: &[(String, String)],
    on_chunk: &mut dyn FnMut(&str),
) -> Result<String, PluginError> {
    let target_lang = dashscope_target_lang(&cfg.target)?;
    let tm_list: Vec<Value> = history
        .iter()
        .rev()
        .take(3)
        .map(|(src, dst)| json!({ "source": src, "target": dst }))
        .collect();
    let mut translation_options = json!({ "source_lang": "auto", "target_lang": target_lang });
    if !tm_list.is_empty() {
        translation_options["tm_list"] = json!(tm_list);
    }
    let payload = json!({
        "model": cfg.model,
        "messages": [ { "role": "user", "content": text } ],
        "translation_options": translation_options,
        "stream": true,
    });
    let body = serde_json::to_string(&payload).map_err(|e| translate_err(e.to_string()))?;
    let url = format!("{}/chat/completions", cfg.endpoint.trim_end_matches('/'));
    let resp = http_agent()
        .post(&url)
        .set("Authorization", &format!("Bearer {}", cfg.api_key))
        .set("Content-Type", "application/json")
        .send_string(&body)
        .map_err(|e| translate_err(http_err_text(e)))?;
    // SSE 流式：flash/lite 为增量追加，plus/turbo 为全量回放（前缀包含则替换）
    let mut acc = String::new();
    let reader = std::io::BufReader::new(resp.into_reader());
    crate::http::for_each_sse_data(reader, |data| {
        if let Some(delta) = crate::http::sse_delta_content(data) {
            if delta.len() > acc.len() && delta.starts_with(&acc) {
                acc = delta; // 全量回放
            } else {
                acc.push_str(&delta); // 增量
            }
            on_chunk(&acc);
        }
    })
    .map_err(|e| translate_err(format!("sse stream: {e}")))?;
    let trimmed = acc.trim().to_string();
    if trimmed.is_empty() {
        return Err(translate_err("stream ended without content"));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stt_config_defaults_to_mock() {
        let cfg = SttConfig::parse(&json!({})).unwrap();
        assert_eq!(cfg.provider, SttProvider::Mock);
        assert_eq!(cfg.language, "auto");
        assert_eq!(cfg.endpoint, DEFAULT_OPENAI_ENDPOINT);
    }

    #[test]
    fn stt_config_requires_api_key_for_openai() {
        let err = SttConfig::parse(&json!({ "provider": "openai" })).unwrap_err();
        assert!(err.to_string().contains("E3604"), "got: {err}");
    }

    #[test]
    fn stt_config_rejects_unknown_provider_and_bad_language() {
        let err = SttConfig::parse(&json!({ "provider": "whisperx" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
        let err = SttConfig::parse(&json!({ "language": "english" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
    }

    #[test]
    fn translate_config_validation() {
        // mock 无需密钥
        let cfg = TranslateConfig::parse(&json!({ "provider": "mock", "target": "en" })).unwrap();
        assert_eq!(cfg.target, "en");
        // openai-compatible 缺密钥 → E3604
        let err = TranslateConfig::parse(&json!({ "provider": "openai-compatible" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
        // deepl 缺密钥 → E3604
        let err = TranslateConfig::parse(&json!({ "provider": "deepl" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
        // deepl 默认 endpoint 与 openai 不同
        let cfg = TranslateConfig::parse(&json!({ "provider": "deepl", "apiKey": "k" })).unwrap();
        assert_eq!(cfg.endpoint, DEFAULT_DEEPL_ENDPOINT);
        // 未知提供源
        let err = TranslateConfig::parse(&json!({ "provider": "bing" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
    }

    #[test]
    fn mock_stt_rotates_samples() {
        let mut mock = MockStt::default();
        let a = mock.next();
        let b = mock.next();
        let c = mock.next();
        let d = mock.next();
        assert_eq!(a.1.as_deref(), Some("en"));
        assert_eq!(b.1.as_deref(), Some("ja"));
        assert_eq!(c.1.as_deref(), Some("zh"));
        assert_eq!(d, a); // 循环
    }

    #[test]
    fn mock_translate_prefixes_target() {
        let cfg = TranslateConfig::parse(&json!({ "provider": "mock", "target": "ja" })).unwrap();
        assert_eq!(
            translate_with_context_stream(&cfg, "hello", &[], &mut |_| {}).unwrap(),
            "[→ja] hello"
        );
    }

    #[test]
    fn deepl_target_mapping() {
        assert_eq!(deepl_target_lang("zh"), "ZH");
        assert_eq!(deepl_target_lang("en"), "EN-US");
        assert_eq!(deepl_target_lang("ja"), "JA");
        assert_eq!(deepl_target_lang("de"), "DE");
    }

    #[test]
    fn dashscope_config_defaults() {
        // STT：dashscope 提供源落到 DashScope 端点与 qwen3-asr 模型，缺密钥 → E3604
        let err = SttConfig::parse(&json!({ "provider": "dashscope" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
        let cfg = SttConfig::parse(&json!({ "provider": "dashscope", "apiKey": "sk-x" })).unwrap();
        assert_eq!(cfg.endpoint, DEFAULT_DASHSCOPE_ENDPOINT);
        assert_eq!(cfg.model, DEFAULT_DASHSCOPE_STT_MODEL);

        // 翻译：dashscope 提供源落到 qwen-mt 模型，缺密钥 → E3604
        let err = TranslateConfig::parse(&json!({ "provider": "dashscope" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
        let cfg =
            TranslateConfig::parse(&json!({ "provider": "dashscope", "apiKey": "sk-x" })).unwrap();
        assert_eq!(cfg.endpoint, DEFAULT_DASHSCOPE_ENDPOINT);
        assert_eq!(cfg.model, DEFAULT_DASHSCOPE_MT_MODEL);
    }

    #[test]
    fn dashscope_target_mapping() {
        assert_eq!(dashscope_target_lang("zh").unwrap(), "Chinese");
        assert_eq!(
            dashscope_target_lang("zh-TW").unwrap(),
            "Traditional Chinese"
        );
        assert_eq!(dashscope_target_lang("en").unwrap(), "English");
        assert_eq!(dashscope_target_lang("ja").unwrap(), "Japanese");
        assert!(dashscope_target_lang("xx").is_err());
    }

    #[test]
    fn streaming_model_rejected_on_http_providers() {
        // WebSocket 实时模型误填进 HTTP 通道 → E3604 并指明应改用 dashscope-realtime
        let err = SttConfig::parse(
            &json!({ "provider": "dashscope", "apiKey": "sk-x", "model": "qwen-audio-3.0-asr-flash-streaming" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("E3604"));
        assert!(err.to_string().contains("dashscope-realtime"));
        // 实时提供源下同名模型放行
        let cfg = SttConfig::parse(
            &json!({ "provider": "dashscope-realtime", "apiKey": "sk-x", "model": "qwen-audio-3.0-asr-flash-streaming" }),
        )
        .unwrap();
        assert!(cfg.is_realtime());
    }

    #[test]
    fn dashscope_realtime_config_defaults() {
        // 实时流式提供源：落到 wss 端点与 streaming 模型，缺密钥 → E3604
        let err = SttConfig::parse(&json!({ "provider": "dashscope-realtime" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
        let cfg = SttConfig::parse(&json!({ "provider": "dashscope-realtime", "apiKey": "sk-x" }))
            .unwrap();
        assert_eq!(cfg.endpoint, DEFAULT_DASHSCOPE_WS_ENDPOINT);
        assert_eq!(cfg.model, DEFAULT_DASHSCOPE_WS_MODEL);
        assert!(cfg.is_realtime());
        // 非实时提供源
        let cfg = SttConfig::parse(&json!({ "provider": "dashscope", "apiKey": "sk-x" })).unwrap();
        assert!(!cfg.is_realtime());
    }

    #[test]
    fn thinking_off_params_by_endpoint() {
        assert_eq!(
            thinking_off_params("https://dashscope.aliyuncs.com/compatible-mode/v1"),
            Some(("enable_thinking", json!(false)))
        );
        assert_eq!(
            thinking_off_params("https://api.deepseek.com/v1"),
            Some(("thinking", json!({ "type": "disabled" })))
        );
        assert_eq!(thinking_off_params("https://api.openai.com/v1"), None);
    }

    #[test]
    fn lang_code_validation() {
        assert!(is_valid_lang("en"));
        assert!(is_valid_lang("zh"));
        assert!(is_valid_lang("zh-CN"));
        assert!(!is_valid_lang("english"));
        assert!(!is_valid_lang("e"));
        assert!(!is_valid_lang("EN")); // 须小写 base
        assert!(!is_valid_lang(""));
    }
}
