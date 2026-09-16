//! # ZannenToolbox 壳程序（Tauri 2）
//!
//! 职责：
//! - 持有 `zannen_core::Core`（服务 + 插件管理器）
//! - 暴露 Tauri 命令：`plugin_list` / `plugin_invoke` / `shell_info`
//! - `plugin-asset://` 自定义协议：向前端提供插件目录内的 ESM/CSS 等资源
//! - 事件转发：总线事件 → 前端 `zannen-bus`；硬件侧事件回投插件
//!   （`serial.*` / `device.*` / `ble.*`，插件自身 emit 的主题不回投，避免回声循环）

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use zannen_core::Core;

struct AppState {
    core: Arc<Core>,
    plugin_dir: PathBuf,
}

/// 已加载插件清单列表（壳前端据此装配路由与侧边栏）。
#[tauri::command]
async fn plugin_list(
    state: State<'_, AppState>,
) -> Result<Vec<zannen_plugin_api::PluginManifest>, String> {
    Ok(state.core.plugins.list())
}

/// 通用插件调用路由：前端 → 指定插件的 invoke。
#[tauri::command]
async fn plugin_invoke(
    state: State<'_, AppState>,
    plugin: String,
    method: String,
    args: Value,
) -> Result<Value, String> {
    let core = state.inner().core.clone();
    tauri::async_runtime::spawn_blocking(move || core.plugins.invoke(&plugin, &method, &args))
        .await
        .map_err(|e| format!("plugin invoke join: {e}"))?
}

/// 壳信息：版本 / ABI / 插件目录 / 已装载插件 id / 构建目标三元组。
#[tauri::command]
async fn shell_info(state: State<'_, AppState>) -> Result<Value, String> {
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "abi": zannen_plugin_api::ZANNEN_ABI_VERSION,
        "plugin_dir": state.plugin_dir.to_string_lossy(),
        "plugins": state.core.plugins.list().iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        "target": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    }))
}

/// 内置更新源默认配置（app_data 下 update-sources.json 可覆盖）。
const UPDATE_SOURCES_BUILTIN: &str = include_str!("../update-sources.json");

/// 读取更新源配置：app_data 覆盖文件优先，其次内置默认。
#[tauri::command]
async fn update_sources(app: AppHandle) -> Result<Value, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?;
    let override_path = dir.join("update-sources.json");
    match std::fs::read_to_string(&override_path) {
        Ok(text) => {
            serde_json::from_str(&text).map_err(|e| format!("update-sources override: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            serde_json::from_str(UPDATE_SOURCES_BUILTIN)
                .map_err(|e| format!("builtin sources: {e}"))
        }
        Err(e) => Err(format!("read override sources: {e}")),
    }
}

// ---------- 首启状态持久化（全新/升级识别） ----------

fn state_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create app data dir: {e}"))?;
    Ok(dir.join("zannen-state.json"))
}

/// 读取壳状态（首启版本标记等）。不存在时返回 null。
#[tauri::command]
async fn state_read(app: AppHandle) -> Result<Value, String> {
    let path = state_file_path(&app)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("state json: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(format!("read state: {e}")),
    }
}

/// 写入壳状态（整体覆盖）。
#[tauri::command]
async fn state_write(app: AppHandle, state: Value) -> Result<(), String> {
    let path = state_file_path(&app)?;
    let text = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("write state: {e}"))
}

// ---------- 插件在线更新（安装 + 热重载） ----------

