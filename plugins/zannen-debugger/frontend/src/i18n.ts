/**
 * 调试模块 i18n：zh-CN 为基准字典，en-US 完整对照。
 * 视图用法：`const locale = useLocale(); t(locale, "console.title")`；
 * 含占位符的文案用 `tf(locale, "key", { n: 1 })`，占位符形如 `{n}`。
 */

import { createI18n, type Locale } from "@zannen/plugin-sdk";

const zhCN = {
  // ---------- 插件元信息（壳展示层本地化桥） ----------
  "plugin.name": "Zannen SlimeVR调试",
  "plugin.desc": "ZannenSmol / SmolAir / Dongle 调试模块：设备识别、UF2 刷写、3D 运动、数据时间轴、串口终端、状态解读",

  // ---------- 通用 ----------
  "common.connect": "连接",
  "common.disconnect": "断开",
  "common.clear": "清空",
  "common.send": "发送",
  "common.pause": "暂停",
  "common.resume": "继续",
  "common.notConnected": "尚未连接设备",
  "common.active": "活跃",
  "common.setActive": "设为活跃",
  "common.connectedSession": "已连接 · 会话 {n}",

  // ---------- 设备总览 ----------
  "devices.title": "设备",
  "devices.desc": "自动识别 ZannenSmol / SmolAir / Dongle（串口 identify 探测 + BLE 广播指纹）",
  "devices.scan": "扫描设备",
  "devices.sessions": "已连接会话",
  "devices.empty": "未发现设备",
  "devices.emptyDesc":
    "连接 Zannen 硬件后点击「扫描设备」。开发模式下内置 mock:// 虚拟设备（mock-transport feature），可直接体验完整数据流。",
  "devices.unidentified": "未识别",
  "devices.bleNotice.title": "需要蓝牙权限",
  "devices.bleNotice.desc":
    "Zannen SlimeVR调试使用蓝牙（BLE）发现并连接 Zannen 设备，传输命令与传感器数据。点击「继续并授权」后，系统将弹出蓝牙授权框。",
  "devices.bleNotice.allow": "继续并授权",
  "devices.bleNotice.later": "暂不，仅串口扫描",
  "devices.bleDenied.title": "蓝牙权限未授予",
  "devices.bleDenied.desc":
    "蓝牙（BLE）用于发现并连接 Zannen 设备、传输命令与传感器数据。权限未授予期间，BLE 相关功能已暂停，串口功能不受影响。",
  "devices.bleDenied.reasons":
    "可能原因：在系统授权弹窗中选择了「不允许」；或此前在系统设置中关闭了本应用的蓝牙权限；或系统蓝牙未开启。",
  "devices.bleDenied.retry": "重试权限检测",
  "devices.bleDenied.openSettings": "打开系统设置",
  "devices.selNone": "未选择调试设备",
  "devices.selEmpty": "尚未连接设备 —— 在「设备」页扫描并连接",
  "devices.serial": "串口",
  "devices.mock": "模拟",
  "devices.noPath": "该设备既无串口路径也无 BLE 地址，无法连接",

  // ---------- 串口终端 ----------
  "console.title": "串口终端",
  "console.descIdle": "与设备的命令行交互",
  "console.descConnected": '{label} · 会话 {n} · 命令为 JSON 行，如 {"cmd":"ping"}',
  "console.emptyDesc": '请先在「设备」页建立连接。提示：命令是 JSON 行，例如 {"cmd":"ping"}',
  "console.encoding.text": "文本",
  "console.append.none": "无行尾",
  "console.timestamps": "时间戳",
  "console.export": "导出日志",
  "console.exportFilter": "日志文件",
  "console.exported": "已导出 {n} 行至 {path}",
  "console.exportFailed": "导出失败：{error}",
  "console.placeholderHex": "输入 hex，如 7b22636d64223a2270696e67227d",
  "console.sysClosed": "[会话已关闭]",
  "console.sysError": "[串口错误] {error}",
  "console.sendFailed": "[发送失败] {error}",

  // ---------- 数据时间轴 ----------
  "timeline.title": "数据时间轴",
  "timeline.desc": "传感器与 RF 原始数据的时间轴展示",
  "timeline.descConnected": "{label} · 已接收 {n} 个样本 · 缓冲 {cap} 点",
  "timeline.emptyDesc": "请先在「设备」页建立连接。",
  "timeline.clear": "清空缓冲",
  "timeline.source": "数据源",
  "timeline.legendHint": "点击图例项可隐藏/显示通道",
  "timeline.pausedHint": "已暂停：移动游标可在图例读取数值；点击图例项可隐藏/显示通道",
  "timeline.exportCsv": "导出 CSV",
  "timeline.exportHint":
    "导出当前选中设备的缓冲为两个文件：<名>.imu.csv（ts_ms,ax,ay,az,gx,gy,gz）与 <名>.rssi.csv（ts_ms,rssi）",
  "timeline.exported": "已导出 {imu} 与 {rf}",
  "timeline.exportFailed": "导出失败：{error}",
  "timeline.chart.accel": "加速度",
  "timeline.chart.gyro": "陀螺仪",
  "timeline.chart.rssi": "RF 信号强度",

  // ---------- 3D 运动 ----------
  "motion.title": "3D 运动",
  "motion.descIdle": "跟踪器姿态实时渲染（无数据流时展示待机自转）",
  "motion.descConnected": "{label} · 四元数姿态流",
  "motion.calibrate": "标定当前姿态为零点",
  "motion.live": "● 数据流中",
  "motion.waiting": "○ 等待数据",
  "motion.overlayTitle": "未连接设备",
  "motion.overlayDesc": "连接设备后此处渲染其实时姿态",

  // ---------- 固件 ----------
  "firmware.title": "固件",
  "firmware.desc": "UF2 固件刷写与升级（nRF52 UF2 Bootloader；nRF54 MCUboot 适配见文档）",
  "firmware.step1": "选择固件",
  "firmware.step3": "刷写",
  "firmware.card1": "① 固件文件",
  "firmware.card2": "② Bootloader 卷",
  "firmware.card3": "③ 刷写",
  "firmware.pick": "选择 .uf2 文件",
  "firmware.noFile": "未选择",
  "firmware.filterName": "UF2 固件",
  "firmware.familyMatch": "设备匹配",
  "firmware.match": "匹配",
  "firmware.mismatch": "不匹配（期望 {expect}）",
  "firmware.noFamily": "（未携带）",
  "firmware.blocks": "块数",
  "firmware.payload": "有效载荷",
  "firmware.addrRange": "地址范围",
  "firmware.enterBootloader": "进入 Bootloader",
  "firmware.refreshVolumes": "刷新卷列表",
  "firmware.needConnect": "需先在「设备」页连接",
  "firmware.noVolumes": "未检测到 UF2 卷（找 INFO_UF2.TXT）。",
  "firmware.finishFirst": "先完成前两步",
  "firmware.flash": "开始刷写",
  "firmware.done": "刷写完成（{n} 块），设备将自动重启。",
  "firmware.failed": "刷写失败：{error}",
  "firmware.validateFailed": "校验失败：{error}",
  "firmware.entered": "已进入 Bootloader：{mount}",
  "firmware.notFound":
    "未检测到 UF2 卷。设备可能仍在复位，或固件尚未支持 dfu 命令；可手动双击复位键后点「刷新卷列表」。",
  "firmware.enterFailed": "进入 Bootloader 失败：{error}",
  "firmware.flashStartFailed": "无法启动刷写：{error}",

  // ---------- MCUboot 串行 DFU ----------
  "firmware.mcuboot.title": "MCUboot 串行刷写（nRF54）",
  "firmware.mcuboot.pickBin": "选择 .bin 镜像",
  "firmware.mcuboot.noFile": "未选择",
  "firmware.mcuboot.disconnectNote": "上传前将自动断开调试会话以独占串口",
  "firmware.mcuboot.upload": "上传镜像",
  "firmware.mcuboot.confirm": "确认镜像",
  "firmware.mcuboot.reset": "复位运行",
  "firmware.mcuboot.uploaded": "上传完成（{n} B）。请确认并复位。",
  "firmware.mcuboot.confirmed": "镜像已确认（{hash}），复位后生效。",
  "firmware.mcuboot.resetDone": "复位指令已发送，设备重启中。",
  "firmware.mcuboot.failed": "MCUboot 操作失败：{error}",
  "firmware.mcuboot.needSerial": "MCUboot 刷写需要设备的串口路径（BLE 会话请改用带串口的连接）。",

  // ---------- 状态解读 ----------
  "status.title": "状态解读",
  "status.desc": "设备状态包的自动解读：规则引擎将原始字段映射为人类可读卡片",
  "status.empty": "暂无状态数据",
  "status.emptyNeedConnect": "请先在「设备」页建立连接；设备连接后按 1Hz 上报 status 包。",
  "status.emptyWaiting": "等待设备上报 status 包…",
  "status.level.ok": "正常",
  "status.level.warn": "注意",
  "status.level.critical": "异常",
  "status.raw": "原始数据包 · {device}",
  "status.yes": "是",
  "status.no": "否",
};

