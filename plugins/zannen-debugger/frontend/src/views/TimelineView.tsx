/**
 * 数据时间轴：IMU（加速度/陀螺仪）与 RF（RSSI）原始数据的流式时间轴图表。
 *
 * 性能设计：
 * - uPlot（canvas 渲染，~45KB）替代 DOM/SVG 图表库；
 * - 数据缓冲在 React 之外的 ref 数组，按设备 label 分桶（Map<string, Buffers>），
 *   rAF 驱动 setData，切设备只换 buffersRef 指向，不重建图表；
 * - 后端已按 ~30Hz 批量推送（imu.batch / rf.batch），此处不再二次节流。
 *
 * 图例项可点击隐藏/显示通道（uPlot setSeries）；暂停时图例 live 值随游标读数。
 */

import { save } from "@tauri-apps/plugin-dialog";
import {
  Button,
  EmptyState,
  GlassCard,
  Segmented,
  SectionTitle,
  invokeShell,
  useBusEvent,
  useLocale,
  useTheme,
  type Locale,
} from "@zannen/plugin-sdk";
import { Download, LineChart, Pause, Play, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import {
  TIMELINE_CAPACITY,
  bucketFor,
  emptyBuffers,
  pushImuSamples,
  pushRfSamples,
  sampleDevice,
  type BufferBuckets,
  type Buffers,
  type ImuSample,
  type RfSample,
} from "../buffers";
import { fileStamp, imuCsv, rssiCsv, withSuffix } from "../export";
import { t, tf, type I18nKey } from "../i18n";
import { useConnection } from "../state";

const SERIES_COLORS = ["#6e8bff", "#9b7bff", "#34c77b", "#f5a524"];

interface ChartDef {
  titleKey: I18nKey;
  unit: string;
  series: string[];
  pick: (b: Buffers) => uPlot.AlignedData;
}

const CHARTS: ChartDef[] = [
  {
    titleKey: "timeline.chart.accel",
    unit: "g",
    series: ["ax", "ay", "az"],
    pick: (b) => [b.tImu, b.ax, b.ay, b.az],
  },
  {
    titleKey: "timeline.chart.gyro",
    unit: "°/s",
    series: ["gx", "gy", "gz"],
    pick: (b) => [b.tImu, b.gx, b.gy, b.gz],
  },
  {
    titleKey: "timeline.chart.rssi",
    unit: "dBm",
    series: ["rssi"],
    pick: (b) => [b.tRf, b.rssi],
  },
];

function makeOpts(title: string, series: string[], width: number, dark: boolean): uPlot.Options {
  const axisStroke = dark ? "rgba(255,255,255,0.45)" : "rgba(15,23,42,0.5)";
  const gridStroke = dark ? "rgba(255,255,255,0.06)" : "rgba(15,23,42,0.08)";
  const tickStroke = dark ? "rgba(255,255,255,0.12)" : "rgba(15,23,42,0.15)";
  return {
    title,
    width,
    height: 190,
    padding: [8, 8, 0, 0],
    legend: { show: true, live: true },
    cursor: { drag: { x: false, y: false }, focus: { prox: 24 } },
    scales: { x: { time: false } },
    hooks: {
      ready: [
        (u) => {
          // 图例行不含 x 序列（legend.live 模式），行 i 对应 series[i+1]
          u.root.querySelectorAll<HTMLElement>(".u-legend .u-series").forEach((row, i) => {
            row.addEventListener("click", () => {
              const idx = i + 1;
              u.setSeries(idx, { show: !u.series[idx].show });
            });
          });
        },
      ],
    },
    axes: [
      {
        stroke: axisStroke,
        grid: { stroke: gridStroke, width: 1 },
        ticks: { stroke: tickStroke },
        font: "10px " + getComputedStyle(document.body).getPropertyValue("--zt-font-mono"),
        values: (_u, splits) => splits.map((v) => `${(v / 1000).toFixed(1)}s`),
      },
      {
        stroke: axisStroke,
        grid: { stroke: gridStroke, width: 1 },
        ticks: { stroke: tickStroke },
        size: 44,
      },
    ],
    series: [
      { label: "t" },
      ...series.map((label, i) => ({
        label,
        stroke: SERIES_COLORS[i % SERIES_COLORS.length],
        width: 1.5,
        points: { show: false },
      })),
    ],
  };
}

function StreamChart({
  def,
  buffers,
  pausedRef,
  versionRef,
  theme,
  locale,
}: {
  def: ChartDef;
  buffers: React.RefObject<Buffers>;
  pausedRef: React.RefObject<boolean>;
  /** 数据源版本号：暂停中切换设备时也要强制刷新一次画面。 */
  versionRef: React.RefObject<number>;
  theme: "dark" | "light";
  locale: Locale;
}) {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const chart = new uPlot(
      makeOpts(t(locale, def.titleKey), def.series, host.clientWidth, theme === "dark"),
      def.pick(buffers.current),
      host,
    );

    const ro = new ResizeObserver(() => {
      chart.setSize({ width: host.clientWidth, height: 190 });
    });
    ro.observe(host);

    let raf = 0;
    let lastVersion = -1;
    const tick = () => {
      if (!pausedRef.current || versionRef.current !== lastVersion) {
        lastVersion = versionRef.current;
        chart.setData(def.pick(buffers.current));
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      chart.destroy();
    };
    // def/buffers/versionRef 在组件生命周期内稳定；theme/locale 变化时重建以换配色与标题
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [theme, locale]);

  return (
    <GlassCard className="zd-chart-card">
      <div className="zd-chart-head">
        <span className="zd-chart-title">{t(locale, def.titleKey)}</span>
        <span className="zd-chart-unit">{def.unit}</span>
      </div>
      <div ref={hostRef} className="zd-chart-host" />
    </GlassCard>
  );
}

export function TimelineView() {
  const locale = useLocale();
  const conn = useConnection();
  const theme = useTheme();
  const [paused, setPaused] = useState(false);
  const pausedRef = useRef(false);
  pausedRef.current = paused;
  const bucketsRef = useRef<BufferBuckets>(new Map());
  const buffersRef = useRef<Buffers>(emptyBuffers());
  const versionRef = useRef(0);
  const [sampleCount, setSampleCount] = useState(0);
  const countRef = useRef(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [message, setMessage] = useState<{ kind: "ok" | "critical"; text: string } | null>(null);

  // 设备选择器：选项来自已连接会话（label 即样本的 device 字段）
  const deviceLabels = [...new Set(conn.sessions.map((s) => s.label))];
  const sel =
    selected && deviceLabels.includes(selected)
      ? selected
      : conn.label && deviceLabels.includes(conn.label)
        ? conn.label
        : (deviceLabels[0] ?? null);

  // 切设备：换 buffersRef 指向对应分桶，并 bump 版本让暂停中的图表也重绘
  useEffect(() => {
    if (sel === null) return;
    buffersRef.current = bucketFor(bucketsRef.current, sel);
    versionRef.current += 1;
  }, [sel]);

  useBusEvent<{ samples: ImuSample[] }>("zannen.debugger/imu.batch", ({ samples }) => {
    const fallback = useConnection.getState().label ?? "";
    const byDev = new Map<string, ImuSample[]>();
    for (const s of samples) {
      const dev = sampleDevice(s, fallback);
      if (!dev) continue;
      let arr = byDev.get(dev);
      if (!arr) byDev.set(dev, (arr = []));
      arr.push(s);
    }
    for (const [dev, arr] of byDev) {
      pushImuSamples(bucketFor(bucketsRef.current, dev), arr);
    }
    countRef.current += samples.length;
  });

  useBusEvent<{ samples: RfSample[] }>("zannen.debugger/rf.batch", ({ samples }) => {
    const fallback = useConnection.getState().label ?? "";
    const byDev = new Map<string, RfSample[]>();
    for (const s of samples) {
      const dev = sampleDevice(s, fallback);
      if (!dev) continue;
      let arr = byDev.get(dev);
      if (!arr) byDev.set(dev, (arr = []));
      arr.push(s);
    }
    for (const [dev, arr] of byDev) {
      pushRfSamples(bucketFor(bucketsRef.current, dev), arr);
    }
    countRef.current += samples.length;
  });

  // 低频刷新样本计数（1Hz 足够）
  useEffect(() => {
    const timer = setInterval(() => setSampleCount(countRef.current), 1000);
    return () => clearInterval(timer);
  }, []);

  const exportCsv = async () => {
    setMessage(null);
    const base = await save({
      filters: [{ name: "CSV", extensions: ["csv"] }],
      defaultPath: `zannen-${sel ?? "timeline"}-${fileStamp()}.csv`,
    });
    if (!base) return;
    const b = buffersRef.current;
    const imuPath = withSuffix(base, ".imu.csv");
    const rfPath = withSuffix(base, ".rssi.csv");
    try {
      await invokeShell("export_text_file", { path: imuPath, content: imuCsv(b) });
      await invokeShell("export_text_file", { path: rfPath, content: rssiCsv(b) });
      setMessage({ kind: "ok", text: tf(locale, "timeline.exported", { imu: imuPath, rf: rfPath }) });
    } catch (e) {
      setMessage({ kind: "critical", text: tf(locale, "timeline.exportFailed", { error: String(e) }) });
    }
  };

  if (conn.session === null) {
    return (
      <div className="zd-view">
        <SectionTitle title={t(locale, "timeline.title")} desc={t(locale, "timeline.desc")} />
        <GlassCard>
          <EmptyState
            icon={<LineChart size={26} />}
            title={t(locale, "common.notConnected")}
            desc={t(locale, "timeline.emptyDesc")}
          />
        </GlassCard>
      </div>
    );
  }

  return (
    <div className="zd-view">
      <SectionTitle
        title={t(locale, "timeline.title")}
        desc={tf(locale, "timeline.descConnected", {
          label: sel ?? "",
          n: sampleCount,
          cap: TIMELINE_CAPACITY,
        })}
      />
      <div className="zd-toolbar">
        {deviceLabels.length > 1 && sel !== null && (
          <>
            <span className="zd-toolbar-label">{t(locale, "timeline.source")}</span>
            <Segmented
              options={deviceLabels.map((l) => ({ value: l, label: l }))}
              value={sel}
              onChange={setSelected}
            />
          </>
        )}
        <Button variant="ghost" onClick={() => setPaused((v) => !v)}>
          {paused ? <Play size={13} /> : <Pause size={13} />}{" "}
          {paused ? t(locale, "common.resume") : t(locale, "common.pause")}
        </Button>
        <Button
          variant="ghost"
          onClick={() => {
            if (sel !== null) bucketsRef.current.set(sel, emptyBuffers());
            buffersRef.current = sel !== null ? bucketFor(bucketsRef.current, sel) : emptyBuffers();
            versionRef.current += 1;
          }}
        >
          <Trash2 size={13} /> {t(locale, "timeline.clear")}
        </Button>
        <Button variant="ghost" onClick={() => void exportCsv()}>
          <Download size={13} /> {t(locale, "timeline.exportCsv")}
        </Button>
      </div>
      <p className="zd-hint">
        {paused ? t(locale, "timeline.pausedHint") : t(locale, "timeline.legendHint")}
        {" · "}
        {t(locale, "timeline.exportHint")}
      </p>
      {message && <div className={`zd-message zd-message-${message.kind}`}>{message.text}</div>}
      <div className="zd-chart-stack">
        {CHARTS.map((def) => (
          <StreamChart
            key={def.titleKey}
            def={def}
            buffers={buffersRef}
            pausedRef={pausedRef}
            versionRef={versionRef}
            theme={theme}
            locale={locale}
          />
        ))}
      </div>
    </div>
  );
}