/// 从本地 .znplugin 包安装/更新插件并热重载。
/// 安装落点为用户插件目录（应用数据目录/plugins），覆盖出厂内置版本。
/// `signature` 为包文件的 ed25519 签名（hex，随更新清单下发）；
/// 未提供签名时仅当 `allow_unsigned` 为 true 才安装（缺省 false 即拒绝）。
#[tauri::command]
async fn plugin_install(
    app: AppHandle,
    state: State<'_, AppState>,
    plugin_id: String,
    archive_path: String,
    sha256: Option<String>,
    signature: Option<String>,
    allow_unsigned: Option<bool>,
) -> Result<Value, String> {
    let user_dir = user_plugin_dir(&app)?;
    let core = state.inner().core.clone();
    let (report, manifest) = tauri::async_runtime::spawn_blocking(move || {
        let report = zannen_core::install_archive(
            &user_dir,
            &plugin_id,
            Path::new(&archive_path),
            sha256.as_deref(),
            signature.as_deref(),
            allow_unsigned.unwrap_or(false),
        )
        .map_err(|e| e.to_string())?;
        // 从用户目录加载新版本（同 id 覆盖旧实例）
        let manifest = core.plugins.load_dir(&user_dir.join(&plugin_id))?;
        let _ = std::fs::remove_file(&archive_path);
        Ok::<_, String>((report, manifest))
    })
    .await
    .map_err(|e| format!("plugin install join: {e}"))??;
    Ok(json!({ "report": report, "manifest": manifest }))
}

/// 窗口扩缩动画。macOS 走 NSWindow 原生动画（零 IPC 洪泛、系统级平滑、
/// 且 webview 不做逐帧重排，消除泛白托尾与卡顿）；其他平台返回 false 由前端步进插值。
#[cfg_attr(target_os = "macos", allow(clippy::needless_return))]
#[tauri::command]
fn animate_resize(
    app: AppHandle,
    width: f64,
    height: f64,
    theme: Option<String>,
) -> Result<Value, String> {
    let Some(window) = app.get_webview_window("main") else {
        return Err("no main window".to_string());
    };
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSAnimatablePropertyContainer, NSAnimationContext, NSWindow};
        use objc2_core_foundation::{CGPoint, CGRect, CGSize};
        use objc2_quartz_core::CAMediaTimingFunction;
        let ns_window = window.ns_window().map_err(|e| e.to_string())? as *mut NSWindow;
        if ns_window.is_null() {
            return Err("null ns_window".to_string());
        }
        unsafe {
            // 动画前重申底色：起始帧新暴露区也不透白
            let theme = theme.as_deref().unwrap_or("dark");
            set_ns_window_bg(&window, theme);
            set_webview_underpage_bg(&window, theme);
            let ns_window = &*ns_window;
            let frame = ns_window.frame();
            let content_h = ns_window
                .contentView()
                .map(|v| v.frame().size.height)
                .unwrap_or(height);
            let chrome = (frame.size.height - content_h).max(0.0);
            let target = CGSize::new(width, height + chrome);
            let center = CGPoint::new(
                frame.origin.x + frame.size.width / 2.0,
                frame.origin.y + frame.size.height / 2.0,
            );
            let origin = CGPoint::new(
                center.x - target.width / 2.0,
                center.y - target.height / 2.0,
            );
            let expand = target.width * target.height > frame.size.width * frame.size.height;
            // 原生连续动画 + 自定义 timingFunction：
            // 放大带回弹感的控制点，缩小为强减速曲线。
            let (c1x, c1y, c2x, c2y) = if expand {
                (0.25f32, 1.18f32, 0.38f32, 1.0f32)
            } else {
                (0.2f32, 0.75f32, 0.3f32, 1.0f32)
            };
            NSAnimationContext::beginGrouping();
            let ctx = NSAnimationContext::currentContext();
            ctx.setDuration(if expand { 0.48 } else { 0.4 });
            let timing = CAMediaTimingFunction::functionWithControlPoints(c1x, c1y, c2x, c2y);
            ctx.setTimingFunction(Some(&timing));
            ns_window
                .animator()
                .setFrame_display_animate(CGRect::new(origin, target), true, true);
            NSAnimationContext::endGrouping();
        }
        return Ok(json!({ "animated": true }));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (width, height, window);
        Ok(json!({ "animated": false }))
    }
}

/// 重启应用（updater 安装完成后调用）。
#[tauri::command]
fn app_restart(app: AppHandle) {
    app.restart();
}

