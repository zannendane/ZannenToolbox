//! FFI 辅助函数。供 `export_plugin!` 宏展开代码与宿主侧（zannen-core）使用。
//!
//! 直接调用这些函数需要自行保证指针有效性；正常业务代码不应触碰本模块。

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// 读取宿主/插件传入的 C 字符串为拥有的 `String`。
///
/// # Safety
/// `ptr` 必须为有效的 NUL 结尾 UTF-8 字符串，或 NULL（返回 Err）。
///
/// 返回拥有的 `String` 而非借用，规避生命周期跨越 FFI 边界的风险。
pub unsafe fn cstr_to_str(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("null string pointer".to_string());
    }
    CStr::from_ptr(ptr)
        .to_str()
        .map(str::to_owned)
        .map_err(|e| format!("invalid utf-8: {e}"))
}

/// 将 Rust 字符串移交为 C 字符串（调用方负责经对应的 free 函数释放）。
///
/// 字符串内含 NUL 时不会 panic，而是退化为一个 err JSON。
pub fn into_raw_c_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(e) => {
            let fallback =
                serde_json::json!({"err": format!("nul byte in string: {e}")}).to_string();
            CString::new(fallback)
                .expect("fallback json contains no nul")
                .into_raw()
        }
    }
}

/// 释放由 `into_raw_c_string` 产生的字符串。NULL 安全。
///
/// # Safety
/// `s` 必须来自本 crate 的 `into_raw_c_string`，且只能释放一次。
pub unsafe fn free_c_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// 运行 `f` 并把结果包装为 `{"ok": <json>}` / `{"err": "..."}`，panic 转为 err。
///
/// 用于 `zannen_plugin_invoke`：闭包返回的 `String` 必须是合法 JSON 值文本。
pub fn catch_to_json_string<F>(f: F) -> *mut c_char
where
    F: FnOnce() -> Result<String, String>,
{
    let json = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(value_json)) => format!("{{\"ok\":{value_json}}}"),
        Ok(Err(err)) => err_json(&err),
        Err(payload) => err_json(&format!("panic: {}", panic_message(&payload))),
    };
    into_raw_c_string(json)
}

/// 运行 `f`，成功返回其原始字符串，失败/panic 返回 NULL。
///
/// 用于 `zannen_plugin_manifest` 这类"裸字符串"导出。
pub fn catch_to_null_string<F>(f: F) -> *mut c_char
where
    F: FnOnce() -> Result<String, String>,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(s)) => into_raw_c_string(s),
        _ => std::ptr::null_mut(),
    }
}

fn err_json(msg: &str) -> String {
    serde_json::json!({"err": msg}).to_string()
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}
