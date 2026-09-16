/**
 * 状态解读：设备 status 包 → 规则引擎卡片（电池/充电/传感器/RF/固件）。
 */

import { Badge, EmptyState, GlassCard, SectionTitle, useBusEvent, useLocale } from "@zannen/plugin-sdk";
import { BatteryMedium, Compass, PlugZap, RadioTower, Archive, Stethoscope } from "lucide-react";
import { useState } from "react";
import { t, tf, type I18nKey } from "../i18n";
import { useConnection } from "../state";

interface StatusCard {
  id: string;
  title: string;
  icon: string;
  value: unknown;
  unit: string;
  level: "ok" | "warn" | "critical" | string;
  note: string;
}

interface StatusEvent {
  device: string;
  raw: Record<string, unknown>;
  cards: StatusCard[];
}

const ICONS: Record<string, typeof BatteryMedium> = {
  "battery-medium": BatteryMedium,
  "plug-zap": PlugZap,
  compass: Compass,
  "radio-tower": RadioTower,
  archive: Archive,
};

const LEVEL_KEY: Record<string, I18nKey> = {
  ok: "status.level.ok",
  warn: "status.level.warn",
  critical: "status.level.critical",
};

export function StatusView() {
  const locale = useLocale();
  const conn = useConnection();
  const [latest, setLatest] = useState<Record<string, StatusEvent>>({});

  useBusEvent<StatusEvent>("zannen.debugger/status", (ev) => {
    setLatest((prev) => ({ ...prev, [ev.device]: ev }));
  });

  const entries = Object.values(latest);
  const current = conn.label ? latest[conn.label] : entries[0];

  return (
    <div className="zd-view">
      <SectionTitle title={t(locale, "status.title")} desc={t(locale, "status.desc")} />
      {!current ? (
        <GlassCard>
          <EmptyState
            icon={<Stethoscope size={26} />}
            title={t(locale, "status.empty")}
            desc={
              conn.session === null
                ? t(locale, "status.emptyNeedConnect")
                : t(locale, "status.emptyWaiting")
            }
          />
        </GlassCard>
      ) : (
        <>
          <div className="zd-status-grid">
            {current.cards.map((card) => {
              const Icon = ICONS[card.icon] ?? Archive;
              return (
                <GlassCard key={card.id} className={`zd-status-card is-${card.level}`}>
                  <div className="zd-status-head">
                    <Icon size={15} />
                    <span>{card.title}</span>
                    <Badge tone={card.level === "ok" ? "ok" : card.level === "warn" ? "warn" : "critical"}>
                      {t(locale, LEVEL_KEY[card.level] ?? "status.level.critical")}
                    </Badge>
                  </div>
                  <div className="zd-status-value">
                    {formatValue(card.value, locale)}
                    <span className="zd-status-unit">{card.unit}</span>
                  </div>
                  <div className="zd-status-note">{card.note}</div>
                </GlassCard>
              );
            })}
          </div>
          <GlassCard className="zd-status-raw">
            <div className="zd-card-title">{tf(locale, "status.raw", { device: current.device })}</div>
            <pre className="zd-raw-json">{JSON.stringify(current.raw, null, 2)}</pre>
          </GlassCard>
        </>
      )}
    </div>
  );
}

function formatValue(v: unknown, locale: Parameters<typeof t>[0]): string {
  if (typeof v === "number") return Number.isInteger(v) ? String(v) : v.toFixed(1);
  if (typeof v === "boolean") return t(locale, v ? "status.yes" : "status.no");
  return String(v ?? "-");
}