/// 导出文本文件（终端日志 / CSV 等用户选择的保存路径）。
#[tauri::command]
fn export_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("write {path}: {e}"))
}

/// 打开系统对应权限的设置页（bluetooth / microphone），便于用户在被拒后手动恢复授权。
#[tauri::command]
fn open_permission_settings(kind: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let pane = match kind.as_str() {
            "bluetooth" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Bluetooth"
            }
            "microphone" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            _ => return Err(format!("unsupported permission kind: {kind}")),
        };
        std::process::Command::new("open")
            .arg(pane)
            .spawn()
            .map_err(|e| format!("open settings: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        let page = match kind.as_str() {
            "bluetooth" => "ms-settings:bluetooth",
            "microphone" => "ms-settings:privacy-microphone",
            _ => return Err(format!("unsupported permission kind: {kind}")),
        };
        std::process::Command::new("cmd")
            .args(["/c", "start", page])
            .spawn()
            .map_err(|e| format!("open settings: {e}"))?;
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = kind;
        return Err("open_permission_settings unsupported on this platform".into());
    }
    Ok(())
}

// ---------- 插件悬浮窗（overlay） ----------

/// overlay 窗口 label 前缀；主窗口关闭行为据此判断是否有存活 overlay。
const OVERLAY_LABEL_PREFIX: &str = "overlay-";

/// 插件 id 安全字符（用于拼进窗口 label 与 URL query）。
fn valid_overlay_plugin_id(plugin: &str) -> bool {
    !plugin.is_empty()
        && plugin.len() <= 64
        && plugin
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// 是否有任一 overlay-* 窗口存活。
fn has_overlay_windows(app: &AppHandle) -> bool {
    app.webview_windows()
        .keys()
        .any(|l| l.starts_with(OVERLAY_LABEL_PREFIX))
}

/// 插件 id → 窗口 label：Tauri 窗口 label 仅允许字母数字与 `-` `/` `:` `_`，
/// 插件 id 中的 `.` 等字符统一替换为 `-`（如 zannen.translator → overlay-zannen-translator）。
fn overlay_label(plugin: &str) -> String {
    let sanitized: String = plugin
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '/' | ':' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{OVERLAY_LABEL_PREFIX}{sanitized}")
}

/// 打开（或聚焦）某插件的悬浮窗：透明、无边框、置顶，加载同一前端入口，
/// 由 `?overlay=<plugin>` 查询参数切换到 OverlayHost 渲染该插件的 overlay 组件。
#[tauri::command]
fn overlay_open(app: AppHandle, state: State<'_, AppState>, plugin: String) -> Result<(), String> {
    if !valid_overlay_plugin_id(&plugin) {
        return Err(format!("invalid plugin id for overlay: {plugin}"));
    }
    let label = overlay_label(&plugin);
    if let Some(win) = app.get_webview_window(&label) {
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    // 标题用插件显示名（任务栏/窗口列表里可辨识），悬浮窗是独立任务栏项
    let title = state
        .core
        .plugins
        .list()
        .into_iter()
        .find(|m| m.id == plugin)
        .map(|m| m.name)
        .unwrap_or_else(|| plugin.clone());
    let url = tauri::WebviewUrl::App(format!("index.html?overlay={plugin}").into());
    let win = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(title)
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(false)
        .resizable(true)
        .inner_size(560.0, 300.0)
        .min_inner_size(420.0, 220.0)
        .shadow(true)
        .build()
        .map_err(|e| format!("create overlay window: {e}"))?;
    // 最后一个 overlay 关闭且主窗口处于隐藏态时重新示出主窗口：
    // 应用无托盘，避免成为不可见的孤儿进程。
    // （Destroyed 触发时本窗口可能尚未从窗口表移除，按 label 排除自身）
    let handle = app.clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let any_other_overlay = handle
                .webview_windows()
                .keys()
                .any(|l| l.starts_with(OVERLAY_LABEL_PREFIX) && *l != label);
            if !any_other_overlay {
                if let Some(main) = handle.get_webview_window("main") {
                    if !main.is_visible().unwrap_or(true) {
                        let _ = main.show();
                        let _ = main.set_focus();
                    }
                }
            }
        }
    });
    Ok(())
}

