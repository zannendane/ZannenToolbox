//! 会话线程：采集（cpal，或 mock 提词）→ VAD 分段 → STT → 翻译 → 事件上行。
//!
//! 线程模型：
//! - mock STT：不碰麦克风，每 2.5s 产出一句示例文本（离线演示，不触发系统授权框）；
//! - 真实 STT：cpal 输入流回调只做 转 f32 → 混音 → 重采样 → 入缓冲；
//!   会话线程主循环每 60ms 取缓冲喂 VAD，段尾同步执行 STT→翻译（ureq 阻塞调用）。
//!   段处理期间新音频持续入缓冲，不丢字。
//! - 错误分级：设备打开失败等致命错误 → error 事件 + 状态回 idle；
//!   单段 STT/翻译失败 → error 事件后会话继续（回 listening）。
//! - 代际防护：每次 start/stop 递增 SharedState.generation，旧线程的一切事件
//!   （含状态）在代际过期后一律丢弃——重复 start 不会被旧线程的迟发事件篡位。
//!   `idle` 状态由 session.stop 调用方同步发出，线程正常退出不再发状态。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use zannen_plugin_api::{HostHandle, PluginError};

use crate::dsp::{
    downmix_to_mono, wav_encode_16k_mono_s16le, LinearResampler, VadSegmenter, VoiceActivity,
    TARGET_SAMPLE_RATE,
};
use crate::providers::{
    transcribe, translate_with_context_stream, MockStt, SttConfig, SttProvider, TranslateConfig,
};
use crate::realtime::{
    build_finish_task, build_run_task, f32_to_s16le_bytes, gen_task_id, parse_server_event,
    ServerEvent,
};

pub const TOPIC_STATE: &str = "zannen.translator/state";
pub const TOPIC_UTTERANCE: &str = "zannen.translator/utterance";
pub const TOPIC_TRANSLATION: &str = "zannen.translator/translation";
pub const TOPIC_ERROR: &str = "zannen.translator/error";
pub const TOPIC_PARTIAL: &str = "zannen.translator/partial";
pub const TOPIC_PARTIAL_TRANSLATION: &str = "zannen.translator/partial_translation";
/// 音频电平/语音活动（指示灯用）：{level: f32, speaking: bool}，约 5Hz。
pub const TOPIC_AUDIO: &str = "zannen.translator/audio";

pub const ERR_AUDIO_DEVICE: &str = "E3601";
pub const ERR_STT: &str = "E3602";
pub const ERR_TRANSLATE: &str = "E3603";
/// 提供源响应迟缓（非致命告警，前端指示灯转黄）。
pub const ERR_SLOW: &str = "E3605";

/// 提供源请求延迟告警阈值。
const SLOW_REQUEST_THRESHOLD: Duration = Duration::from_secs(4);

/// 静音保活帧长度（100ms @ 16kHz 单声道）。
const SILENCE_FRAME_SAMPLES: usize = 1600;

/// 会话可见状态（session.status 查询用）与代际计数。
pub struct SharedState {
    pub state: String,
    pub generation: u64,
}

/// 活跃会话句柄；stop 只置标志位，线程自行收尾（不 join，避免阻塞宿主 invoke 串行化锁）。
pub struct SessionHandle {
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

impl SessionHandle {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 启动会话线程（调用方需已递增 generation 并传入）。
pub fn start(
    host: HostHandle,
    stt: SttConfig,
    translate_cfg: TranslateConfig,
    shared: Arc<Mutex<SharedState>>,
    generation: u64,
) -> SessionHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));
    let (stop2, running2) = (stop.clone(), running.clone());
    std::thread::Builder::new()
        .name("zannen-translator-session".into())
        .spawn(move || {
            if stt.provider == SttProvider::Mock {
                run_mock(host, translate_cfg, &stop2, &shared, generation);
            } else if stt.is_realtime() {
                run_realtime(host, stt, translate_cfg, stop2.clone(), &shared, generation);
            } else {
                run_capture(host, stt, translate_cfg, &stop2, &shared, generation);
            }
            running2.store(false, Ordering::Relaxed);
        })
        .expect("spawn translator session thread");
    SessionHandle { stop, running }
}

