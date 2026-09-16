/**
 * 调试模块前端入口：默认导出 PluginModule（壳运行时经 plugin-asset + blob import 装载）。
 * 每个视图外包一层 PluginFrame，右上角浮放调试设备选择下拉框（插件专属）。
 */

import { currentLocale, definePlugin, registerPluginTitles } from "@zannen/plugin-sdk";
import type { ComponentType } from "react";
import { t } from "./i18n";
import { PLUGIN_ID } from "./state";
import { DeviceSelector } from "./components/DeviceSelector";
import { ConsoleView } from "./views/ConsoleView";
import { DevicesView } from "./views/DevicesView";
import { FirmwareView } from "./views/FirmwareView";
import { Motion3DView } from "./views/Motion3DView";
import { StatusView } from "./views/StatusView";
import { TimelineView } from "./views/TimelineView";
import "./styles.css";

/**
 * 模块装载即注册本地化标题提供者（无需任何视图挂载）。
 * 壳在注册时与每次语言切换时调用本回调，主菜单卡片 / 标签栏 / 面包屑
 * 始终与壳语言保持一致。
 */
registerPluginTitles(PLUGIN_ID, () => {
  const locale = currentLocale();
  return {
    name: t(locale, "plugin.name"),
    description: t(locale, "plugin.desc"),
    routes: {
      devices: t(locale, "devices.title"),
      console: t(locale, "console.title"),
      timeline: t(locale, "timeline.title"),
      motion3d: t(locale, "motion.title"),
      firmware: t(locale, "firmware.title"),
      status: t(locale, "status.title"),
    },
  };
});

function frame(View: ComponentType): ComponentType {
  return function PluginFrame() {
    return (
      <div className="zd-plugin-frame">
        <DeviceSelector />
        <View />
      </div>
    );
  };
}

export default definePlugin({
  routes: {
    devices: frame(DevicesView),
    console: frame(ConsoleView),
    timeline: frame(TimelineView),
    motion3d: frame(Motion3DView),
    firmware: frame(FirmwareView),
    status: frame(StatusView),
  },
});
