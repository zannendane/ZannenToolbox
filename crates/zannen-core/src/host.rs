//! 宿主侧 ABI 粘合层：构造注入插件的 `ZannenHostVTable`，
//! 并把 `host_call` 路由到进程内的 `ServiceDispatcher` 单例。
//!
//! C ABI 函数无法携带 Rust 侧上下文指针（vtable.context 保留），
//! 因此分发器经 `OnceLock` 全局注册。单进程单壳是设计前提。

use std::os::raw::{c_char, c_void};
use std::sync::{Arc, OnceLock};

use zannen_plugin_api::ffi::{catch_to_json_string, cstr_to_str, free_c_string};
use zannen_plugin_api::{ZannenHostVTable, ZANNEN_ABI_VERSION};

use crate::services::ServiceDispatcher;

static DISPATCHER: OnceLock<Arc<ServiceDispatcher>> = OnceLock::new();

/// 注册进程级服务分发器（启动时调用一次；重复调用保留首个）。
pub fn install_dispatcher(dispatcher: Arc<ServiceDispatcher>) {
    let _ = DISPATCHER.set(dispatcher);
}

pub fn global_dispatcher() -> Option<Arc<ServiceDispatcher>> {
    DISPATCHER.get().cloned()
}

unsafe extern "C" fn host_call_shim(
    service: *const c_char,
    method: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    catch_to_json_string(|| {
        let service = unsafe { cstr_to_str(service) }?;
        let method = unsafe { cstr_to_str(method) }?;
        let args_raw = unsafe { cstr_to_str(args_json) }?;
        let args: serde_json::Value =
            serde_json::from_str(&args_raw).map_err(|e| format!("bad args json: {e}"))?;
        let dispatcher =
            global_dispatcher().ok_or_else(|| "dispatcher not installed".to_string())?;
        let result = dispatcher.call(&service, &method, args)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    })
}

unsafe extern "C" fn host_free_string_shim(s: *mut c_char) {
    unsafe { free_c_string(s) }
}
/// 构造传给插件的 vtable。`Box::leak` 固化，生命周期覆盖所有插件。
pub fn host_vtable() -> &'static ZannenHostVTable {
    Box::leak(Box::new(ZannenHostVTable {
        abi_version: ZANNEN_ABI_VERSION,
        host_call: host_call_shim,
        host_free_string: host_free_string_shim,
        context: std::ptr::null_mut() as *mut c_void,
    }))
}
