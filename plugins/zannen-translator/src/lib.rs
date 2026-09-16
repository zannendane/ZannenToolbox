//! # Zannen 实时翻译插件
//!
//! 麦克风输入 → 语音转文字（STT，自动识别或指定输入语言）→ 翻译为目标语言。
//!
//! - `ping` / `session.start` / `session.stop` / `session.status`
//! - 事件（topic 前缀 `zannen.translator/`）：
//!   `state`（listening/processing/idle/error）、`utterance`（STT 完成一段）、
//!   `translation`（翻译完成，id 与 utterance 对应）、`error`（携带 E36xx 识别码）
//! - 会话唯一活跃：重复 `session.start` 先停旧会话。
//!
//! 错误码（docs/ERROR-CODES.md）：E3601 音频输入设备、E3602 STT、E3603 翻译、E3604 配置。

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use zannen_plugin_api::{
    export_plugin, BackendDecl, FrontendDecl, HostHandle, PluginError, PluginManifest, PluginRoute,
    ZannenPlugin, ZANNEN_ABI_VERSION,
};

mod dsp;
mod http;
mod providers;
mod realtime;
mod session;

use providers::{SttConfig, TranslateConfig};
use session::{SessionHandle, SharedState};

pub struct TranslatorPlugin {
    host: HostHandle,
    shared: Arc<Mutex<SharedState>>,
    session: Option<SessionHandle>,
}

impl ZannenPlugin for TranslatorPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "zannen.translator".into(),
            name: "Zannen Live Translate".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            api: ZANNEN_ABI_VERSION,
            description: "Live speech translation: microphone → STT → target language".into(),
            icon: Some("languages".into()),
            frontend: Some(FrontendDecl {
                entry: "frontend/dist/index.js".into(),
                css: Some("frontend/dist/style.css".into()),
            }),
            backend: Some(BackendDecl {
                name: "zannen_translator".into(),
            }),
            capabilities: vec!["audio".into()],
            routes: vec![PluginRoute {
                path: "translator".into(),
                title: "Live Translate".into(),
                icon: "languages".into(),
            }],
        }
    }

    fn new(host: HostHandle) -> Self {
        host.log(3, "zannen.translator initialized");
        Self {
            host,
            shared: Arc::new(Mutex::new(SharedState {
                state: "idle".into(),
                generation: 0,
            })),
            session: None,
        }
    }

    fn invoke(&mut self, method: &str, args: Value) -> Result<Value, PluginError> {
        match method {
            "ping" => Ok(json!({
                "pong": true,
                "id": "zannen.translator",
                "version": env!("CARGO_PKG_VERSION"),
            })),
            "session.start" => {
                let stt = SttConfig::parse(args.get("stt").unwrap_or(&Value::Null))?;
                let translate =
                    TranslateConfig::parse(args.get("translate").unwrap_or(&Value::Null))?;
                // 会话唯一活跃：重复 start 先停旧；代际递增使旧线程迟发事件全部失效
                if let Some(old) = self.session.take() {
                    old.stop();
                }
                let generation = {
                    let mut s = self
                        .shared
                        .lock()
                        .map_err(|e| PluginError::Other(e.to_string()))?;
                    s.generation += 1;
                    s.generation
                };
                self.session = Some(session::start(
                    self.host,
                    stt,
                    translate,
                    self.shared.clone(),
                    generation,
                ));
                Ok(json!({ "active": true }))
            }
            "session.stop" => {
                if let Some(old) = self.session.take() {
                    old.stop();
                }
                if let Ok(mut s) = self.shared.lock() {
                    s.generation += 1;
                }
                session::emit_state_now(&self.host, &self.shared, "idle");
                Ok(json!({ "active": false }))
            }
            "session.status" => {
                let active = self
                    .session
                    .as_ref()
                    .map(SessionHandle::is_running)
                    .unwrap_or(false);
                let state = self
                    .shared
                    .lock()
                    .map(|s| s.state.clone())
                    .unwrap_or_else(|_| "idle".into());
                Ok(json!({ "active": active, "state": state }))
            }
            other => Err(PluginError::UnknownMethod(other.into())),
        }
    }
}

impl Drop for TranslatorPlugin {
    fn drop(&mut self) {
        if let Some(old) = self.session.take() {
            old.stop();
        }
    }
}

export_plugin!(TranslatorPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_consistent() {
        let m = TranslatorPlugin::manifest();
        assert_eq!(m.id, "zannen.translator");
        assert_eq!(m.api, ZANNEN_ABI_VERSION);
        assert_eq!(m.routes.len(), 1);
        assert_eq!(m.routes[0].path, "translator");
        assert_eq!(m.capabilities, vec!["audio".to_string()]);
        assert!(m.frontend.unwrap().entry.ends_with("index.js"));
    }

    #[test]
    fn session_start_rejects_bad_config() {
        // 构造不依赖宿主的纯配置校验路径（openai 缺 apiKey → E3604）
        let err = SttConfig::parse(&json!({ "provider": "openai" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
        let err = TranslateConfig::parse(&json!({ "provider": "deepl" })).unwrap_err();
        assert!(err.to_string().contains("E3604"));
    }
}
