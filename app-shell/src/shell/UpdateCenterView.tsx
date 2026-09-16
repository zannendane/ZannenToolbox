/**
 * 更新中心（壳内建视图）：本体更新 + 插件更新 + 更新源状态。
 *
 * 状态机：idle → checking → upToDate | available → downloading → ready → (restart)
 * 一切网络/配置问题降级为温和提示，不打断使用。
 */

import { invoke } from "@tauri-apps/api/core";
import type { Update } from "@tauri-apps/plugin-updater";
import {
  Badge,
  Button,
  GlassCard,
  Progress,
  SectionTitle,
  Spinner,
} from "@zannen/plugin-sdk";
import { motion } from "framer-motion";
import { CircleArrowUp, Download, PackageCheck, RefreshCw, RotateCcw, Server } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useT } from "./i18n";
import { SHELL_ERR } from "./errors";
import { useShellStore } from "./store";
import {
  checkAppUpdate,
  checkPluginUpdate,
  installPluginUpdate,
  loadUpdateSources,
  type PluginUpdateInfo,
  type UpdateSources,
} from "./updater";

type AppUpdateState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "upToDate" }
  | { phase: "available"; update: Update }
  | { phase: "downloading"; percent: number | null }
  | { phase: "ready" }
  | { phase: "error"; message: string };

type PluginRowState =
  | { phase: "unchecked" }
  | { phase: "checking" }
  | { phase: "upToDate" }
  | { phase: "available"; info: PluginUpdateInfo }
  | { phase: "installing" }
  | { phase: "done"; version: string }
  | { phase: "error"; message: string };