/// 关闭某插件的悬浮窗（不存在时静默成功）。
#[tauri::command]
fn overlay_close(app: AppHandle, plugin: String) -> Result<(), String> {
    if !valid_overlay_plugin_id(&plugin) {
        return Err(format!("invalid plugin id for overlay: {plugin}"));
    }
    let label = overlay_label(&plugin);
    if let Some(win) = app.get_webview_window(&label) {
        win.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- 主题与图标（深浅色跟随） ----------

/// 读取真实系统外观（不受窗口主题钉死影响）：
/// macOS 主线程读 NSApp.effectiveAppearance；Windows 读个性化注册表；其余默认深色。
#[tauri::command]
async fn system_theme(app: AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<&'static str>();
        app.run_on_main_thread(move || {
            use objc2::MainThreadMarker;
            use objc2_app_kit::NSApplication;
            let theme = match MainThreadMarker::new() {
                Some(mtm) => {
                    let appearance = NSApplication::sharedApplication(mtm).effectiveAppearance();
                    let dark = appearance
                        .name()
                        .to_string()
                        .to_lowercase()
                        .contains("dark");
                    if dark {
                        "dark"
                    } else {
                        "light"
                    }
                }
                None => "dark",
            };
            let _ = tx.send(theme);
        })
        .map_err(|e| e.to_string())?;
        return rx.await.map(str::to_string).map_err(|e| e.to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        let dark = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
            .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
            .map(|v| v == 0)
            .unwrap_or(false);
        return Ok(if dark { "dark".into() } else { "light".into() });
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = app;
        Ok("dark".into())
    }
}

/// 主题切换时同步原生窗口主题（Windows 边框/对话框等）。
///
/// `mode` 为 "auto" 时解除窗口主题钉死（`set_theme(None)` = 跟随系统）——
/// 否则 WKWebView/WebView2 的 prefers-color-scheme 会跟随被钉死的窗口外观，
/// 导致前端的 auto 解析读到的是旧的手动选择而非真实系统主题。
/// `theme` 始终是解析后的主题（深/浅），用于窗口与 webview 底色。
#[tauri::command]
fn set_window_theme(app: AppHandle, theme: String, mode: Option<String>) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window missing".to_string())?;
    let pin = match mode.as_deref().unwrap_or(theme.as_str()) {
        "dark" => Some(tauri::Theme::Dark),
        "light" => Some(tauri::Theme::Light),
        _ => None, // auto：解除钉死，跟随系统
    };
    window.set_theme(pin).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        set_ns_window_bg(&window, theme.as_str());
        set_webview_underpage_bg(&window, theme.as_str());
    }
    Ok(())
}

/// 按主题设置 WKWebView 底层背景色（扩缩动画起始帧 webview 尚未重排时，
/// 新暴露区域显示的是该底色——不透白）。
#[cfg(target_os = "macos")]
fn set_webview_underpage_bg(window: &tauri::WebviewWindow, theme: &str) {
    let (r, g, b) = if theme == "light" {
        (0.933, 0.945, 0.969)
    } else {
        (0.043, 0.051, 0.075)
    };
    let result = window.with_webview(move |webview| unsafe {
        use objc2_app_kit::NSColor;
        let wk = webview.inner();
        if wk.is_null() {
            log::warn!("underpage bg: null WKWebView handle");
            return;
        }
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0);
        (&*(wk as *mut objc2_web_kit::WKWebView)).setUnderPageBackgroundColor(Some(&color));
    });
    if let Err(e) = result {
        log::warn!("underpage bg: with_webview failed: {e}");
    }
}