// ---------- 事件上行辅助（全部经过代际检查） ----------

/// 代际仍有效才返回 true（锁失败视为失效，静默丢弃）。
fn is_current(shared: &Arc<Mutex<SharedState>>, generation: u64) -> bool {
    shared
        .lock()
        .map(|s| s.generation == generation)
        .unwrap_or(false)
}

fn emit(host: &HostHandle, topic: &str, payload: Value) {
    if let Err(e) = host.emit(topic, payload) {
        host.log(2, &format!("translator emit {topic} failed: {e}"));
    }
}

/// 无条件状态上行（session.stop 同步调用路径用）。
pub fn emit_state_now(host: &HostHandle, shared: &Arc<Mutex<SharedState>>, state: &str) {
    if let Ok(mut s) = shared.lock() {
        s.state = state.to_string();
    }
    emit(host, TOPIC_STATE, json!({ "state": state }));
}

fn set_state(
    host: &HostHandle,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
    state: &str,
    detail: Option<&str>,
) {
    if !is_current(shared, generation) {
        return;
    }
    if let Ok(mut s) = shared.lock() {
        s.state = state.to_string();
    }
    let mut payload = json!({ "state": state });
    if let Some(d) = detail {
        payload["detail"] = json!(d);
    }
    emit(host, TOPIC_STATE, payload);
}

fn emit_error(
    host: &HostHandle,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
    code: &str,
    message: &str,
) {
    if !is_current(shared, generation) {
        return;
    }
    emit(
        host,
        TOPIC_ERROR,
        json!({ "code": code, "message": message }),
    );
}

/// 致命错误：error 事件 + 状态经 error 回 idle。
fn fatal(
    host: &HostHandle,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
    code: &str,
    message: &str,
) {
    host.log(1, &format!("translator session fatal [{code}]: {message}"));
    emit_error(host, shared, generation, code, message);
    set_state(host, shared, generation, "error", Some(message));
    set_state(host, shared, generation, "idle", None);
}

// ---------- 公共段处理 ----------

/// 单段流水线：state processing → utterance → 翻译 → translation；失败降级为 error 事件。
#[allow(clippy::too_many_arguments)]
fn process_text(
    host: &HostHandle,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
    id: u64,
    text: &str,
    lang: Option<&str>,
    translate_cfg: &TranslateConfig,
    history: &mut TranslationHistory,
) {
    set_state(host, shared, generation, "processing", None);
    finish_utterance(
        host,
        shared,
        generation,
        id,
        text,
        lang,
        translate_cfg,
        history,
    );
    set_state(host, shared, generation, "listening", None);
}

/// 近期译文对（连贯性上下文）：跨会话路径共享的小缓冲，最多保留 3 对。
type TranslationHistory = Vec<(String, String)>;

fn push_history(history: &mut TranslationHistory, source: &str, translated: &str) {
    history.push((source.to_string(), translated.to_string()));
    if history.len() > 3 {
        history.remove(0);
    }
}

/// 翻译调用计时：超阈值发 E3605 非致命告警（指示灯转黄）。
/// 流式提供源逐 token 经 `TOPIC_PARTIAL_TRANSLATION` 上屏（累计文本），显著降低感知延迟。
fn timed_translate(
    host: &HostHandle,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
    cfg: &TranslateConfig,
    text: &str,
    history: &TranslationHistory,
) -> Result<String, PluginError> {
    let t0 = std::time::Instant::now();
    let result = translate_with_context_stream(cfg, text, history, &mut |acc| {
        if is_current(shared, generation) {
            emit(
                host,
                TOPIC_PARTIAL_TRANSLATION,
                json!({ "text": acc, "source": text }),
            );
        }
    });
    let elapsed = t0.elapsed();
    if elapsed > SLOW_REQUEST_THRESHOLD {
        emit_error(
            host,
            shared,
            generation,
            ERR_SLOW,
            &format!("provider response slow ({:.1}s)", elapsed.as_secs_f32()),
        );
    }
    result
}