export function UpdateCenterView() {
  const plugins = useShellStore((s) => s.plugins);
  const setAppUpdateAvailable = useShellStore((s) => s.setAppUpdateAvailable);
  const [sources, setSources] = useState<UpdateSources | null>(null);
  const [appVersion, setAppVersion] = useState("...");
  const [target, setTarget] = useState("");
  const [appState, setAppState] = useState<AppUpdateState>({ phase: "idle" });
  const [rows, setRows] = useState<Record<string, PluginRowState>>({});
  const t = useT();

  const setRow = (id: string, state: PluginRowState) =>
    setRows((prev) => ({ ...prev, [id]: state }));

  useEffect(() => {
    void (async () => {
      const [src, info] = await Promise.all([
        loadUpdateSources(),
        invoke<{ version: string; target: string }>("shell_info"),
      ]);
      setSources(src);
      setAppVersion(info.version);
      setTarget(info.target);
    })();
  }, []);

  const runAppCheck = useCallback(async () => {
    setAppState({ phase: "checking" });
    const update = await checkAppUpdate(sources);
    if (!update) {
      setAppState({ phase: sources?.app.enabled ? "upToDate" : "idle" });
      return;
    }
    setAppState({ phase: "available", update });
    setAppUpdateAvailable(update.version);
  }, [sources, setAppUpdateAvailable]);

  const runAppInstall = async () => {
    if (appState.phase !== "available") return;
    const { update } = appState;
    setAppState({ phase: "downloading", percent: null });
    try {
      let total = 0;
      let downloaded = 0;
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") {
          total = ev.data.contentLength ?? 0;
          setAppState({ phase: "downloading", percent: total === 0 ? null : 0 });
        } else if (ev.event === "Progress") {
          downloaded += ev.data.chunkLength;
          setAppState({
            phase: "downloading",
            percent: total > 0 ? Math.min(99, Math.round((downloaded / total) * 100)) : null,
          });
        }
      });
      setAppState({ phase: "ready" });
    } catch (e) {
      setAppState({ phase: "error", message: `[${SHELL_ERR.APP_UPDATE_FAILED}] ${String(e)}` });
    }
  };

  const runPluginCheck = useCallback(
    async (id: string, version: string) => {
      setRow(id, { phase: "checking" });
      const info = await checkPluginUpdate(id, version, target, sources);
      setRow(id, info ? { phase: "available", info } : { phase: "upToDate" });
    },
    [sources, target],
  );

  const runPluginInstall = async (id: string) => {
    const row = rows[id];
    if (!row || row.phase !== "available") return;
    setRow(id, { phase: "installing" });
    try {
      const { report } = await installPluginUpdate(row.info);
      setRow(id, { phase: "done", version: report.version });
      // 热重载后刷新插件前端模块
      const store = useShellStore.getState();
      const plugin = store.plugins.find((p) => p.manifest.id === id);
      if (plugin) {
        const { loadPluginFrontend } = await import("./pluginLoader");
        const reloaded = await loadPluginFrontend({
          ...plugin.manifest,
          version: report.version,
        });
        store.replacePlugin(reloaded);
      }
    } catch (e) {
      setRow(id, { phase: "error", message: `[${SHELL_ERR.PLUGIN_UPDATE_FAILED}] ${String(e)}` });
    }
  };

  const sourcesEnabled = sources && (sources.app.enabled || Object.values(sources.plugins).some((p) => p.enabled));

  return (
    <div className="zd-view">
      <SectionTitle title={t("updateCenterTitle")} desc={t("updateCenterDesc")} />

      {!sourcesEnabled && (
        <div className="zd-message zd-message-muted">
          <Server size={13} style={{ marginRight: 6, verticalAlign: -2 }} />
          {t("updateSourcesMissing")}
        </div>
      )}

      {/* 本体 */}
      <GlassCard>
        <div className="zd-card-title">
          <CircleArrowUp size={15} /> {t("appCardTitle")}
          <Badge tone="muted">{t("currentVersion")} v{appVersion}</Badge>
          {appState.phase === "available" && (
            <Badge tone="accent">{t("newVersion")} v{appState.update.version}</Badge>
          )}
        </div>

        {appState.phase === "available" && appState.update.body && (
          <p className="upd-notes">{appState.update.body}</p>
        )}
        {appState.phase === "downloading" && (
          <div className="zd-fw-progress">
            <Progress percent={appState.percent} />
            <span>{appState.percent === null ? "..." : `${appState.percent}%`}</span>
          </div>
        )}
        {appState.phase === "error" && (
          <div className="zd-message zd-message-critical">{t("updateFailed")}: {appState.message}</div>
        )}

        <div className="zd-toolbar">
          {appState.phase === "ready" ? (
            <Button variant="primary" onClick={() => void invoke("app_restart")}>
              <RotateCcw size={14} /> {t("restartToUpdate")}
            </Button>
          ) : appState.phase === "available" ? (
            <Button variant="primary" onClick={() => void runAppInstall()}>
              <Download size={14} /> {t("downloadAndInstall")}
            </Button>
          ) : (
            <Button
              variant="ghost"
              disabled={appState.phase === "checking" || appState.phase === "downloading"}
              onClick={() => void runAppCheck()}
            >
              {appState.phase === "checking" ? <Spinner size={13} /> : <RefreshCw size={13} />}
              {t("checkUpdates")}
            </Button>
          )}
          {appState.phase === "upToDate" && <Badge tone="ok">{t("upToDate")}</Badge>}
        </div>
      </GlassCard>

      {/* 插件 */}
      <GlassCard>
        <div className="zd-card-title">
          <PackageCheck size={15} /> {t("loadedPlugins")}
        </div>
        <div className="upd-plugin-list">
          {plugins.map((p) => {
            const row = rows[p.manifest.id] ?? { phase: "unchecked" as const };
            const enabled = !!sources?.plugins[p.manifest.id]?.enabled;
            return (
              <motion.div
                key={p.manifest.id}
                className="upd-plugin-row"
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
              >
                <div className="upd-plugin-meta">
                  <span className="upd-plugin-name">{p.manifest.name}</span>
                  <span className="upd-plugin-id">{p.manifest.id}</span>
                </div>
                <Badge tone="muted">v{p.manifest.version}</Badge>
                <div className="upd-plugin-state">
                  {row.phase === "checking" && <Spinner size={13} />}
                  {row.phase === "upToDate" && <Badge tone="ok">{t("latest")}</Badge>}
                  {row.phase === "available" && <Badge tone="accent">v{row.info.version}</Badge>}
                  {row.phase === "installing" && <Spinner size={13} />}
                  {row.phase === "done" && <Badge tone="ok">{t("updated")} v{row.version}</Badge>}
                  {row.phase === "error" && <Badge tone="critical" >{t("failed")}</Badge>}
                </div>
                {row.phase === "available" ? (
                  <Button variant="primary" onClick={() => void runPluginInstall(p.manifest.id)}>
                    <Download size={13} /> {t("updates")}
                  </Button>
                ) : (
                  <Button
                    variant="ghost"
                    disabled={!enabled || row.phase === "checking" || row.phase === "installing"}
                    title={enabled ? undefined : t("noUpdateSource")}
                    onClick={() => void runPluginCheck(p.manifest.id, p.manifest.version)}
                  >
                    {t("check")}
                  </Button>
                )}
                {row.phase === "error" && (
                  <div className="upd-plugin-error">{row.message}</div>
                )}
                {row.phase === "available" && row.info.notes && (
                  <div className="upd-plugin-notes">{row.info.notes}</div>
                )}
              </motion.div>
            );
          })}
          {plugins.length === 0 && <p className="zd-hint">{t("noPluginsLoaded")}</p>}
        </div>
      </GlassCard>
    </div>
  );
}
