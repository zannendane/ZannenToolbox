//! 插件清单的 serde 类型。与插件目录内 `plugin.toml` 一一对应。

use serde::{Deserialize, Serialize};

/// 插件清单。插件 dylib 通过 `zannen_plugin_manifest` 导出同一份信息，
/// 宿主加载时与 `plugin.toml` 交叉校验。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// 全局唯一 id，反向域名风格，如 `zannen.debugger`。
    pub id: String,
    /// 显示名，如 `Zannen 调试器`。
    pub name: String,
    /// 插件自身版本（semver）。
    pub version: String,
    /// 要求的 ABI 版本，须等于宿主的 `ZANNEN_ABI_VERSION`。
    pub api: u32,
    #[serde(default)]
    pub description: String,
    /// 侧边栏图标（lucide 图标名）。
    #[serde(default)]
    pub icon: Option<String>,
    /// 前端声明；纯后端插件可为 None。
    #[serde(default)]
    pub frontend: Option<FrontendDecl>,
    /// 后端声明（动态库 crate 名）；纯前端插件可为 None。
    #[serde(default)]
    pub backend: Option<BackendDecl>,
    /// 能力声明（如 `serial` / `ble` / `uf2`），供权限与审计使用。
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// 前端路由声明。
    #[serde(default)]
    pub routes: Vec<PluginRoute>,
}

/// 插件后端动态库声明。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendDecl {
    /// 动态库 crate 名（不含平台前后缀/扩展名），如 `zannen_debugger`。
    pub name: String,
}

/// 插件前端入口（相对于插件目录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendDecl {
    /// 单文件 ESM，如 `frontend/dist/index.js`。
    /// 模块默认导出 `PluginModule`（见 packages/plugin-sdk）。
    pub entry: String,
    /// 可选样式表，如 `frontend/dist/style.css`。
    #[serde(default)]
    pub css: Option<String>,
}

/// 插件贡献的一个前端路由。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRoute {
    /// 路由路径，在插件命名空间下唯一，如 `devices`。
    pub path: String,
    /// 侧边栏显示名。
    pub title: String,
    /// lucide 图标名。
    #[serde(default)]
    pub icon: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip() {
        let m = PluginManifest {
            id: "zannen.debugger".into(),
            name: "Zannen Debugger".into(),
            version: "0.1.0".into(),
            api: 1,
            description: "debugger module".into(),
            icon: Some("bug".into()),
            frontend: Some(FrontendDecl {
                entry: "frontend/dist/index.js".into(),
                css: Some("frontend/dist/style.css".into()),
            }),
            backend: Some(crate::BackendDecl {
                name: "zannen_debugger".into(),
            }),
            capabilities: vec!["serial".into(), "ble".into(), "uf2".into()],
            routes: vec![PluginRoute {
                path: "devices".into(),
                title: "Devices".into(),
                icon: "cpu".into(),
            }],
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "zannen.debugger");
        assert_eq!(back.routes.len(), 1);
        assert_eq!(
            back.frontend.unwrap().css.as_deref(),
            Some("frontend/dist/style.css")
        );
    }

    #[test]
    fn manifest_defaults() {
        let m: PluginManifest =
            serde_json::from_str(r#"{"id":"a.b","name":"B","version":"0.0.1","api":1}"#).unwrap();
        assert!(m.frontend.is_none());
        assert!(m.capabilities.is_empty());
        assert!(m.routes.is_empty());
    }
}
