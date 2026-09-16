/**
 * 实时翻译前端入口：默认导出 PluginModule（壳运行时经 plugin-asset + blob import 装载）。
 * 除主窗口路由视图外，经 `overlay` 字段导出悬浮窗组件（壳 overlay_open 窗口渲染）。
 */

import { currentLocale, definePlugin, registerPluginTitles } from "@zannen/plugin-sdk";
import { PLUGIN_ID } from "./config";
import { t } from "./i18n";
import { TranslatorOverlay } from "./overlay/TranslatorOverlay";
import { TranslatorView } from "./views/TranslatorView";
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
      translator: t(locale, "translator.title"),
    },
  };
});

export default definePlugin({
  routes: {
    translator: TranslatorView,
  },
  overlay: TranslatorOverlay,
});