/// 按主题设置 NSWindow 背景色（令牌底色，避免动画时白底透出）。
#[cfg(target_os = "macos")]
fn set_ns_window_bg(window: &tauri::WebviewWindow, theme: &str) {
    let Ok(ns_window) = window.ns_window() else {
        return;
    };
    if ns_window.is_null() {
        return;
    }
    unsafe {
        use objc2_app_kit::{NSColor, NSWindow};
        let ns_window = &*(ns_window as *mut NSWindow);
        // dark: #0B0D13, light: #EEF1F7
        let (r, g, b) = if theme == "light" {
            (0.933, 0.945, 0.969)
        } else {
            (0.043, 0.051, 0.075)
        };
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0);
        ns_window.setBackgroundColor(Some(&color));
    }
}

/// 运行时图标（内嵌 512² PNG；dock 图标用 1024 母版在构建期已入 icns，
/// 此处负责运行时可编程部分：Windows 任务栏窗口图标 + macOS Dock 图标）。
const ICON_DARK_PNG: &[u8] = include_bytes!("../icons/icon-dark-512.png");
const ICON_LIGHT_PNG: &[u8] = include_bytes!("../icons/icon-light-512.png");

#[tauri::command]
fn set_app_icon(app: AppHandle, theme: String) -> Result<(), String> {
    let bytes = if theme == "light" {
        ICON_LIGHT_PNG
    } else {
        ICON_DARK_PNG
    };
    let image = tauri::image::Image::from_bytes(bytes).map_err(|e| e.to_string())?;

    // Windows/Linux：窗口图标（任务栏/Alt-Tab）
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_icon(image.clone());
    }

    // macOS：Dock 图标（NSApplication.setApplicationIconImage，须主线程）
    #[cfg(target_os = "macos")]
    {
        let png: &'static [u8] = bytes;
        let _ = app.run_on_main_thread(move || unsafe {
            use objc2::MainThreadMarker;
            use objc2_app_kit::{NSApplication, NSImage};
            use objc2_foundation::NSData;
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let data = NSData::with_bytes(png);
            if let Some(img) = NSImage::initWithData(mtm.alloc::<NSImage>(), &data) {
                NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&img));
            }
        });
    }
    Ok(())
}

/// 插件目录解析顺序：环境变量 → 可执行文件旁 plugins/ → macOS bundle 内 Resources/plugins
/// → 开发态 crate 相对路径。这是"出厂内置"目录（打包后只读）。
fn resolve_bundled_plugin_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ZANNEN_PLUGIN_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("plugins");
            if candidate.is_dir() {
                return Some(candidate);
            }
            // macOS .app: Contents/MacOS/../Resources/plugins
            let bundle_res = dir.join("../Resources/plugins");
            if bundle_res.is_dir() {
                return Some(bundle_res);
            }
        }
    }
    // 开发态回退：src-tauri/plugins（构建脚本装配点）
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