/// utterance 事件 + 翻译 + translation 事件（翻译失败降级为 error 事件）。
#[allow(clippy::too_many_arguments)]
fn finish_utterance(
    host: &HostHandle,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
    id: u64,
    text: &str,
    lang: Option<&str>,
    translate_cfg: &TranslateConfig,
    history: &mut TranslationHistory,
) {
    if !is_current(shared, generation) {
        return;
    }
    emit(
        host,
        TOPIC_UTTERANCE,
        json!({ "id": id, "text": text, "lang": lang }),
    );
    // 目标语言直通：识别语言已是目标语言 → 不经翻译直接输出
    if langs_match(lang, &translate_cfg.target) {
        emit(
            host,
            TOPIC_TRANSLATION,
            json!({
                "id": id, "text": text, "source": text,
                "target": translate_cfg.target, "passthrough": true,
            }),
        );
        return;
    }
    match timed_translate(host, shared, generation, translate_cfg, text, history) {
        Ok(translated) => {
            push_history(history, text, &translated);
            if is_current(shared, generation) {
                emit(
                    host,
                    TOPIC_TRANSLATION,
                    json!({
                        "id": id,
                        "text": translated,
                        "source": text,
                        "target": translate_cfg.target,
                    }),
                );
            }
        }
        Err(e) => emit_error(host, shared, generation, ERR_TRANSLATE, &e.to_string()),
    }
}

/// 仅发 utterance 事件（实时路径：翻译交给 worker，不阻塞 IO 线程）。
fn emit_utterance_only(
    host: &HostHandle,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
    id: u64,
    text: &str,
    lang: Option<&str>,
) {
    if !is_current(shared, generation) {
        return;
    }
    emit(
        host,
        TOPIC_UTTERANCE,
        json!({ "id": id, "text": text, "lang": lang }),
    );
}

/// 翻译工作项（实时路径）：partial 中间稿（最新优先，过期丢弃）/ final 断句（保证送达）。
enum WorkItem {
    Partial {
        text: String,
        lang: Option<String>,
    },
    Final {
        id: u64,
        text: String,
        lang: Option<String>,
    },
}

/// 目标语言直通判定：识别语言与目标语言同族（忽略区域后缀）时无需翻译。
fn langs_match(lang: Option<&str>, target: &str) -> bool {
    fn base(code: &str) -> &str {
        code.split(['-', '_']).next().unwrap_or(code)
    }
    match lang {
        Some(lang) => base(&lang.to_lowercase()) == base(&target.to_lowercase()),
        None => false,
    }
}

/// 翻译 worker 池：partial 单通道（最新稿优先折叠积压）+ final 三并发（不排队等中间稿）。
struct TranslationWorker {
    partial_tx: std::sync::mpsc::Sender<WorkItem>,
    final_tx: std::sync::mpsc::Sender<WorkItem>,
}

