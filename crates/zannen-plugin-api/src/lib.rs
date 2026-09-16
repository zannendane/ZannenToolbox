//! # ZannenToolbox 插件 ABI 契约
//!
//! 宿主（app-shell / zannen-core）与原生动态库插件（cdylib）共用的契约层。
//!
//! ## 设计要点
//!
//! - **C ABI + 版本握手**：Rust 自身 ABI 不稳定，跨编译器版本加载 dylib 必须走 C ABI。
//!   插件导出 `zannen_abi_version()`，宿主与 `ZANNEN_ABI_VERSION` 不一致即拒绝加载。
//! - **JSON 数据面**：除 vtable 外一切数据以 NUL 结尾 UTF-8 JSON 传递，
//!   返回值为 `{"ok": ...}` 或 `{"err": "..."}`，避免 `repr(C)` 复杂结构的内存布局陷阱。
//! - **内存所有权**：谁分配谁释放——插件返回的字符串用插件导出的
//!   `zannen_plugin_free_string` 释放；宿主 `host_call` 返回的字符串用
//!   `host_free_string` 释放。
//! - **恐慌隔离**：`export_plugin!` 生成的每个导出函数内部都有 `catch_unwind`，
//!   插件 panic 不会跨 FFI 边界传播（UB），而是转为 `{"err": "panic: ..."}`。
//!
//! ## 插件侧用法
//!
//! ```rust,no_run
//! use zannen_plugin_api::{export_plugin, HostHandle, PluginError, PluginManifest, ZannenPlugin};
//!
//! struct MyPlugin { host: HostHandle }
//!
//! impl ZannenPlugin for MyPlugin {
//!     fn manifest() -> PluginManifest { /* ... */ unimplemented!() }
//!     fn new(host: HostHandle) -> Self { Self { host } }
//!     fn invoke(&mut self, method: &str, args: serde_json::Value)
//!         -> Result<serde_json::Value, PluginError>
//!     { unimplemented!() }
//! }
//!
//! export_plugin!(MyPlugin);
//! ```

use std::os::raw::{c_char, c_void};

#[doc(hidden)]
pub mod ffi;
mod host;
mod manifest;

pub use host::HostHandle;
pub use manifest::{BackendDecl, FrontendDecl, PluginManifest, PluginRoute};

/// 当前 ABI 版本。宿主加载插件时要求插件导出的版本与此完全一致。
pub const ZANNEN_ABI_VERSION: u32 = 1;

/// 宿主提供给插件的函数表（`repr(C)`）。
///
/// 宿主保证 vtable 的生命周期覆盖插件的整个存活期（以 `Box::leak` 固化）。
/// 所有字符串参数均为 NUL 结尾的 UTF-8。
#[repr(C)]
pub struct ZannenHostVTable {
    /// 宿主侧 ABI 版本；插件在 `new` 之前可核对。
    pub abi_version: u32,
    /// 同步调用宿主服务。
    ///
    /// - `service`：服务名（`"serial"` / `"ble"` / `"uf2"` / `"devices"` / `"events"` / `"log"`）
    /// - `method`：服务内方法名
    /// - `args_json`：参数 JSON
    ///
    /// 返回 JSON 字符串（`{"ok": ...}` / `{"err": "..."}`），调用方必须用
    /// `host_free_string` 释放。返回 NULL 表示宿主内部错误。
    pub host_call: unsafe extern "C" fn(
        service: *const c_char,
        method: *const c_char,
        args_json: *const c_char,
    ) -> *mut c_char,
    /// 释放 `host_call` 返回的字符串。
    pub host_free_string: unsafe extern "C" fn(s: *mut c_char),
    /// 保留字段，当前恒为 NULL。
    pub context: *mut c_void,
}

// vtable 内为函数指针与只读字段，宿主实现必须保证 host_call 可跨线程调用。
unsafe impl Send for ZannenHostVTable {}
unsafe impl Sync for ZannenHostVTable {}

/// 插件实现错误。序列化为 `{"err": "..."}` 返回给调用方。
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("[E2001] host call failed: {0}")]
    Host(String),
    #[error("[E2002] unknown method: {0}")]
    UnknownMethod(String),
    #[error("[E2003] invalid argument: {0}")]
    BadArgs(String),
    #[error("[E2004] invalid state: {0}")]
    InvalidState(String),
    #[error("[E2005] io error: {0}")]
    Io(String),
    #[error("[E2006] {0}")]
    Other(String),
}

