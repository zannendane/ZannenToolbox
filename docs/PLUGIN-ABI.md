# 插件 ABI 契约与开发指南

> ABI 版本：1 · 实现：`crates/zannen-plugin-api`（契约）、`crates/zannen-core`（宿主）

## 1. 设计原则

- **C ABI 边界**：Rust ABI 不稳定，跨编译器版本的 dylib 交互必须走 `extern "C"`；
- **版本握手**：插件导出 `zannen_abi_version()`，与宿主的 `ZANNEN_ABI_VERSION` 不完全相等即拒绝加载（破坏式变更直接升主版本）；
- **JSON 数据面**：除 vtable 外一切数据为 NUL 结尾 UTF-8 JSON，响应包装为 `{"ok": ...}` 或 `{"err": "..."}`；
- **内存所有权**：谁分配谁释放——插件返回的字符串用 `zannen_plugin_free_string`，宿主 `host_call` 返回的用 `host_free_string`；
- **恐慌隔离**：导出宏 `export_plugin!` 内置 `catch_unwind`，宿主侧 invoke 再兜一层；插件 panic 表现为 `{"err":"panic: ..."}`，不跨 FFI 传播（那是 UB）；
- **调用串行化**：宿主对单个插件实例的 invoke/on_event 以 Mutex 串行化；插件内部可自由开线程（实例要求 `Send`）。

## 2. 导出符号（由 `export_plugin!` 生成）

| 符号 | 签名 | 说明 |
|---|---|---|
| `zannen_abi_version` | `fn() -> u32` | ABI 版本 |
| `zannen_plugin_manifest` | `fn() -> *mut c_char` | 清单 JSON（裸字符串，非 ok/err 包装） |
| `zannen_plugin_init` | `fn(*const ZannenHostVTable) -> *mut c_void` | 构造实例；NULL = 失败 |
| `zannen_plugin_invoke` | `fn(*mut c_void, method, args_json) -> *mut c_char` | 同步调用，返回 `{"ok"/"err"}` |
| `zannen_plugin_on_event` | `fn(*mut c_void, topic, payload_json)` | 总线事件下发（serial.\*/device.\*/ble.\*） |
| `zannen_plugin_destroy` | `fn(*mut c_void)` | 销毁实例 |
| `zannen_plugin_free_string` | `fn(*mut c_char)` | 释放本插件返回的字符串 |

## 3. 宿主 vtable

```c
struct ZannenHostVTable {
    uint32_t abi_version;
    char* (*host_call)(const char* service, const char* method, const char* args_json);
    void  (*host_free_string)(char* s);
    void* context;  // 保留，恒为 NULL
};
```

插件侧用 `HostHandle`（`host.call(service, method, &json)` / `host.emit(topic, payload)` / `host.log(level, msg)`）访问，无需手写 FFI。

### 宿主服务目录（`host_call` 路由）

| 服务.方法 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `serial.list` | `{}` | `[{path, vid?, pid?, product?, mock}]` | 枚举串口（含 mock 虚拟口） |
| `serial.open` | `{path, baud}` | `{session}` | 打开并启动读线程 |
| `serial.write` | `{session, data, encoding: "text"\|"hex", append: "none"\|"lf"\|"crlf"}` | `{sent}` | 写 |
| `serial.close` | `{session}` | `{closed}` | 关闭 |
| `ble.scan` | `{timeout_ms}` | `{devices: [{id, address, name, rssi, services, mock?}]}` | 扫描（同时发 `ble.found` 事件） |
| `ble.connect` | `{address}` | `{session}` | 连接并订阅 NUS TX notify；会话 id ≥ 10000 |
| `ble.write` | `{session, data, encoding, append}` | `{sent}` | 写 NUS RX（write-without-response） |
| `ble.close` | `{session}` | `{closed}` | 断开 |
| `uf2.volumes` | `{}` | `[{mount, info}]` | 检测 UF2 卷（找 `INFO_UF2.TXT`） |
| `uf2.validate` | `{path}` | `{family_id?, num_blocks, payload_bytes, addr_min, addr_max}` | 镜像校验 |
| `uf2.wait` | `{timeout_ms?, poll_ms?}` | `{found, volume?}` | 阻塞等待卷出现 |
| `uf2.flash` | `{path, volume, job?}` | `{job, started}` | 后台刷写；进度走 `uf2.progress` / `uf2.done` / `uf2.error` 事件 |
| `dfu.upload` | `{path, baud?, image_path, job?}` | `{job, started}` | MCUboot SMP 串行上传；进度走 `dfu.progress` / `dfu.done` / `dfu.error` |
| `dfu.confirm` | `{path, baud?, hash?}` | `{confirmed}` | 确认镜像（hash 缺省读 image state 取首个） |
| `dfu.reset` | `{path, baud?}` | `{reset}` | 复位设备 |
| `devices.list` | `{}` | `[DeviceInfo]` | 设备注册表 |
| `devices.report` | `{device: DeviceInfo}` | `{reported}` | 上报（upsert，发 `device.found/update`） |
| `devices.remove` | `{id}` | `{removed}` | 移除（发 `device.lost`） |
| `events.emit` | `{topic, payload}` | `true` | 发布到总线（透传前端） |
| `log.write` | `{level: 1-5, msg}` | `true` | 宿主日志 |