fn spawn_translator(
    host: HostHandle,
    shared: Arc<Mutex<SharedState>>,
    generation: u64,
    translate_cfg: TranslateConfig,
) -> TranslationWorker {
    // 近期译文对上下文（final 成功后追加；多 worker 共享）
    let history = Arc::new(Mutex::new(TranslationHistory::new()));

    // final 通道：3 个并发 worker 共享接收端（final 不互相等待，乱序完成由前端按 id 归位）
    let (final_tx, final_rx) = std::sync::mpsc::channel::<WorkItem>();
    let final_rx = Arc::new(Mutex::new(final_rx));
    for i in 0..3 {
        let rx = final_rx.clone();
        let shared = shared.clone();
        let cfg = translate_cfg.clone();
        let history = history.clone();
        std::thread::Builder::new()
            .name(format!("zannen-translator-final-{i}"))
            .spawn(move || loop {
                let item = match rx.lock() {
                    Ok(guard) => match guard.recv() {
                        Ok(item) => item,
                        Err(_) => return,
                    },
                    Err(_) => return,
                };
                let WorkItem::Final { id, text, lang } = item else {
                    continue;
                };
                if !is_current(&shared, generation) {
                    continue;
                }
                // 目标语言直通：输入已是目标语言 → 不经翻译直接输出
                if langs_match(lang.as_deref(), &cfg.target) {
                    emit(
                        &host,
                        TOPIC_TRANSLATION,
                        json!({
                            "id": id, "text": text, "source": text,
                            "target": cfg.target, "passthrough": true,
                        }),
                    );
                    continue;
                }
                let hist = history.lock().map(|h| h.clone()).unwrap_or_default();
                match timed_translate(&host, &shared, generation, &cfg, &text, &hist) {
                    Ok(translated) => {
                        if let Ok(mut h) = history.lock() {
                            push_history(&mut h, &text, &translated);
                        }
                        if is_current(&shared, generation) {
                            emit(
                                &host,
                                TOPIC_TRANSLATION,
                                json!({
                                    "id": id, "text": translated, "source": text,
                                    "target": cfg.target,
                                }),
                            );
                        }
                    }
                    Err(e) => emit_error(&host, &shared, generation, ERR_TRANSLATE, &e.to_string()),
                }
            })
            .expect("spawn translator final worker");
    }

    // partial 通道：单 worker，最新稿优先（队列折叠）
    let (partial_tx, partial_rx) = std::sync::mpsc::channel::<WorkItem>();
    {
        let shared = shared.clone();
        let cfg = translate_cfg.clone();
        std::thread::Builder::new()
            .name("zannen-translator-partial".into())
            .spawn(move || {
                while let Ok(item) = partial_rx.recv() {
                    // 折叠积压：只留最新 partial
                    let mut work = item;
                    while let Ok(next) = partial_rx.try_recv() {
                        work = next;
                    }
                    let WorkItem::Partial { text, lang } = work else {
                        continue;
                    };
                    if !is_current(&shared, generation) {
                        continue;
                    }
                    // 目标语言直通：中间稿同义直出
                    if langs_match(lang.as_deref(), &cfg.target) {
                        emit(
                            &host,
                            TOPIC_PARTIAL_TRANSLATION,
                            json!({ "text": text, "source": text, "passthrough": true }),
                        );
                        continue;
                    }
                    let hist = history.lock().map(|h| h.clone()).unwrap_or_default();
                    match timed_translate(&host, &shared, generation, &cfg, &text, &hist) {
                        Ok(translated) => {
                            if is_current(&shared, generation) {
                                emit(
                                    &host,
                                    TOPIC_PARTIAL_TRANSLATION,
                                    json!({ "text": translated, "source": text }),
                                );
                            }
                        }
                        Err(e) => {
                            emit_error(&host, &shared, generation, ERR_TRANSLATE, &e.to_string())
                        }
                    }
                }
            })
            .expect("spawn translator partial worker");
    }

    TranslationWorker {
        partial_tx,
        final_tx,
    }
}

// ---------- mock 会话（离线演示） ----------

fn run_mock(
    host: HostHandle,
    translate_cfg: TranslateConfig,
    stop: &AtomicBool,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
) {
    let mut mock = MockStt::default();
    let mut id = 0u64;
    let mut history: TranslationHistory = Vec::new();
    set_state(&host, shared, generation, "listening", None);
    while !stop.load(Ordering::Relaxed) {
        // 2.5s 一句示例，100ms 粒度响应停止
        let mut waited = 0u64;
        while waited < 2500 && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
            waited += 100;
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        id += 1;
        let (text, lang) = mock.next();
        // mock 无麦克风：产出句子前后发合成语音活动脉冲，指示灯演示一致
        emit(
            &host,
            TOPIC_AUDIO,
            json!({ "level": 0.4, "speaking": true }),
        );
        process_text(
            &host,
            shared,
            generation,
            id,
            &text,
            lang.as_deref(),
            &translate_cfg,
            &mut history,
        );
        emit(
            &host,
            TOPIC_AUDIO,
            json!({ "level": 0.0, "speaking": false }),
        );
    }
}

// ---------- 真实采集会话 ----------

