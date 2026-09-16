/**
 * 模块选择主页（类 nRF Connect 启动器）：插件卡片网格。
 * 无插件时给出排查指引。卡片点击进入插件首个路由；卡片底部的路由
 * 胶囊可直达子视图。
 */

import { Badge, EmptyState, GlassCard, SectionTitle } from "@zannen/plugin-sdk";
import { motion } from "framer-motion";
import { Boxes, TriangleAlert } from "lucide-react";
import { iconFor } from "./iconMap";
import { useT } from "./i18n";
import { useShellStore } from "./store";

export function LauncherView() {
  const plugins = useShellStore((s) => s.plugins);
  const select = useShellStore((s) => s.select);
  const pluginName = useShellStore((s) => s.pluginName);
  const pluginDesc = useShellStore((s) => s.pluginDesc);
  const t = useT();

  return (
    <div className="launcher">
      <SectionTitle title={t("launcherTitle")} desc={t("launcherDesc")} />
      {plugins.length === 0 ? (
        <GlassCard>
          <EmptyState icon={<Boxes size={30} />} title={t("launcherEmpty")} desc={t("launcherEmptyDesc")} />
        </GlassCard>
      ) : (
        <div className="launcher-grid">
          {plugins.map((p, i) => {
            const Icon = iconFor(p.manifest.icon ?? undefined);
            return (
              <motion.div
                key={p.manifest.id}
                initial={{ opacity: 0, y: 14 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: i * 0.06, duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
                whileHover={{ scale: 1.02, y: -2 }}
                whileTap={{ scale: 0.98 }}
              >
                <GlassCard
                  className={`launcher-card${p.error ? " is-broken" : ""}`}
                  role="button"
                  tabIndex={0}
                  onClick={() =>
                    !p.error &&
                    p.manifest.routes[0] &&
                    select({ pluginId: p.manifest.id, route: p.manifest.routes[0].path })
                  }
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !p.error && p.manifest.routes[0]) {
                      select({ pluginId: p.manifest.id, route: p.manifest.routes[0].path });
                    }
                  }}
                >
                  <div className="launcher-card-head">
                    <span className="launcher-icon">
                      <Icon size={22} />
                    </span>
                    <div className="launcher-title">
                      <div className="launcher-name">{pluginName(p.manifest.id, p.manifest.name)}</div>
                      <div className="launcher-id">{p.manifest.id}</div>
                    </div>
                    <Badge tone="muted">v{p.manifest.version}</Badge>
                  </div>
                  {p.manifest.description && (
                    <p className="launcher-desc">{pluginDesc(p.manifest.id, p.manifest.description)}</p>
                  )}
                  {p.error ? (
                    <div className="launcher-error">
                      <TriangleAlert size={12} /> {p.error}
                    </div>
                  ) : (
                    <div className="launcher-routes">
                      {p.manifest.routes.map((r) => (
                        <button
                          key={r.path}
                          className="launcher-route-chip"
                          onClick={(e) => {
                            e.stopPropagation();
                            select({ pluginId: p.manifest.id, route: r.path });
                          }}
                        >
                          {r.title}
                        </button>
                      ))}
                    </div>
                  )}
                </GlassCard>
              </motion.div>
            );
          })}
        </div>
      )}
    </div>
  );
}