impl From<std::io::Error> for PluginError {
    fn from(e: std::io::Error) -> Self {
        PluginError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for PluginError {
    fn from(e: serde_json::Error) -> Self {
        PluginError::BadArgs(e.to_string())
    }
}

/// 插件实现 trait。通过 `export_plugin!` 宏导出为 C ABI。
///
/// 实例方法的调用均发生在宿主线程/插件自有线程中，`Send` 约束保证
/// 实例可以被安全地转移到插件自己 spawn 的线程里使用。
pub trait ZannenPlugin: Sized + Send + 'static {
    /// 插件清单（每次调用重新构造；开销可忽略）。
    fn manifest() -> PluginManifest;
    /// 构造插件实例。`host` 是调用宿主服务的句柄。
    fn new(host: HostHandle) -> Self;
    /// 处理一次调用。`method` 为 `"<域>.<动作>"` 形式（如 `"serial.open"`）。
    fn invoke(
        &mut self,
        method: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError>;
    /// 处理宿主广播的事件（如设备热插拔）。默认忽略。
    fn on_event(&mut self, _topic: &str, _payload: serde_json::Value) {}
}

/// 生成插件的 C ABI 导出函数。每个插件 cdylib 在 crate 根调用一次。
#[macro_export]
macro_rules! export_plugin {
    ($ty:ty) => {
        #[no_mangle]
        pub extern "C" fn zannen_abi_version() -> u32 {
            $crate::ZANNEN_ABI_VERSION
        }

        #[no_mangle]
        pub extern "C" fn zannen_plugin_manifest() -> *mut ::std::os::raw::c_char {
            $crate::ffi::catch_to_null_string(|| {
                let manifest = <$ty as $crate::ZannenPlugin>::manifest();
                ::serde_json::to_string(&manifest).map_err(|e| e.to_string())
            })
        }

        #[no_mangle]
        pub unsafe extern "C" fn zannen_plugin_init(
            host: *const $crate::ZannenHostVTable,
        ) -> *mut ::std::os::raw::c_void {
            if host.is_null() {
                return ::std::ptr::null_mut();
            }
            ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                let handle = unsafe { $crate::HostHandle::from_raw(host) };
                let instance = <$ty as $crate::ZannenPlugin>::new(handle);
                ::std::boxed::Box::into_raw(::std::boxed::Box::new(instance))
                    as *mut ::std::os::raw::c_void
            }))
            .unwrap_or(::std::ptr::null_mut())
        }

        #[no_mangle]
        pub unsafe extern "C" fn zannen_plugin_invoke(
            instance: *mut ::std::os::raw::c_void,
            method: *const ::std::os::raw::c_char,
            args_json: *const ::std::os::raw::c_char,
        ) -> *mut ::std::os::raw::c_char {
            $crate::ffi::catch_to_json_string(|| {
                if instance.is_null() {
                    return Err("null instance".to_string());
                }
                let plugin = unsafe { &mut *(instance as *mut $ty) };
                let method = unsafe { $crate::ffi::cstr_to_str(method)? };
                let args_raw = unsafe { $crate::ffi::cstr_to_str(args_json)? };
                let args: ::serde_json::Value =
                    ::serde_json::from_str(&args_raw).map_err(|e| e.to_string())?;
                let result = <$ty as $crate::ZannenPlugin>::invoke(plugin, &method, args)
                    .map_err(|e| e.to_string())?;
                ::serde_json::to_string(&result).map_err(|e| e.to_string())
            })
        }

        #[no_mangle]
        pub unsafe extern "C" fn zannen_plugin_on_event(
            instance: *mut ::std::os::raw::c_void,
            topic: *const ::std::os::raw::c_char,
            payload_json: *const ::std::os::raw::c_char,
        ) {
            let _ = ::std::panic::catch_unwind(|| {
                if instance.is_null() {
                    return;
                }
                let plugin = unsafe { &mut *(instance as *mut $ty) };
                let Ok(topic) = (unsafe { $crate::ffi::cstr_to_str(topic) }) else {
                    return;
                };
                let Ok(payload_raw) = (unsafe { $crate::ffi::cstr_to_str(payload_json) }) else {
                    return;
                };
                let payload: ::serde_json::Value =
                    ::serde_json::from_str(&payload_raw).unwrap_or(::serde_json::Value::Null);
                <$ty as $crate::ZannenPlugin>::on_event(plugin, &topic, payload);
            });
        }

        #[no_mangle]
        pub unsafe extern "C" fn zannen_plugin_destroy(instance: *mut ::std::os::raw::c_void) {
            let _ = ::std::panic::catch_unwind(|| {
                if !instance.is_null() {
                    drop(unsafe { ::std::boxed::Box::from_raw(instance as *mut $ty) });
                }
            });
        }

