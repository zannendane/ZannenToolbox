/**
 * 悬浮窗宿主：`?overlay=<pluginId>` 窗口中替代正常壳界面，
 * 复用插件装载链（plugin_list → ensurePluginLoaded → module.overlay）
 * 渲染插件导出的 overlay 组件。
 *
 * 窗口 chrome（工具栏/拖动/关闭）由插件 overlay 组件自绘；
 * 此处仅负责装载与错误占位。
 */

import { useEffect } from "react";
import { EmptyState, Spinner } from "@zannen/plugin-sdk";
import { TriangleAlert } from "lucide-react";
import { useT } from "./i18n";
import { ensurePluginLoaded } from "./pluginLoader";
import { useShellStore } from "./store";

export function OverlayHost({ pluginId }: { pluginId: string }) {
  const plugins = useShellStore((s) => s.plugins);
  const pluginsReady = useShellStore((s) => s.pluginsReady);
  const t = useT();

  const plugin = plugins.find((p) => p.manifest.id === pluginId);
  const needsLoad = !!plugin && !plugin.module && !plugin.error && !!plugin.manifest.frontend;

  useEffect(() => {
    if (pluginsReady && needsLoad) {
      void ensurePluginLoaded(pluginId);
    }
  }, [pluginsReady, needsLoad, pluginId]);

  if (!pluginsReady || !plugin) {
    return (
      <div className="overlay-host-loading">
        {pluginsReady ? (
          <EmptyState
            icon={<TriangleAlert size={24} />}
            title={t("pluginNotFound")}
            desc={pluginId}
          />
        ) : (
          <Spinner size={20} />
        )}
      </div>
    );
  }
  if (plugin.error) {
    return (
      <div className="overlay-host-loading">
        <EmptyState
          icon={<TriangleAlert size={24} />}
          title={t("pluginLoadFailed", { name: plugin.manifest.name })}
          desc={plugin.error}
        />
      </div>
    );
  }
  const Overlay = plugin.module?.overlay;
  if (!Overlay) {
    return (
      <div className="overlay-host-loading">
        {plugin.module ? (
          <EmptyState
            icon={<TriangleAlert size={24} />}
            title={t("overlayNotAvailable")}
            desc={pluginId}
          />
        ) : (
          <Spinner size={20} />
        )}
      </div>
    );
  }
  return <Overlay />;
}
