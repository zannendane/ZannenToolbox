//! 插件管理器：扫描插件目录 → 校验清单 → 加载 dylib → ABI 握手 → 路由调用。
//!
//! ## 插件目录布局
//!
//! ```text
//! plugins/
//! └── zannen.debugger/
//!     ├── plugin.toml          # 清单（与 dylib 导出的 manifest 交叉校验）
//!     ├── libzannen_debugger.dylib / zannen_debugger.dll / libzannen_debugger.so
//!     └── frontend/dist/index.js (+ style.css)
//! ```
//!
//! ## 安全与健壮性
//!
//! - ABI 版本必须完全等于宿主的 `ZANNEN_ABI_VERSION`，否则拒绝加载；
//! - 每次 invoke 外层再做一次 `catch_unwind`（插件宏内已有一层，双保险）；
//! - 插件实例串行化调用（Mutex），插件内部可自由使用自有线程；
//! - 动态库为原生代码：信任模型与签名路线见 docs/ARCHITECTURE.md §插件信任模型。

use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use libloading::Library;
use parking_lot::RwLock;
use serde_json::Value;
use zannen_plugin_api::ffi::cstr_to_str;
use zannen_plugin_api::{PluginManifest, ZannenHostVTable, ZANNEN_ABI_VERSION};

#[derive(Debug, thiserror::Error)]
pub enum PluginLoadError {
    #[error("[E2101] IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("[E2102] plugin.toml parse failed: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("[E2103] dynamic library load failed: {0}")]
    Library(String),
    #[error("[E2104] missing exported symbol: {0}")]
    MissingSymbol(String),
    #[error("[E2105] ABI version mismatch: plugin {got}, host {want}")]
    AbiMismatch { got: u32, want: u32 },
    #[error("[E2106] exported manifest does not match plugin.toml: {0}")]
    ManifestMismatch(String),
    #[error("[E2107] plugin init failed (init returned NULL)")]
    InitFailed,
}

/// 插件导出的 C ABI 函数指针集（在 `Library` 存活期内有效）。
#[derive(Clone, Copy)]
struct PluginFns {
    init: unsafe extern "C" fn(*const ZannenHostVTable) -> *mut c_void,
    invoke: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> *mut c_char,
    on_event: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char),
    destroy: unsafe extern "C" fn(*mut c_void),
    free_string: unsafe extern "C" fn(*mut c_char),
}

/// 一个已加载的插件。析构顺序：先 `destroy` 实例，再卸载库（字段顺序保证）。
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
    fns: PluginFns,
    instance: Mutex<Option<*mut c_void>>,
    // 保持库加载直到插件销毁；必须最后声明（最后 drop）。
    _lib: Library,
}

// 实例指针受 Mutex 保护串行访问；`Library` 本身 Send/Sync。
unsafe impl Send for LoadedPlugin {}
unsafe impl Sync for LoadedPlugin {}

impl LoadedPlugin {
    /// 同步调用插件方法，返回 `"ok"` 分支的值。
    pub fn invoke(&self, method: &str, args: &Value) -> Result<Value, String> {
        let guard = self
            .instance
            .lock()
            .map_err(|_| "plugin lock poisoned".to_string())?;
        let instance = guard.ok_or_else(|| "plugin destroyed".to_string())?;

        let c_method = CString::new(method).map_err(|e| e.to_string())?;
        let c_args = CString::new(serde_json::to_string(args).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;

        let raw = catch_unwind(AssertUnwindSafe(|| unsafe {
            (self.fns.invoke)(instance, c_method.as_ptr(), c_args.as_ptr())
        }))
        .map_err(|_| "plugin panicked across ffi boundary".to_string())?;

        if raw.is_null() {
            return Err("plugin returned null".to_string());
        }
        let text = unsafe { cstr_to_str(raw) }?;
        unsafe { (self.fns.free_string)(raw) };

        let parsed: Value =
            serde_json::from_str(&text).map_err(|e| format!("plugin returned bad json: {e}"))?;
        if let Some(err) = parsed.get("err") {
            return Err(err.as_str().unwrap_or("unknown plugin error").to_string());
        }
        Ok(parsed.get("ok").cloned().unwrap_or(Value::Null))
    }

    /// 向插件分发事件（尽力而为，单插件失败不影响其他）。
    pub fn dispatch_event(&self, topic: &str, payload: &Value) {
        let Ok(guard) = self.instance.lock() else {
            return;
        };
        let Some(instance) = *guard else { return };
        let (Ok(c_topic), Ok(c_payload)) = (CString::new(topic), CString::new(payload.to_string()))
        else {
            return;
        };
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            (self.fns.on_event)(instance, c_topic.as_ptr(), c_payload.as_ptr());
        }));
    }
}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.instance.lock() {
            if let Some(instance) = guard.take() {
                let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
                    (self.fns.destroy)(instance);
                }));
            }
        }
    }
}