        #[no_mangle]
        pub unsafe extern "C" fn zannen_plugin_free_string(s: *mut ::std::os::raw::c_char) {
            unsafe { $crate::ffi::free_c_string(s) };
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    struct EchoPlugin {
        host: HostHandle,
    }

    impl ZannenPlugin for EchoPlugin {
        fn manifest() -> PluginManifest {
            PluginManifest {
                id: "test.echo".into(),
                name: "Echo".into(),
                version: "0.1.0".into(),
                api: ZANNEN_ABI_VERSION,
                description: "test".into(),
                icon: None,
                frontend: None,
                backend: None,
                capabilities: vec![],
                routes: vec![],
            }
        }

        fn new(host: HostHandle) -> Self {
            Self { host }
        }

        fn invoke(&mut self, method: &str, args: Value) -> Result<Value, PluginError> {
            match method {
                "echo" => Ok(json!({ "echoed": args })),
                "boom" => panic!("intentional panic"),
                "host_ping" => self
                    .host
                    .call("log", "write", &json!({"level": 3, "msg": "ping"})),
                _ => Err(PluginError::UnknownMethod(method.into())),
            }
        }
    }

    export_plugin!(EchoPlugin);

    // —— 宿主侧模拟 ——

    unsafe extern "C" fn mock_host_call(
        service: *const c_char,
        method: *const c_char,
        _args: *const c_char,
    ) -> *mut c_char {
        let service = ffi::cstr_to_str(service).unwrap_or_default();
        let method = ffi::cstr_to_str(method).unwrap_or_default();
        assert_eq!(service, "log");
        assert_eq!(method, "write");
        ffi::into_raw_c_string(r#"{"ok":true}"#.to_string())
    }

    unsafe extern "C" fn mock_host_free(s: *mut c_char) {
        unsafe { ffi::free_c_string(s) }
    }

    fn mock_vtable() -> &'static ZannenHostVTable {
        Box::leak(Box::new(ZannenHostVTable {
            abi_version: ZANNEN_ABI_VERSION,
            host_call: mock_host_call,
            host_free_string: mock_host_free,
            context: std::ptr::null_mut(),
        }))
    }

    fn cstring(s: &str) -> std::ffi::CString {
        std::ffi::CString::new(s).unwrap()
    }

    #[test]
    fn abi_roundtrip() {
        assert_eq!(zannen_abi_version(), ZANNEN_ABI_VERSION);

        // manifest
        let raw = zannen_plugin_manifest();
        assert!(!raw.is_null());
        let manifest_json = unsafe { ffi::cstr_to_str(raw) }.unwrap();
        let manifest: PluginManifest = serde_json::from_str(&manifest_json).unwrap();
        assert_eq!(manifest.id, "test.echo");
        unsafe { zannen_plugin_free_string(raw) };

        // init + invoke
        let inst = unsafe { zannen_plugin_init(mock_vtable()) };
        assert!(!inst.is_null());

        let method = cstring("echo");
        let args = cstring(r#"{"n":42}"#);
        let raw = unsafe { zannen_plugin_invoke(inst, method.as_ptr(), args.as_ptr()) };
        let out_json = unsafe { ffi::cstr_to_str(raw) }.unwrap();
        unsafe { zannen_plugin_free_string(raw) };
        let out: Value = serde_json::from_str(&out_json).unwrap();
        assert_eq!(out, json!({"ok": {"echoed": {"n": 42}}}));

        // 未知方法 → err
        let method = cstring("nope");
        let args = cstring("{}");
        let raw = unsafe { zannen_plugin_invoke(inst, method.as_ptr(), args.as_ptr()) };
        let out_json = unsafe { ffi::cstr_to_str(raw) }.unwrap();
        unsafe { zannen_plugin_free_string(raw) };
        let out: Value = serde_json::from_str(&out_json).unwrap();
        assert!(out.get("err").is_some());

        // host_call 经由 HostHandle 走通
        let method = cstring("host_ping");
        let raw = unsafe { zannen_plugin_invoke(inst, method.as_ptr(), args.as_ptr()) };
        let out_json = unsafe { ffi::cstr_to_str(raw) }.unwrap();
        unsafe { zannen_plugin_free_string(raw) };
        let out: Value = serde_json::from_str(&out_json).unwrap();
        assert_eq!(out, json!({"ok": true}));

        unsafe { zannen_plugin_destroy(inst) };
    }

    #[test]
    fn panic_is_contained() {
        let inst = unsafe { zannen_plugin_init(mock_vtable()) };
        let method = cstring("boom");
        let args = cstring("{}");
        let raw = unsafe { zannen_plugin_invoke(inst, method.as_ptr(), args.as_ptr()) };
        assert!(!raw.is_null());
        let out_json = unsafe { ffi::cstr_to_str(raw) }.unwrap();
        unsafe { zannen_plugin_free_string(raw) };
        let out: Value = serde_json::from_str(&out_json).unwrap();
        let err = out["err"].as_str().unwrap();
        assert!(err.contains("panic"), "expect panic error, got: {err}");
        unsafe { zannen_plugin_destroy(inst) };
    }
}