/// 用户插件目录（可写）：在线更新的落点；同 id 覆盖出厂内置版本。
fn user_plugin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("plugins");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create plugin dir: {e}"))?;
    Ok(dir)
}

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let core = Core::new();
    let bundled_dir = resolve_bundled_plugin_dir();
    if let Some(dir) = &bundled_dir {
        log::info!("bundled plugin dir: {}", dir.display());
        for entry in core.load_plugins_from(dir) {
            match entry {
                Ok(id) => log::info!("plugin ready: {id}"),
                Err(e) => log::error!("plugin load failed: {e}"),
            }
        }
    } else {
        log::warn!("no bundled plugin dir found");
    }

    let protocol_bundled = bundled_dir.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(AppState {
            core: core.clone(),
            plugin_dir: bundled_dir.clone().unwrap_or_default(),
        })
        .invoke_handler(tauri::generate_handler![
            plugin_list,
            plugin_invoke,
            shell_info,
            update_sources,
            state_read,
            state_write,
            plugin_install,
            app_restart,
            animate_resize,
            export_text_file,
            open_permission_settings,
            overlay_open,
            overlay_close,
            set_window_theme,
            set_app_icon,
            system_theme,
        ])
        .register_uri_scheme_protocol("plugin-asset", move |ctx, request| {
            // 用户目录（更新版）优先，出厂内置兜底
            let user = ctx
                .app_handle()
                .path()
                .app_data_dir()
                .map(|d| d.join("plugins"))
                .unwrap_or_default();
            serve_plugin_asset(
                &[user, protocol_bundled.clone().unwrap_or_default()],
                request.uri().path(),
            )
        })
        .setup(move |app| {
            // 用户插件目录（在线更新版）：仅当版本高于出厂同 id 版本才遮蔽
            if let Ok(user_dir) = user_plugin_dir(app.handle()) {
                if user_dir.is_dir() {
                    for entry in core.load_user_plugins(&user_dir) {
                        match entry {
                            Ok(id) => log::info!("user plugin ready: {id}"),
                            Err(e) => log::error!("user plugin load failed: {e}"),
                        }
                    }
                }
            }
            // 启动即设置窗口背景色（默认深色；主题切换时再跟随）
            #[cfg(target_os = "macos")]
            if let Some(main) = app.get_webview_window("main") {
                set_ns_window_bg(&main, "dark");
                set_webview_underpage_bg(&main, "dark");
            }
            // 主菜单窗口（main）是枢纽：无 overlay 存活时关闭即退出（连带全部插件子窗口）；
            // 有 overlay 存活时仅隐藏主窗口（悬浮窗仍可独立工作，dock 点图标经 Reopen 唤回）
            if let Some(main) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if has_overlay_windows(&handle) {
                            api.prevent_close();
                            if let Some(main) = handle.get_webview_window("main") {
                                let _ = main.hide();
                            }
                        } else {
                            handle.exit(0);
                        }
                    }
                });
            }
            let handle = app.handle().clone();
            let core = core.clone();
            tauri::async_runtime::spawn(async move {
                let mut rx = core.bus.subscribe();
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            if event.topic.starts_with("serial.")
                                || event.topic.starts_with("device.")
                                || event.topic.starts_with("ble.")
                            {
                                core.plugins.dispatch_event(&event.topic, &event.payload);
                            }
                            let _ = handle.emit(
                                "zannen-bus",
                                json!({ "topic": event.topic, "payload": event.payload }),
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            log::warn!("event bus lagged, dropped {n} events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            log::info!("zannen-toolbox shell ready");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building ZannenToolbox");

    app.run(|handle, event| {
        // macOS：dock 点图标时唤回隐藏的主窗口（overlay 独立存活场景）
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen {
            has_visible_windows,
            ..
        } = event
        {
            if !has_visible_windows {
                if let Some(main) = handle.get_webview_window("main") {
                    let _ = main.show();
                    let _ = main.set_focus();
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (handle, event);
    });
}

/// `plugin-asset://localhost/<plugin_dir相对路径>` → 文件内容。
/// 按根目录顺序查找（用户目录在前 = 更新版优先）。
///
/// 路径安全：拒绝 `..` 与符号链接逃逸（canonicalize 后必须仍在插件根内）。
fn serve_plugin_asset(roots: &[PathBuf], rel_path: &str) -> tauri::http::Response<Vec<u8>> {
    let not_found = || {
        tauri::http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap()
    };
    let rel = rel_path.trim_start_matches('/');
    if rel.is_empty()
        || Path::new(rel)
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        return not_found();
    }
    for root in roots {
        if root.as_os_str().is_empty() {
            continue;
        }
        let candidate = root.join(rel);
        let (Ok(canon), Ok(canon_root)) = (candidate.canonicalize(), root.canonicalize()) else {
            continue;
        };
        if !canon.starts_with(&canon_root) || !canon.is_file() {
            continue;
        }
        return match std::fs::read(&canon) {
            Ok(bytes) => tauri::http::Response::builder()
                .status(200)
                .header("Content-Type", mime_for(&canon))
                // 允许模块脚本跨 scheme 引用
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes)
                .unwrap_or_else(|_| not_found()),
            Err(_) => not_found(),
        };
    }
    not_found()
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" | "map" => "application/json",
        "html" => "text/html",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}
