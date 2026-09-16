/**
 * 顶栏：仅当前位置面包屑。
 * 设备选择是调试插件（zannen.debugger / SlimeVR 调试）的专属功能，由插件视图自身承载，
 * 壳不提供全局设备下拉框。外观/语言/设置入口在左下角悬浮工具栈（FloatingBar）。
 */

import { useT } from "./i18n";
import { useShellStore } from "./store";

export function TopBar() {
  const plugins = useShellStore((s) => s.plugins);
  const selection = useShellStore((s) => s.selection);
  const routeTitle = useShellStore((s) => s.routeTitle);
  const pluginName = useShellStore((s) => s.pluginName);
  const t = useT();

  const plugin = plugins.find((p) => p.manifest.id === selection?.pluginId);
  const route = plugin?.manifest.routes.find((r) => r.path === selection?.route);
  const crumb =
    selection?.pluginId === "shell"
      ? t("crumbSettings")
      : plugin
        ? `${pluginName(plugin.manifest.id, plugin.manifest.name)} · ${route ? routeTitle(plugin.manifest.id, route.path, route.title) : (selection?.route ?? "")}`
        : t("crumbHome");

  return (
    <header className="topbar">
      <div className="topbar-crumb">{crumb}</div>
      {/* 插件专属控件插槽（如 SlimeVR 调试的设备选择器，经 createPortal 填充，
          与插件名面包屑同一行水平） */}
      {selection?.pluginId && selection.pluginId !== "shell" && (
        <div className="topbar-slot" data-plugin-slot={selection.pluginId} />
      )}
    </header>
  );
}
