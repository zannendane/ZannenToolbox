# 调试模块设计（zannen.debugger）

> 目标硬件：ZannenSmol（nRF52840）、ZannenSmolAir（nRF52833）、ZannenDongle / ZannenDongle33（nRF52840 / nRF52833）
> 实现：`plugins/zannen-debugger/`（后端 cdylib + 前端六视图）
> 硬件事实来源：同族固件仓库 SlimeVR-Tracker-nRF / SlimeVR-Tracker-nRF-Receiver / Adafruit_nRF52_Bootloader

## 1. 功能地图

| 视图 | 路由 | 能力 |
|---|---|---|
| 设备 | `devices` | 自动识别（串口双协议探测 + BLE 指纹 + USB VID/PID/Product）、能力展示、一键连接 |
| 串口终端 | `console` | 命令行交互：文本/HEX、行尾选择、收发着色、历史（环形 4000 行） |
| 数据时间轴 | `timeline` | 加速度/陀螺仪/RSSI 流式曲线（uPlot），暂停/清空，60fps |
| 3D 运动 | `motion3d` | 四元数姿态实时渲染（three.js，slerp 平滑），一键标定零点 |
| 固件 | `firmware` | UF2 校验 → 进入 Bootloader → 刷写进度；family 匹配警告 |
| 状态解读 | `status` | status 包 / 控制台 Battery 行 → 规则引擎 → 人类可读卡片 |

## 2. 设备自动识别

证据链任一命中即识别；指纹表 `devices.json`（插件目录内，可热编辑），与真实固件仓库对齐：

1. **串口双协议探测（最可靠）**：打开候选端口 → 同时发送 `{"cmd":"identify"}`（调试协议）与 `info`（真实固件控制台）→ 800ms 内等待 `hello` 行或 `Board: <board_target>` 行 → 精确匹配 `hw_names`（同时收录 board target 名如 `zannensmol_uf2` 与友好名）；
   - 候选端口过滤：mock 端口、VID/PID 命中指纹表、product 命中 `usb_product_patterns`、product 含 "zannen"（避免遍历全部系统端口）；
2. **BLE 广播指纹**：扫描 3s，广播名子串匹配 `ble_name_patterns`（大小写不敏感，**最长模式优先**）。
   注意：**当前应用固件无 BLE**（Tracker↔Dongle 之间是 Nordic ESB 私有 2.4G，对 PC 只有 USB）；BLE 仅存在于 Adafruit bootloader 的 OTA 模式（广播名 `AdaDFU`），不用于识别。BLE 指纹主要服务 mock 演示与未来固件；
3. **USB VID/PID + product 字符串**：VID `0x1209`（pid.codes 开源 VID）。应用态 PID 在型号间复用（Smol/SmolAir 共用 `0x7692`，Dongle/Dongle33 共用 `0x7690`），区分依据是 USB product 字符串（`usb_product_patterns`，**最长匹配优先**）：`SlimeVR ZannenSmol` / `SlimeVR ZannenSmolAir` / `SlimeNRF Receiver ZannenDongle[33]`；bootloader 态 product 为 `ZannenSmolAir` / `ZannenDongle_nRF52840` / `ZannenDongle_nRF52833`。

识别结果归并为 `DeviceInfo` 上报宿主注册表（`devices.report`），壳顶栏即时可见。未命中指纹时按"未知设备"展示但保留会话能力。

## 3. 数据协议（调试通道）

串口/BLE 数据面统一为 **UTF-8 JSON Lines**（每行一个对象，含 `type`）。该协议是调试基线，便于 mock 与联调；后续二进制帧化在插件 `protocol.rs` 单点替换。

```
→ {"cmd":"identify"}                ← {"type":"hello","hw":"zannen-smol","fw":"1.4.2","proto":1}
→ {"cmd":"ping"}                    ← {"type":"pong","ts":123}
→ {"cmd":"calibrate"}               ← {"type":"ack","cmd":"calibrate","msg":"..."}
→ {"cmd":"dfu"} 或 dfu              ← {"type":"dfu","msg":"entering UF2 bootloader"}（随后设备复位）
← {"type":"imu","ts":ms,"quat":[w,x,y,z],"accel":[g],"gyro":[°/s]}      100Hz
← {"type":"rf","ts":ms,"rssi":dBm,"ch":37}                             10Hz
← {"type":"status","battery":87.0,"charging":false,"fw":"1.4.2",
   "imu":"ok","rf_link":"good","uptime_s":42}                          1Hz
```

非 JSON 行（固件日志打印）原样透传给终端视图；其中两类文本行被结构化解析：

- `Board: <board_target>`（`info` 应答）→ 识别证据（见 §2）；
- `Battery: 87.42% (Raw ...)`（`battery` 命令/后台日志）→ 合成 status 包送规则引擎。

### 真实固件控制台（SlimeVR-Tracker-nRF 系）

