//! 插件侧调用宿主的句柄。

use std::ffi::CString;
use std::os::raw::c_void;

use serde_json::{json, Value};

use crate::{ffi, PluginError, ZannenHostVTable};

/// 宿主服务句柄。由 `zannen_plugin_init` 传入的 vtable 构造，可跨线程使用。
///
/// 所有调用都是同步的；串口接收等异步数据流通过宿主事件
/// （`on_event` 回调 + `emit` 上行）传输。
#[derive(Clone, Copy)]
pub struct HostHandle {
    vtable: *const ZannenHostVTable,
}

// vtable 由宿主以 Box::leak 固化，host_call 实现必须线程安全（宿主侧契约）。
unsafe impl Send for HostHandle {}
unsafe impl Sync for HostHandle {}

impl HostHandle {
    /// # Safety
    /// `vtable` 必须指向宿主提供的、生命周期覆盖插件存活期的 `ZannenHostVTable`。
    pub unsafe fn from_raw(vtable: *const ZannenHostVTable) -> Self {
        debug_assert!(!vtable.is_null());
        Self { vtable }
    }

    /// 宿主 ABI 版本。
    pub fn abi_version(&self) -> u32 {
        unsafe { (*self.vtable).abi_version }
    }

    /// 同步调用宿主服务，返回 `"ok"` 分支的值。
    pub fn call(&self, service: &str, method: &str, args: &Value) -> Result<Value, PluginError> {
        let vt = unsafe { &*self.vtable };
        let c_service = CString::new(service).map_err(|e| PluginError::BadArgs(e.to_string()))?;
        let c_method = CString::new(method).map_err(|e| PluginError::BadArgs(e.to_string()))?;
        let c_args = CString::new(serde_json::to_string(args)?)
            .map_err(|e| PluginError::BadArgs(e.to_string()))?;

        let raw = unsafe { (vt.host_call)(c_service.as_ptr(), c_method.as_ptr(), c_args.as_ptr()) };
        if raw.is_null() {
            return Err(PluginError::Host("host returned null".to_string()));
        }
        // cstr_to_str 复制出内容后即可释放宿主字符串。
        let text = unsafe { ffi::cstr_to_str(raw) }.map_err(PluginError::Host)?;
        unsafe { (vt.host_free_string)(raw) };

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| PluginError::Host(format!("bad host json: {e}")))?;
        if let Some(err) = parsed.get("err") {
            let msg = err.as_str().unwrap_or("unknown host error").to_string();
            return Err(PluginError::Host(msg));
        }
        Ok(parsed.get("ok").cloned().unwrap_or(Value::Null))
    }

    /// 向总线发布事件（经宿主转发到前端与所有订阅方）。
    ///
    /// 高频数据（如 IMU 批）请遵循宿主约定使用 `imu.batch` 等批量化主题，
    /// 避免逐条事件淹没 IPC。
    pub fn emit(&self, topic: &str, payload: Value) -> Result<(), PluginError> {
        self.call(
            "events",
            "emit",
            &json!({ "topic": topic, "payload": payload }),
        )?;
        Ok(())
    }

    /// 写一条宿主日志。`level`: 1=error 2=warn 3=info 4=debug 5=trace。
    pub fn log(&self, level: u8, msg: &str) {
        let _ = self.call("log", "write", &json!({ "level": level, "msg": msg }));
    }

    /// 供宿主侧保留的原始上下文指针（当前恒为 NULL）。
    pub fn context(&self) -> *mut c_void {
        unsafe { (*self.vtable).context }
    }
}
