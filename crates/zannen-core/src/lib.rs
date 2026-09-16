//! # ZannenToolbox 宿主核心
//!
//! - [`EventBus`]：异步事件总线（服务/插件 → 前端）
//! - [`DeviceRegistry`]：统一设备视图
//! - [`PluginManager`]：原生动态库插件的扫描、加载、调用路由
//! - [`services::ServiceDispatcher`]：暴露给插件的宿主服务（串口/BLE/UF2/设备/事件/日志）
//! - [`host`]：C ABI 粘合层
//!
//! 本 crate 不依赖 Tauri，可独立测试；Tauri 壳在 `app-shell/src-tauri`。

pub mod devices;
pub mod events;
pub mod host;
pub mod plugin_installer;
pub mod plugin_manager;
pub mod services;

use std::path::Path;
use std::sync::Arc;

pub use devices::{DeviceInfo, DeviceRegistry};
pub use events::{Event, EventBus};
pub use plugin_installer::{install_archive, sha256_hex, InstallReport};
pub use plugin_manager::{LoadedPlugin, PluginManager};
pub use services::ServiceDispatcher;

/// 宿主运行时：事件总线 + 服务分发器 + 插件管理器的组合体。
pub struct Core {
    pub bus: EventBus,
    pub dispatcher: Arc<ServiceDispatcher>,
    pub plugins: PluginManager,
}

impl Core {
    /// 初始化宿主：创建服务并安装全局分发器（供插件 host_call 使用）。
    pub fn new() -> Arc<Self> {
        let dispatcher = ServiceDispatcher::new();
        host::install_dispatcher(dispatcher.clone());
        Arc::new(Self {
            bus: dispatcher.bus.clone(),
            dispatcher,
            plugins: PluginManager::new(),
        })
    }

    /// 从目录加载插件，返回逐插件报告。
    pub fn load_plugins_from(&self, dir: &Path) -> Vec<Result<String, String>> {
        self.plugins.scan_and_load(dir)
    }

    /// 加载用户目录插件（在线更新落点）：逐目录比较版本，
    /// 仅当用户版严格高于已加载同 id 版本（通常为出厂版）才遮蔽——
    /// 相等或更低的陈旧用户插件被忽略，出厂新版生效。
    pub fn load_user_plugins(&self, dir: &Path) -> Vec<Result<String, String>> {
        let mut report = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("user plugin dir {} unreadable: {e}", dir.display());
                return report;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || !path.join("plugin.toml").exists() {
                continue;
            }
            // 先读清单拿 id，再与已加载版本比较决定是否遮蔽
            let manifest: zannen_plugin_api::PluginManifest =
                match std::fs::read_to_string(path.join("plugin.toml"))
                    .ok()
                    .and_then(|t| toml::from_str(&t).ok())
                {
                    Some(m) => m,
                    None => {
                        report.push(Err(format!("{}: unreadable manifest", path.display())));
                        continue;
                    }
                };
            let current = self.plugins.version_of(&manifest.id);
            match self.plugins.load_dir_if_newer(&path, current.as_deref()) {
                Ok(Some(m)) => report.push(Ok(m.id)),
                Ok(None) => {} // 版本不更高：跳过并已在 core 内记日志
                Err(e) => report.push(Err(e)),
            }
        }
        report
    }
}