/// 平台动态库文件名。
pub(crate) fn platform_lib_name(crate_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{crate_name}.dll")
    }
    #[cfg(target_os = "macos")]
    {
        format!("lib{crate_name}.dylib")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        format!("lib{crate_name}.so")
    }
}

/// 加载单个插件目录。
fn load_one(dir: &Path) -> Result<Arc<LoadedPlugin>, PluginLoadError> {
    let toml_text = std::fs::read_to_string(dir.join("plugin.toml"))?;
    let disk_manifest: PluginManifest = toml::from_str(&toml_text)?;

    let backend = disk_manifest
        .backend
        .clone()
        .ok_or_else(|| PluginLoadError::ManifestMismatch("missing [backend] declaration".into()))?;
    let lib_path = dir.join(platform_lib_name(&backend.name));

    // SAFETY: 加载的是插件目录下的约定文件名；信任模型见模块文档。
    let lib = unsafe { Library::new(&lib_path) }
        .map_err(|e| PluginLoadError::Library(format!("{}: {e}", lib_path.display())))?;

    unsafe fn sym<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, PluginLoadError> {
        let s: libloading::Symbol<T> = lib
            .get(name)
            .map_err(|_| PluginLoadError::MissingSymbol(String::from_utf8_lossy(name).into()))?;
        Ok(*s)
    }

    let abi_version: unsafe extern "C" fn() -> u32 = unsafe { sym(&lib, b"zannen_abi_version") }?;
    let got = unsafe { abi_version() };
    if got != ZANNEN_ABI_VERSION {
        return Err(PluginLoadError::AbiMismatch {
            got,
            want: ZANNEN_ABI_VERSION,
        });
    }

    let manifest_fn: unsafe extern "C" fn() -> *mut c_char =
        unsafe { sym(&lib, b"zannen_plugin_manifest") }?;
    let fns = PluginFns {
        init: unsafe { sym(&lib, b"zannen_plugin_init") }?,
        invoke: unsafe { sym(&lib, b"zannen_plugin_invoke") }?,
        on_event: unsafe { sym(&lib, b"zannen_plugin_on_event") }?,
        destroy: unsafe { sym(&lib, b"zannen_plugin_destroy") }?,
        free_string: unsafe { sym(&lib, b"zannen_plugin_free_string") }?,
    };

    // 交叉校验 dylib 导出清单与 plugin.toml。
    let raw = unsafe { manifest_fn() };
    if raw.is_null() {
        return Err(PluginLoadError::ManifestMismatch(
            "dylib manifest is NULL".into(),
        ));
    }
    let exported_text = unsafe { cstr_to_str(raw) }.map_err(PluginLoadError::ManifestMismatch)?;
    unsafe { (fns.free_string)(raw) };
    let exported: PluginManifest = serde_json::from_str(&exported_text)
        .map_err(|e| PluginLoadError::ManifestMismatch(e.to_string()))?;
    if exported.id != disk_manifest.id || exported.version != disk_manifest.version {
        return Err(PluginLoadError::ManifestMismatch(format!(
            "dylib={}@{} toml={}@{}",
            exported.id, exported.version, disk_manifest.id, disk_manifest.version
        )));
    }
    if exported.api != ZANNEN_ABI_VERSION {
        return Err(PluginLoadError::AbiMismatch {
            got: exported.api,
            want: ZANNEN_ABI_VERSION,
        });
    }

    let instance = unsafe { (fns.init)(crate::host::host_vtable()) };
    if instance.is_null() {
        return Err(PluginLoadError::InitFailed);
    }

    log::info!(
        "plugin loaded: {} v{} ({})",
        disk_manifest.id,
        disk_manifest.version,
        dir.display()
    );
    Ok(Arc::new(LoadedPlugin {
        manifest: disk_manifest,
        dir: dir.to_path_buf(),
        fns,
        instance: Mutex::new(Some(instance)),
        _lib: lib,
    }))
}