/// 打开默认输入设备并启动采集流（回调内只做 转 f32 → 混音 → 重采样到 16k → 入缓冲）。
/// 失败经 fatal 上报后返回 None。
fn open_capture(
    host: &HostHandle,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
) -> Option<(cpal::Stream, Arc<Mutex<Vec<f32>>>)> {
    use cpal::traits::{HostTrait, StreamTrait};

    let cpal_host = cpal::default_host();
    let Some(device) = cpal_host.default_input_device() else {
        fatal(
            host,
            shared,
            generation,
            ERR_AUDIO_DEVICE,
            "no audio input device available (check microphone permission)",
        );
        return None;
    };
    let supported = match pick_input_config(&device) {
        Ok(c) => c,
        Err(e) => {
            fatal(
                host,
                shared,
                generation,
                ERR_AUDIO_DEVICE,
                &format!("failed to open audio input: {e}"),
            );
            return None;
        }
    };
    let sample_format = supported.sample_format();
    let stream_config: cpal::StreamConfig = supported.into();
    host.log(
        3,
        &format!(
            "translator capture: {}Hz {}ch {sample_format:?}",
            stream_config.sample_rate.0, stream_config.channels,
        ),
    );

    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let stream = match build_input_stream(
        &device,
        &stream_config,
        sample_format,
        *host,
        buffer.clone(),
    ) {
        Ok(s) => s,
        Err(e) => {
            fatal(host, shared, generation, ERR_AUDIO_DEVICE, &e);
            return None;
        }
    };
    if let Err(e) = stream.play() {
        fatal(
            host,
            shared,
            generation,
            ERR_AUDIO_DEVICE,
            &format!("failed to start audio input: {e}"),
        );
        return None;
    }
    Some((stream, buffer))
}

fn run_capture(
    host: HostHandle,
    stt: SttConfig,
    translate_cfg: TranslateConfig,
    stop: &AtomicBool,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
) {
    let Some((stream, buffer)) = open_capture(&host, shared, generation) else {
        return;
    };

    // 主循环：取缓冲 → VAD → 段处理
    set_state(&host, shared, generation, "listening", None);
    let mut vad = VadSegmenter::new(Default::default());
    let mut voice = VoiceActivity::default();
    let mut history: TranslationHistory = Vec::new();
    let mut id = 0u64;
    let mut ticks = 0u64;
    let mut scratch: Vec<f32> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(60));
        scratch.clear();
        if let Ok(mut b) = buffer.lock() {
            std::mem::swap(&mut *b, &mut scratch);
        }
        // 音频电平/语音活动事件（~5Hz，指示灯）
        ticks += 1;
        if ticks.is_multiple_of(3) {
            let peak = scratch.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            emit(
                &host,
                TOPIC_AUDIO,
                json!({ "level": peak, "speaking": voice.update(peak) }),
            );
        }
        for utterance in vad.push(&scratch) {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            id += 1;
            let wav = wav_encode_16k_mono_s16le(&utterance);
            set_state(&host, shared, generation, "processing", None);
            let t0 = std::time::Instant::now();
            let stt_result = transcribe(&stt, &wav);
            let elapsed = t0.elapsed();
            if elapsed > SLOW_REQUEST_THRESHOLD {
                emit_error(
                    &host,
                    shared,
                    generation,
                    ERR_SLOW,
                    &format!("stt response slow ({:.1}s)", elapsed.as_secs_f32()),
                );
            }
            match stt_result {
                Ok((text, lang)) => {
                    if !text.is_empty() {
                        finish_utterance(
                            &host,
                            shared,
                            generation,
                            id,
                            &text,
                            lang.as_deref(),
                            &translate_cfg,
                            &mut history,
                        );
                    }
                }
                Err(e) => emit_error(&host, shared, generation, ERR_STT, &e.to_string()),
            }
            set_state(&host, shared, generation, "listening", None);
        }
    }
    // 停止即弃：残余未完成的段不送识别（flush 仅为语义完整，丢弃返回值）
    drop(vad.flush());
    drop(stream);
}

// ---------- WebSocket 实时流式会话（DashScope run-task 协议） ----------