const enUS: Record<keyof typeof zhCN, string> = {
  // ---------- plugin meta (shell display bridge) ----------
  "plugin.name": "Zannen SlimeVR Debug",
  "plugin.desc": "Debug module for ZannenSmol / SmolAir / Dongle: identification, UF2 flashing, 3D motion, timeline, serial console, status",

  // ---------- common ----------
  "common.connect": "Connect",
  "common.disconnect": "Disconnect",
  "common.clear": "Clear",
  "common.send": "Send",
  "common.pause": "Pause",
  "common.resume": "Resume",
  "common.notConnected": "No device connected",
  "common.active": "Active",
  "common.setActive": "Set active",
  "common.connectedSession": "Connected · session {n}",

  // ---------- devices ----------
  "devices.title": "Devices",
  "devices.desc": "Auto-detects ZannenSmol / SmolAir / Dongle (serial identify probe + BLE fingerprint)",
  "devices.scan": "Scan devices",
  "devices.sessions": "Connected sessions",
  "devices.empty": "No devices found",
  "devices.emptyDesc":
    'Connect Zannen hardware and click "Scan devices". Dev builds include mock:// virtual devices (mock-transport feature) for a full data-flow demo.',
  "devices.unidentified": "Unidentified",
  "devices.bleNotice.title": "Bluetooth permission needed",
  "devices.bleNotice.desc":
    "Zannen SlimeVR Debug uses Bluetooth (BLE) to discover and connect Zannen devices, carrying commands and sensor data. The system permission prompt will appear after you continue.",
  "devices.bleNotice.allow": "Continue & authorize",
  "devices.bleNotice.later": "Not now, serial only",
  "devices.bleDenied.title": "Bluetooth permission not granted",
  "devices.bleDenied.desc":
    "Bluetooth (BLE) is used to discover and connect Zannen devices, carrying commands and sensor data. BLE features are paused until permission is restored; serial features are unaffected.",
  "devices.bleDenied.reasons":
    "Likely reasons: you chose \"Don't Allow\" in the system prompt; or this app's Bluetooth permission was previously turned off in system settings; or Bluetooth is disabled on this machine.",
  "devices.bleDenied.retry": "Retry permission check",
  "devices.bleDenied.openSettings": "Open system settings",
  "devices.selNone": "No debug device",
  "devices.selEmpty": "No device connected — scan and connect on the Devices page",
  "devices.serial": "Serial",
  "devices.mock": "Mock",
  "devices.noPath": "This device has neither a serial path nor a BLE address; cannot connect",

  // ---------- console ----------
  "console.title": "Serial console",
  "console.descIdle": "Command-line interaction with the device",
  "console.descConnected": '{label} · session {n} · commands are JSON lines, e.g. {"cmd":"ping"}',
  "console.emptyDesc":
    'Connect a device on the Devices page first. Tip: commands are JSON lines, e.g. {"cmd":"ping"}',
  "console.encoding.text": "Text",
  "console.append.none": "No EOL",
  "console.timestamps": "Timestamps",
  "console.export": "Export log",
  "console.exportFilter": "Log files",
  "console.exported": "Exported {n} lines to {path}",
  "console.exportFailed": "Export failed: {error}",
  "console.placeholderHex": "Enter hex, e.g. 7b22636d64223a2270696e67227d",
  "console.sysClosed": "[session closed]",
  "console.sysError": "[serial error] {error}",
  "console.sendFailed": "[send failed] {error}",

  // ---------- timeline ----------
  "timeline.title": "Data timeline",
  "timeline.desc": "Streaming timeline of sensor and RF raw data",
  "timeline.descConnected": "{label} · {n} samples received · buffer {cap} pts",
  "timeline.emptyDesc": "Connect a device on the Devices page first.",
  "timeline.clear": "Clear buffer",
  "timeline.source": "Source",
  "timeline.legendHint": "Click a legend entry to hide/show its channel",
  "timeline.pausedHint":
    "Paused: move the cursor to read values in the legend; click a legend entry to hide/show a channel",
  "timeline.exportCsv": "Export CSV",
  "timeline.exportHint":
    "Exports the selected device's buffer as two files: <name>.imu.csv (ts_ms,ax,ay,az,gx,gy,gz) and <name>.rssi.csv (ts_ms,rssi)",
  "timeline.exported": "Exported {imu} and {rf}",
  "timeline.exportFailed": "Export failed: {error}",
  "timeline.chart.accel": "Accelerometer",
  "timeline.chart.gyro": "Gyroscope",
  "timeline.chart.rssi": "RF signal strength",

  // ---------- motion3d ----------
  "motion.title": "3D motion",
  "motion.descIdle": "Real-time tracker orientation (idle spin while no data)",
  "motion.descConnected": "{label} · quaternion stream",
  "motion.calibrate": "Calibrate current pose as zero",
  "motion.live": "● Streaming",
  "motion.waiting": "○ Waiting for data",
  "motion.overlayTitle": "Not connected",
  "motion.overlayDesc": "Connect a device to render its live orientation here",

  // ---------- firmware ----------
  "firmware.title": "Firmware",
  "firmware.desc": "UF2 flashing and upgrade (nRF52 UF2 bootloader; see docs for nRF54 MCUboot)",
  "firmware.step1": "Pick firmware",
  "firmware.step3": "Flash",
  "firmware.card1": "① Firmware file",
  "firmware.card2": "② Bootloader volume",
  "firmware.card3": "③ Flash",
  "firmware.pick": "Choose .uf2 file",
  "firmware.noFile": "No file chosen",
  "firmware.filterName": "UF2 firmware",
  "firmware.familyMatch": "Device match",
  "firmware.match": "Match",
  "firmware.mismatch": "Mismatch (expected {expect})",
  "firmware.noFamily": "(not present)",
  "firmware.blocks": "Blocks",
  "firmware.payload": "Payload",
  "firmware.addrRange": "Address range",
  "firmware.enterBootloader": "Enter bootloader",
  "firmware.refreshVolumes": "Refresh volumes",
  "firmware.needConnect": "Connect a device on the Devices page first",
  "firmware.noVolumes": "No UF2 volume detected (looking for INFO_UF2.TXT).",
  "firmware.finishFirst": "Complete the first two steps",
  "firmware.flash": "Start flashing",
  "firmware.done": "Flashing finished ({n} blocks); the device will reboot.",
  "firmware.failed": "Flashing failed: {error}",
  "firmware.validateFailed": "Validation failed: {error}",
  "firmware.entered": "Entered bootloader: {mount}",
  "firmware.notFound":
    'No UF2 volume detected. The device may still be resetting, or its firmware lacks the dfu command; double-tap reset manually, then click "Refresh volumes".',
  "firmware.enterFailed": "Failed to enter bootloader: {error}",
  "firmware.flashStartFailed": "Cannot start flashing: {error}",

  // ---------- MCUboot serial DFU ----------
  "firmware.mcuboot.title": "MCUboot serial DFU (nRF54)",
  "firmware.mcuboot.pickBin": "Pick .bin image",
  "firmware.mcuboot.noFile": "None",
  "firmware.mcuboot.disconnectNote": "The debug session will be closed to claim the serial port",
  "firmware.mcuboot.upload": "Upload image",
  "firmware.mcuboot.confirm": "Confirm image",
  "firmware.mcuboot.reset": "Reset & run",
  "firmware.mcuboot.uploaded": "Upload complete ({n} B). Confirm and reset to boot.",
  "firmware.mcuboot.confirmed": "Image confirmed ({hash}); takes effect after reset.",
  "firmware.mcuboot.resetDone": "Reset sent; device is rebooting.",
  "firmware.mcuboot.failed": "MCUboot operation failed: {error}",
  "firmware.mcuboot.needSerial": "MCUboot DFU needs the device's serial path (reconnect via serial instead of BLE).",

  // ---------- status ----------
  "status.title": "Status insight",
  "status.desc": "Automatic interpretation of device status packets: a rule engine maps raw fields to human-readable cards",
  "status.empty": "No status data yet",
  "status.emptyNeedConnect":
    "Connect a device on the Devices page first; devices report status packets at 1 Hz once connected.",
  "status.emptyWaiting": "Waiting for status packets…",
  "status.level.ok": "OK",
  "status.level.warn": "Warning",
  "status.level.critical": "Critical",
  "status.raw": "Raw packet · {device}",
  "status.yes": "Yes",
  "status.no": "No",
};

export const t = createI18n({ "zh-CN": zhCN, "en-US": enUS });

export type I18nKey = keyof typeof zhCN;

/** 带占位符替换的翻译：`{name}` ← params.name。 */
export function tf(locale: Locale, key: I18nKey, params: Record<string, string | number>): string {
  let s = t(locale, key);
  for (const [k, v] of Object.entries(params)) {
    s = s.replaceAll(`{${k}}`, String(v));
  }
  return s;
}