/// 插件集合：扫描/加载/卸载/调用路由/事件广播。
#[derive(Default)]
pub struct PluginManager {
    plugins: RwLock<HashMap<String, Arc<LoadedPlugin>>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 扫描目录并加载全部插件。单个失败不影响其他，错误进入返回的报告。
    pub fn scan_and_load(&self, dir: &Path) -> Vec<Result<String, String>> {
        let mut report = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("plugin dir {} unreadable: {e}", dir.display());
                return report;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || !path.join("plugin.toml").exists() {
                continue;
            }
            match load_one(&path) {
                Ok(plugin) => {
                    let id = plugin.manifest.id.clone();
                    self.plugins.write().insert(id.clone(), plugin);
                    report.push(Ok(id));
                }
                Err(e) => {
                    log::error!("plugin load failed at {}: {e}", path.display());
                    report.push(Err(format!("{}: {e}", path.display())));
                }
            }
        }
        report
    }

    pub fn list(&self) -> Vec<PluginManifest> {
        self.plugins
            .read()
            .values()
            .map(|p| p.manifest.clone())
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<LoadedPlugin>> {
        self.plugins.read().get(id).cloned()
    }

    /// 调用路由：`plugin_invoke` Tauri 命令的落点。
    pub fn invoke(&self, id: &str, method: &str, args: &Value) -> Result<Value, String> {
        let Some(plugin) = self.get(id) else {
            return Err(format!("plugin not loaded: {id}"));
        };
        plugin.invoke(method, args)
    }

    /// 广播总线事件给所有插件。
    pub fn dispatch_event(&self, topic: &str, payload: &Value) {
        for plugin in self.plugins.read().values() {
            plugin.dispatch_event(topic, payload);
        }
    }

    /// 卸载全部插件（应用退出时）。
    pub fn unload_all(&self) {
        self.plugins.write().clear();
    }

    /// 热重载单个插件：销毁实例并卸载动态库 → 从目录重新加载。
    ///
    /// 用于在线更新后的即时生效。插件当前打开的句柄（如串口会话）
    /// 会随实例销毁而释放，调用方应提示用户。
    pub fn reload(&self, id: &str) -> Result<PluginManifest, String> {
        let dir = {
            let existing = self
                .plugins
                .read()
                .get(id)
                .map(|p| p.dir.clone())
                .ok_or_else(|| format!("plugin not loaded: {id}"))?;
            existing
        };
        // 先卸载（Drop 会调用插件 destroy），再重新加载。
        self.plugins.write().remove(id);
        match load_one(&dir) {
            Ok(plugin) => {
                let manifest = plugin.manifest.clone();
                self.plugins.write().insert(id.to_string(), plugin);
                log::info!("plugin reloaded: {id}");
                Ok(manifest)
            }
            Err(e) => Err(format!("reload {id}: {e}")),
        }
    }

    /// 从指定目录加载（或替换同 id）插件。用于在线更新后的覆盖装载：
    /// 新版本先加载成功才替换旧实例，加载失败不影响现有插件。
    pub fn load_dir(&self, dir: &Path) -> Result<PluginManifest, String> {
        let plugin = load_one(dir).map_err(|e| format!("load {}: {e}", dir.display()))?;
        let manifest = plugin.manifest.clone();
        // insert 替换旧实例；旧实例 Drop 时调用插件 destroy。
        self.plugins.write().insert(manifest.id.clone(), plugin);
        log::info!(
            "plugin loaded from dir: {} ({})",
            manifest.id,
            dir.display()
        );
        Ok(manifest)
    }

    /// 已加载同 id 插件的版本（用户目录遮蔽判定用）。
    pub fn version_of(&self, id: &str) -> Option<String> {
        self.plugins
            .read()
            .get(id)
            .map(|p| p.manifest.version.clone())
    }

    /// 仅当目录内插件版本严格高于 current 时才加载遮蔽；否则跳过（Ok(None)）。
    pub fn load_dir_if_newer(
        &self,
        dir: &Path,
        current: Option<&str>,
    ) -> Result<Option<PluginManifest>, String> {
        let text = std::fs::read_to_string(dir.join("plugin.toml"))
            .map_err(|e| format!("read manifest {}: {e}", dir.display()))?;
        let disk: PluginManifest =
            toml::from_str(&text).map_err(|e| format!("parse manifest {}: {e}", dir.display()))?;
        if !should_shadow(current, &disk.version) {
            log::info!(
                "user plugin {}@{} not newer than loaded version {:?}; skipped",
                disk.id,
                disk.version,
                current
            );
            return Ok(None);
        }
        self.load_dir(dir).map(Some)
    }
}

/// 遮蔽判定：无已加载版本 → 遮蔽；SemVer 严格更大 → 遮蔽；
/// 版本串解析失败时回退为"不相等才遮蔽"（保守接受用户版，避免卡死更新通道）。
pub fn should_shadow(current: Option<&str>, candidate: &str) -> bool {
    match current {
        None => true,
        Some(cur) => match (
            semver::Version::parse(candidate),
            semver::Version::parse(cur),
        ) {
            (Ok(new), Ok(cur)) => new > cur,
            _ => candidate != cur,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::should_shadow;

    #[test]
    fn shadow_rules() {
        // 无已加载版本 → 遮蔽
        assert!(should_shadow(None, "0.1.0"));
        // 严格更高 → 遮蔽
        assert!(should_shadow(Some("0.5.4"), "0.5.5"));
        assert!(should_shadow(Some("0.5.4"), "0.6.0"));
        // 相等 / 更低 → 不遮蔽（出厂版生效，陈旧用户版被忽略）
        assert!(!should_shadow(Some("0.5.4"), "0.5.4"));
        assert!(!should_shadow(Some("0.5.4"), "0.5.3"));
        assert!(!should_shadow(Some("1.0.0"), "0.9.9"));
        // 解析失败回退：不相等才遮蔽
        assert!(should_shadow(Some("dev"), "0.5.4"));
        assert!(!should_shadow(Some("dev"), "dev"));
    }
}