fn run_realtime(
    host: HostHandle,
    stt: SttConfig,
    translate_cfg: TranslateConfig,
    stop: Arc<AtomicBool>,
    shared: &Arc<Mutex<SharedState>>,
    generation: u64,
) {
    use tungstenite::Message;

    let Some((stream, buffer)) = open_capture(&host, shared, generation) else {
        return;
    };

    // 建连（Authorization: bearer <key>）。
    // 注意：必须经 IntoClientRequest 由 URL 生成完整握手请求（自动生成
    // sec-websocket-key 等 WS 头），再追加鉴权头——手工 http::Request::builder
    // 构造的请求会被透传，缺少 WS 握手头导致协议错误。
    use tungstenite::client::IntoClientRequest;
    let mut request = match stt.endpoint.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => {
            return fatal(
                &host,
                shared,
                generation,
                ERR_STT,
                &format!("ws request build failed: {e}"),
            );
        }
    };
    let auth_value: tungstenite::http::HeaderValue = match format!("bearer {}", stt.api_key).parse()
    {
        Ok(v) => v,
        Err(e) => {
            return fatal(
                &host,
                shared,
                generation,
                ERR_STT,
                &format!("ws auth header invalid: {e}"),
            );
        }
    };
    request.headers_mut().insert("Authorization", auth_value);
    let (ws, _resp) = match tungstenite::connect(request) {
        Ok(x) => x,
        Err(e) => {
            return fatal(
                &host,
                shared,
                generation,
                ERR_STT,
                &format!("ws connect failed: {e}"),
            );
        }
    };
    let mut ws = ws;
    let task_id = gen_task_id();
    let language_hints: Vec<String> = if stt.language != "auto" {
        vec![stt.language.clone()]
    } else {
        Vec::new()
    };
    if let Err(e) = ws.send(Message::Text(
        build_run_task(&task_id, &stt.model, &language_hints).into(),
    )) {
        return fatal(
            &host,
            shared,
            generation,
            ERR_STT,
            &format!("ws run-task send failed: {e}"),
        );
    }

    // 关键：读写不能共享互斥锁上的阻塞 read——接收线程持锁阻塞时发送线程
    // 会被饿死（音频发不出去，服务端必然空闲超时）。改为单线程 IO 循环：
    // 底层流设 50ms 读超时作 tick，同一线程交替处理入站事件与出站通道。
    match ws.get_mut() {
        tungstenite::stream::MaybeTlsStream::Rustls(s) => {
            let _ = s
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(50)));
        }
        tungstenite::stream::MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(Some(Duration::from_millis(50)));
        }
        _ => {}
    }

    /// 出站帧（音频二进制 / finish-task 文本 / 关闭）。
    enum Outbound {
        Binary(Vec<u8>),
        Text(String),
        Close,
    }

    // 翻译 worker：partial/final 的翻译与事件接收解耦（HTTP 不阻塞 IO 循环）
    let worker = spawn_translator(host, shared.clone(), generation, translate_cfg);

    let (out_tx, out_rx) = std::sync::mpsc::channel::<Outbound>();
    let started = Arc::new(AtomicBool::new(false));
    let reader_error = Arc::new(Mutex::new(None::<String>));
    let started_io = started.clone();
    let error_io = reader_error.clone();
    let host_io = host;
    let shared_io = shared.clone();
    let lang_hint = (stt.language != "auto").then(|| stt.language.clone());
    let partial_tx_io = worker.partial_tx.clone();
    let final_tx_io = worker.final_tx.clone();
    let io = std::thread::Builder::new()
        .name("zannen-translator-ws-io".into())
        .spawn(move || {
            let mut ws = ws;
            let mut id = 0u64;
            loop {
                // 1) 先出站（音频帧优先送达）
                loop {
                    match out_rx.try_recv() {
                        Ok(Outbound::Binary(b)) => {
                            if ws.send(Message::Binary(b.into())).is_err() {
                                *error_io.lock().unwrap() = Some("ws audio send failed".into());
                                return;
                            }
                        }
                        Ok(Outbound::Text(t)) => {
                            let _ = ws.send(Message::Text(t.into()));
                        }
                        Ok(Outbound::Close) => {
                            let _ = ws.close(None);
                            return;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                // 2) 读一帧（50ms 超时作 tick）
                match ws.read() {
                    Ok(Message::Text(text)) => match parse_server_event(&text) {
                        ServerEvent::TaskStarted => {
                            started_io.store(true, Ordering::Relaxed);
                        }
                        ServerEvent::Result { text, sentence_end } => {
                            if text.is_empty() {
                                continue;
                            }
                            if sentence_end {
                                id += 1;
                                // utterance 立即上行（不等翻译）；翻译交 worker
                                emit_utterance_only(
                                    &host_io,
                                    &shared_io,
                                    generation,
                                    id,
                                    &text,
                                    lang_hint.as_deref(),
                                );
                                let _ = final_tx_io.send(WorkItem::Final {
                                    id,
                                    text,
                                    lang: lang_hint.clone(),
                                });
                            } else if is_current(&shared_io, generation) {
                                emit(&host_io, TOPIC_PARTIAL, json!({ "text": text }));
                                let _ = partial_tx_io.send(WorkItem::Partial {
                                    text,
                                    lang: lang_hint.clone(),
                                });
                            }
                        }
                        ServerEvent::TaskFinished => return,
                        ServerEvent::TaskFailed(m) => {
                            *error_io.lock().unwrap() = Some(m);
                            return;
                        }
                        ServerEvent::Other => {}
                    },
                    Ok(_) => {} // ping/pong/binary 帧忽略
                    Err(tungstenite::Error::Io(e))
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => {
                        *error_io.lock().unwrap() = Some(format!("ws read: {e}"));
                        return;
                    }
                }
            }
        })
        .expect("spawn translator ws io");

    // 等 task-started（≤10s）
    let mut waited = 0u64;
    while !started.load(Ordering::Relaxed) && waited < 10_000 {
        if stop.load(Ordering::Relaxed) || reader_error.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        waited += 50;
    }
    let startup_error = reader_error.lock().unwrap().clone();
    if let Some(e) = startup_error {
        let _ = out_tx.send(Outbound::Close);
        let _ = io.join();
        return fatal(
            &host,
            shared,
            generation,
            ERR_STT,
            &format!("ws task failed: {e}"),
        );
    }
    if !started.load(Ordering::Relaxed) {
        let _ = out_tx.send(Outbound::Close);
        let _ = io.join();
        if !stop.load(Ordering::Relaxed) {
            return fatal(
                &host,
                shared,
                generation,
                ERR_STT,
                "timeout waiting task-started from ASR service",
            );
        }
        return;
    }

    // 推流主循环：每 100ms 取缓冲 → s16le 裸流经出站通道发送
    set_state(&host, shared, generation, "listening", None);
    let mut scratch: Vec<f32> = Vec::new();
    let mut voice = VoiceActivity::default();
    let mut ticks = 0u64;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
        scratch.clear();
        if let Ok(mut b) = buffer.lock() {
            std::mem::swap(&mut *b, &mut scratch);
        }
        if reader_error.lock().unwrap().is_some() {
            break;
        }
        // 实时 ASR 要求持续推流：静音时段也要发静音帧（服务端自身做 VAD）。
        // 静音帧是全零，电平判定天然归为静音——指示灯不误亮。
        let is_silence_frame = scratch.is_empty();
        if is_silence_frame {
            scratch.resize(SILENCE_FRAME_SAMPLES, 0.0);
        }
        // 音频电平/语音活动事件（每 200ms；指示灯驱动）
        ticks += 1;
        if ticks.is_multiple_of(2) {
            let peak = scratch.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            let speaking = !is_silence_frame && voice.update(peak);
            emit(
                &host,
                TOPIC_AUDIO,
                json!({ "level": peak, "speaking": speaking }),
            );
        }
        // 每 ~5s 记录一次峰值电平（诊断"采集流是否有信号"）
        if ticks.is_multiple_of(50) {
            let peak = scratch.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            host.log(3, &format!("translator realtime audio peak: {peak:.4}"));
        }
        if out_tx
            .send(Outbound::Binary(f32_to_s16le_bytes(&scratch)))
            .is_err()
        {
            break;
        }
    }

    // 优雅收尾：finish-task → 等 task-finished（≤3s）→ 关流
    let _ = out_tx.send(Outbound::Text(build_finish_task(&task_id)));
    let mut waited = 0u64;
    while !io.is_finished() && waited < 3000 {
        std::thread::sleep(Duration::from_millis(50));
        waited += 50;
    }
    let _ = out_tx.send(Outbound::Close);
    let _ = io.join();
    drop(stream);

    // 非本端停止导致的断开按错误上报
    let tail_error = reader_error.lock().unwrap().clone();
    if !stop.load(Ordering::Relaxed) {
        if let Some(e) = tail_error {
            fatal(&host, shared, generation, ERR_STT, &e);
        }
    }
}

/// 按采样格式分发到具体类型的流构建（覆盖常见格式，其余报 E3601）。
fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    host: HostHandle,
    buffer: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, String> {
    use cpal::SampleFormat as F;
    match format {
        F::F32 => build_typed::<f32>(device, config, host, buffer),
        F::F64 => build_typed::<f64>(device, config, host, buffer),
        F::I16 => build_typed::<i16>(device, config, host, buffer),
        F::I32 => build_typed::<i32>(device, config, host, buffer),
        F::U16 => build_typed::<u16>(device, config, host, buffer),
        other => Err(format!("unsupported sample format: {other:?}")),
    }
}

/// 构建一路输入流：回调内 转 f32 → 混音 → 线性重采样到 16k → 入共享缓冲。
fn build_typed<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    host: HostHandle,
    buffer: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;

    let channels = config.channels;
    let src_rate = config.sample_rate.0;
    let mut resampler = LinearResampler::new(src_rate, TARGET_SAMPLE_RATE);
    let data_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
        let converted: Vec<f32> = data
            .iter()
            .map(|&x| <f32 as cpal::Sample>::from_sample(x))
            .collect();
        let mono = downmix_to_mono(&converted, channels);
        let out = resampler.push(&mono);
        if let Ok(mut b) = buffer.lock() {
            b.extend_from_slice(&out);
        }
    };
    let err_cb = move |e: cpal::StreamError| {
        host.log(2, &format!("translator audio stream error: {e}"));
    };
    device
        .build_input_stream(config, data_cb, err_cb, None)
        .map_err(|e| format!("failed to open audio input: {e}"))
}