真实固件在 CDC ACM 上提供大小写不敏感的文本控制台（Tracker：`help/info/uptime/reboot/battery/scan/calibrate/dfu/...`；Receiver：`help/info/uptime/list/reboot/add/remove/pair/dfu/...`）。本模块兼容要点：`info` → `Board:`/`SOC:`/`Target:` 行用于识别；`dfu` 复位进 UF2 bootloader（GPREGRET=0x57）；`battery` 文本行用于状态解读。
**已知差距（roadmap）**：真实固件的连续 IMU/RF 数据走 USB HID 16 字节二进制报告（SlimeVR 有线协议，含四元数 fixed15/加速度 fixed7/RSSI），不走串口文本流——HID 数据面服务待实现，当前 3D/时间轴视图依赖 JSON 调试协议（mock 演示）或后续固件调试通道。

### 数据流水线（性能路径）

```
串口读线程 ──serial.rx(hex)──→ 插件 on_event ──行切割/解析──┬─→ imu_buf / rf_buf
                                                            └─→ status → 规则引擎 → status 事件
flusher 线程（33ms 周期）──批量 drain──→ events.emit ──→ 总线 ──→ 前端 rAF → uPlot/three
```

- 批量周期 33ms（≈30Hz）：100Hz IMU 每批 ~3-4 样本，IPC 次数降到 1/30；
- 缓冲上限 4000 样本，前端掉队时丢最旧一半（保实时性）；
- 前端：数据缓冲在 React 之外的环形数组，uPlot `setData` 由 rAF 驱动；终端 100ms 合帧。

## 4. UF2 固件刷写

### 流程（nRF52，Adafruit UF2 Bootloader）

```
连接会话 ──dfu（文本）──→ 设备复位进 Bootloader ──MSC 卷挂载（INFO_UF2.TXT）
     ↓                                            ↓
关闭会话（释放串口）                    uf2.wait 轮询检测（默认 10s/400ms 间隔）
                                                    ↓
uf2.validate（魔数/块序/familyID 校验 + 与设备指纹 family 匹配警告）
                                                    ↓
uf2.flash：512B 逐块写入 + 每 8KB 发 uf2.progress ──→ 完成自动重启
```

familyID 约定（与 bootloader 仓库一致）：nRF52840 = `0xADA52840`（Smol / Dongle），nRF52833 = `0x621E937A`（SmolAir / Dongle33），写入 `devices.json` 的 `uf2_family`。bootloader 卷标：`ZNDONGLE` / `ZNSMOLAIR` / `ZNDONGLE33`。

### MCUboot/SMP 路径（宿主服务已备，当前无 Zannen 硬件使用）

`zannen-core` 内置 `dfu` 服务（`services/dfu.rs`）：CBOR → SMP 头 → base64 → 分片 CRC16 → SLIP 全协议栈，命令含 image state 查询 / 分块上传 / 确认 / 复位；mock-transport 内置内存 MCUboot 模拟器可无硬件演示。**注意**：现有 Zannen 硬件（Smol/SmolAir/Dongle/Dongle33 均为 nRF52 + Adafruit UF2 bootloader）不使用该路径，保留给未来 nRF54 硬件；固件视图按设备 `capabilities` 自动分流（含 `mcuboot` → SMP 卡片流；含 `uf2` → UF2 三步流）。

### 失败保护

- 写前整镜像校验（魔数/块序/载荷越界/块数一致性）；
- 写入后回读大小比对（bootloader 秒卸载卷时跳过回读，不算失败）；
- 进度事件带 `job` id，前端按 job 过滤，支持并发/重复触发不串台；
- 目标卷判定依赖 `INFO_UF2.TXT`，杜绝误写普通 U 盘。

## 5. 状态解读规则引擎

`status_rules.json` 声明式卡片规则；`when` 支持 `<=/>=/</>/==`（数值/字符串/布尔）与 `default` 兜底，按序求值首个命中生效，`level ∈ ok/warn/critical` 直接映射 UI 徽标与卡片描边色。

```jsonc
{
  "id": "battery", "title": "电池电量", "icon": "battery-medium",
  "source": "battery", "unit": "%",
  "levels": [
    { "when": "<= 10", "level": "critical", "note": "电量极低，请立即充电" },
    { "when": "<= 25", "level": "warn", "note": "电量偏低" },
    { "when": "default", "level": "ok", "note": "电量正常" }
  ]
}
```

新增解读项 = 改 JSON，无需改代码；规则表经 `status.rules` invoke 暴露给前端（图例/文档生成）。

## 6. 模拟设备（mock-transport）

无硬件演示与集成测试：`serial.list` 注入 `mock://zannen-smol`、`mock://zannen-smol-air`、`mock://zannen-dongle` 三个虚拟口；行为与真实协议一致（identify/ping/calibrate/dfu 命令，100Hz IMU 缓动旋转 + 噪声，10Hz RSSI 波动，1Hz 电量缓降 status）。`dfu` 命令使虚拟设备消失（模拟复位进 bootloader）。发布版可通过关闭 feature 移除。

## 7. 已知边界与后续

- BLE 数据面（NUS 连接/写入/Notify）已落地，BLE 会话与串口会话统一；
- 串口热插拔已实现：1s 轮询 diff，发 `serial.plugged/unplugged` 事件并自动清理会话；
- 多设备并发数据流：前端图表/3D 视图已按设备分轨选择（时间轴按 device 分桶缓冲）。