### 总线主题约定

- 宿主原生：`device.found|update|lost`、`serial.rx|error|closed`、`serial.plugged|unplugged`、`ble.rx|error|closed`、`ble.found`、`uf2.progress|done|error`、`dfu.progress|done|error`
- 插件自定义：建议 `<plugin.id>/<topic>` 命名空间（如 `zannen.debugger/imu.batch`）；自定义主题不回投插件。

## 4. 清单格式（plugin.toml）

```toml
id = "zannen.debugger"        # 全局唯一（反向域名风格），同时是插件目录名
name = "Zannen 调试器"
version = "0.1.0"             # 与 dylib CARGO_PKG_VERSION 一致（交叉校验）
api = 1                        # 必须等于 ZANNEN_ABI_VERSION
icon = "bug"                   # 侧边栏图标（壳 iconMap 白名单）
capabilities = ["serial", "ble", "uf2"]

[backend]
name = "zannen_debugger"      # crate lib 名（下划线）；动态库文件名按平台推导

[frontend]                     # 可省（纯后端插件）
entry = "frontend/dist/index.js"
css = "frontend/dist/style.css"

[[routes]]                     # 每个路由对应前端 PluginModule.routes 的一个键
path = "devices"
title = "设备"
icon = "cpu"
```

宿主加载时校验：`plugin.toml` 的 `id`/`version` 必须与 dylib 导出清单一致，`api` 与 ABI 版本一致。

## 5. 插件前端契约

- 入口为**单文件 ESM**（vite lib 模式），默认导出 `PluginModule`（见 `packages/plugin-sdk`）；
- external 白名单（壳 vendor 提供，运行时重写）：`react`、`react/jsx-runtime`、`react-dom(-/client)`、`@zannen/plugin-sdk`、`@tauri-apps/api/core`、`@tauri-apps/api/event`、`@tauri-apps/api/window`、`@tauri-apps/plugin-dialog`、`framer-motion`；
- 其余依赖（three、uplot、lucide-react…）自行打包进单文件（享受 tree-shaking）；
- 样式走 `frontend.css`（壳注入一次）+ 壳的全局 `zt-*` 组件类（保持一致观感）；
- 调用后端：`invokePlugin(pluginId, method, args)`；订阅数据：`useBusEvent(topic, cb)`。

## 6. 新插件上手（30 分钟版）

1. `plugins/my-plugin/`：`Cargo.toml`（`crate-type = ["cdylib"]`，依赖 `zannen-plugin-api` path）+ `src/lib.rs` 实现 `ZannenPlugin` + `export_plugin!`；
2. `plugin.toml` 按 §4 填写；
3. 需要 UI 时建 `frontend/`（可拷贝 `plugins/zannen-debugger/frontend` 的 vite 配置），`definePlugin({ routes: {...} })`；
4. 根 `Cargo.toml` workspace members 与 `pnpm-workspace.yaml` 各加一行；
5. `scripts/build-plugin.sh my-plugin` 装配；`pnpm dev` 验证；
6. 参考实现：`plugins/zannen-debugger`（后端含扫描/会话/规则引擎范式，前端含六类视图范式）。

## 7. 故障排查

| 现象 | 排查 |
|---|---|
| 加载报 `ABI 版本不兼容` | 插件与壳的 `zannen-plugin-api` 版本漂移；统一 workspace path 依赖即可 |
| `ManifestMismatch` | plugin.toml 的 id/version 与 crate 的 `CARGO_PKG_VERSION` 不一致 |
| 前端白屏，控制台报 specifier 解析失败 | 插件引用了 external 白名单外的裸导入；改打包进插件或扩充壳 vendor |
| macOS 加载报“无法验证开发者” | 开发期用 `cargo` 本机编译的 dylib 无此问题；分发需对 dylib 做 ad-hoc/开发者签名（`codesign`） |