/// 选输入流配置：优先支持 16kHz 的配置（省重采样，偏好与设备默认相同的
/// 声道数/采样格式），否则退回设备默认配置（后续线性重采样到 16k）。
fn pick_input_config(
    device: &cpal::Device,
) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
    use cpal::traits::DeviceTrait;

    let default = device.default_input_config()?;
    if default.sample_rate().0 == TARGET_SAMPLE_RATE {
        return Ok(default);
    }
    let mut fallback: Option<cpal::SupportedStreamConfig> = None;
    for range in device.supported_input_configs()? {
        let covers_16k = range.min_sample_rate().0 <= TARGET_SAMPLE_RATE
            && TARGET_SAMPLE_RATE <= range.max_sample_rate().0;
        if !covers_16k {
            continue;
        }
        let candidate = range.with_sample_rate(cpal::SampleRate(TARGET_SAMPLE_RATE));
        if candidate.channels() == default.channels()
            && candidate.sample_format() == default.sample_format()
        {
            return Ok(candidate);
        }
        fallback.get_or_insert(candidate);
    }
    Ok(fallback.unwrap_or(default))
}

#[cfg(test)]
mod tests {
    use super::langs_match;

    #[test]
    fn target_language_passthrough_matching() {
        assert!(langs_match(Some("zh"), "zh"));
        assert!(langs_match(Some("zh-CN"), "zh"));
        assert!(langs_match(Some("zh"), "zh-CN"));
        assert!(langs_match(Some("EN"), "en"));
        assert!(!langs_match(Some("en"), "zh"));
        assert!(!langs_match(None, "zh"));
        assert!(!langs_match(Some("ja"), "zh-TW"));
    }
}
