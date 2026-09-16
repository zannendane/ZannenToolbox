/**
 * 固件视图：按设备能力分流 —— UF2 刷写三步流（nRF52）/ MCUboot SMP 串行 DFU（nRF54）。
 */

import { open } from "@tauri-apps/plugin-dialog";
import {
  Badge,
  Button,
  EmptyState,
  GlassCard,
  Progress,
  SectionTitle,
  Spinner,
  invokePlugin,
  useBusEvent,
  useLocale,
} from "@zannen/plugin-sdk";
import { ArrowDownToLine, FileCheck2, HardDriveDownload, PackageOpen, RefreshCw, RotateCcw, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { t, tf } from "../i18n";
import { PLUGIN_ID, useConnection } from "../state";

interface Uf2Summary {
  family_id?: string | null;
  num_blocks: number;
  payload_bytes: number;
  addr_min: number;
  addr_max: number;
  expected_family?: string | null;
  family_match?: boolean;
}

interface Uf2Volume {
  mount: string;
  info: string;
}

type Step = "pick" | "bootloader" | "flash";

/** 设备能力：mcuboot 在指纹表 capabilities 中声明（zannen-smol-air / nRF54 系）。 */
function isMcubootKind(kind: string | null): boolean {
  return kind === "zannen-smol-air";
}

export function FirmwareView() {
  const locale = useLocale();
  const conn = useConnection();
  const [filePath, setFilePath] = useState<string | null>(null);
  const [summary, setSummary] = useState<Uf2Summary | null>(null);
  const [volumes, setVolumes] = useState<Uf2Volume[]>([]);
  const [volume, setVolume] = useState<string | null>(null);
  const [entering, setEntering] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [job, setJob] = useState<string | null>(null);
  const [message, setMessage] = useState<{ kind: "ok" | "warn" | "critical" | "muted"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  useBusEvent<{ job: string; percent: number; done?: boolean }>("uf2.progress", (p) => {
    if (job && p.job === job) setProgress(p.percent);
  });
  useBusEvent<{ job: string; summary: Uf2Summary }>("uf2.done", (p) => {
    if (job && p.job === job) {
      setProgress(100);
      setBusy(false);
      setMessage({ kind: "ok", text: tf(locale, "firmware.done", { n: p.summary.num_blocks }) });
    }
  });
  useBusEvent<{ job: string; error: string }>("uf2.error", (p) => {
    if (job && p.job === job) {
      setBusy(false);
      setMessage({ kind: "critical", text: tf(locale, "firmware.failed", { error: p.error }) });
    }
  });

  const step: Step = summary === null ? "pick" : volume === null ? "bootloader" : "flash";

  const pickFile = async () => {
    const selected = await open({
      filters: [{ name: t(locale, "firmware.filterName"), extensions: ["uf2"] }],
      multiple: false,
    });
    if (typeof selected !== "string") return;
    setFilePath(selected);
    setMessage(null);
    try {
      const s = await invokePlugin<Uf2Summary>(PLUGIN_ID, "uf2.validate", {
        path: selected,
        ...(conn.kind ? { kind: conn.kind } : {}),
      });
      setSummary(s);
    } catch (e) {
      setSummary(null);
      setMessage({ kind: "critical", text: tf(locale, "firmware.validateFailed", { error: String(e) }) });
    }
  };

  const refreshVolumes = async () => {
    const v = await invokePlugin<Uf2Volume[]>(PLUGIN_ID, "uf2.volumes");
    setVolumes(v);
    if (v.length === 1) setVolume(v[0].mount);
  };

  const enterBootloader = async () => {
    if (conn.session === null) return;
    setEntering(true);
    setMessage(null);
    try {
      const res = await invokePlugin<{ found: boolean; volume?: Uf2Volume }>(
        PLUGIN_ID,
        "uf2.enter_bootloader",
        { session: conn.session },
      );
      if (res.found && res.volume) {
        setVolumes([res.volume]);
        setVolume(res.volume.mount);
        setMessage({ kind: "ok", text: tf(locale, "firmware.entered", { mount: res.volume.mount }) });
      } else {
        setMessage({ kind: "warn", text: t(locale, "firmware.notFound") });
        await refreshVolumes();
      }
    } catch (e) {
      setMessage({ kind: "critical", text: tf(locale, "firmware.enterFailed", { error: String(e) }) });
    } finally {
      setEntering(false);
    }
  };

  const flash = async () => {
    if (!filePath || !volume) return;
    setBusy(true);
    setProgress(0);
    setMessage(null);
    const jobId = `flash-${Date.now()}`;
    setJob(jobId);
    try {
      await invokePlugin(PLUGIN_ID, "uf2.flash", { path: filePath, volume, job: jobId });
    } catch (e) {
      setBusy(false);
      setMessage({ kind: "critical", text: tf(locale, "firmware.flashStartFailed", { error: String(e) }) });
    }
  };

  return (
    <div className="zd-view">
      <SectionTitle title={t(locale, "firmware.title")} desc={t(locale, "firmware.desc")} />

      <div className="zd-steps">
        <StepBadge active={step === "pick"} done={summary !== null} index={1} label={t(locale, "firmware.step1")} />
        <StepBadge active={step === "bootloader"} done={volume !== null} index={2} label="Bootloader" />
        <StepBadge active={step === "flash"} done={progress === 100} index={3} label={t(locale, "firmware.step3")} />
      </div>

      {message && (
        <div className={`zd-message zd-message-${message.kind}`}>{message.text}</div>
      )}

      <div className="zd-fw-grid">
        <GlassCard>
          <div className="zd-card-title">
            <FileCheck2 size={15} /> {t(locale, "firmware.card1")}
          </div>
          <div className="zd-fw-file">
            <Button variant="ghost" onClick={() => void pickFile()}>
              {t(locale, "firmware.pick")}
            </Button>
            <span className="zd-fw-path">{filePath ?? t(locale, "firmware.noFile")}</span>
          </div>
          {summary && (
            <div className="zd-fw-summary">
              <Row k="Family ID" v={summary.family_id ?? t(locale, "firmware.noFamily")} />
              {summary.expected_family !== undefined && (
                <Row
                  k={t(locale, "firmware.familyMatch")}
                  v={
                    summary.family_match ? (
                      <Badge tone="ok">{t(locale, "firmware.match")}</Badge>
                    ) : (
                      <Badge tone="critical">
                        {tf(locale, "firmware.mismatch", { expect: summary.expected_family ?? "?" })}
                      </Badge>
                    )
                  }
                />
              )}
              <Row k={t(locale, "firmware.blocks")} v={String(summary.num_blocks)} />
              <Row k={t(locale, "firmware.payload")} v={`${(summary.payload_bytes / 1024).toFixed(1)} KB`} />
              <Row
                k={t(locale, "firmware.addrRange")}
                v={`0x${summary.addr_min.toString(16)} – 0x${summary.addr_max.toString(16)}`}
              />
            </div>
          )}
        </GlassCard>

        <GlassCard>
          <div className="zd-card-title">
            <HardDriveDownload size={15} /> {t(locale, "firmware.card2")}
          </div>
          <div className="zd-fw-boot">
            <Button
              variant="primary"
              disabled={conn.session === null || entering}
              onClick={() => void enterBootloader()}
              title={conn.session === null ? t(locale, "firmware.needConnect") : undefined}
            >
              {entering ? <Spinner size={13} /> : <ArrowDownToLine size={14} />}{" "}
              {t(locale, "firmware.enterBootloader")}
            </Button>
            <Button variant="ghost" onClick={() => void refreshVolumes()}>
              <RefreshCw size={13} /> {t(locale, "firmware.refreshVolumes")}
            </Button>
          </div>
          {volumes.length > 0 ? (
            <div className="zd-fw-volumes">
              {volumes.map((v) => (
                <button
                  key={v.mount}
                  className={`zd-volume${volume === v.mount ? " is-active" : ""}`}
                  onClick={() => setVolume(v.mount)}
                >
                  <span className="zd-volume-mount">{v.mount}</span>
                  <span className="zd-volume-info">{v.info.split("\n")[0]}</span>
                </button>
              ))}
            </div>
          ) : (
            <p className="zd-hint">{t(locale, "firmware.noVolumes")}</p>
          )}
        </GlassCard>

        <GlassCard>
          <div className="zd-card-title">
            <ArrowDownToLine size={15} /> {t(locale, "firmware.card3")}
          </div>
          {summary === null || volume === null ? (
            <EmptyState title={t(locale, "firmware.finishFirst")} />
          ) : (
            <>
              <Button variant="primary" disabled={busy} onClick={() => void flash()}>
                {busy ? <Spinner size={13} /> : <ArrowDownToLine size={14} />} {t(locale, "firmware.flash")}
              </Button>
              {progress !== null && (
                <div className="zd-fw-progress">
                  <Progress percent={progress} />
                  <span>{progress}%</span>
                </div>
              )}
            </>
          )}
        </GlassCard>
      </div>

      {/* MCUboot 串行 DFU（nRF54 设备） */}
      {isMcubootKind(conn.kind) && <McuBootCard />}
    </div>
  );
}

/** MCUboot SMP 串行 DFU 卡片：选镜像 → 上传（断开调试会话独占串口）→ 确认 → 复位。 */
function McuBootCard() {
  const locale = useLocale();
  const conn = useConnection();
  const [imagePath, setImagePath] = useState<string | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [job, setJob] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [uploaded, setUploaded] = useState(false);
  const [dfuPath, setDfuPath] = useState<string | null>(null);
  const [message, setMessage] = useState<{ kind: "ok" | "warn" | "critical" | "muted"; text: string } | null>(null);

  useBusEvent<{ job: string; percent: number }>("dfu.progress", (p) => {
    if (job && p.job === job) setProgress(p.percent);
  });
  useBusEvent<{ job: string; bytes: number }>("dfu.done", (p) => {
    if (job && p.job === job) {
      setProgress(100);
      setBusy(false);
      setUploaded(true);
      setMessage({ kind: "ok", text: tf(locale, "firmware.mcuboot.uploaded", { n: p.bytes }) });
    }
  });
  useBusEvent<{ job: string; error: string }>("dfu.error", (p) => {
    if (job && p.job === job) {
      setBusy(false);
      setMessage({ kind: "critical", text: tf(locale, "firmware.mcuboot.failed", { error: p.error }) });
    }
  });

  const pickBin = async () => {
    const selected = await open({
      filters: [{ name: "MCUboot image", extensions: ["bin"] }],
      multiple: false,
    });
    if (typeof selected === "string") {
      setImagePath(selected);
      setUploaded(false);
      setProgress(null);
      setMessage(null);
    }
  };

  const doUpload = async () => {
    const path = conn.path;
    if (!path || !imagePath) return;
    if (path.startsWith("ble:")) {
      setMessage({ kind: "warn", text: t(locale, "firmware.mcuboot.needSerial") });
      return;
    }
    setBusy(true);
    setProgress(0);
    setMessage(null);
    setDfuPath(path);
    // 独占串口：先断开调试会话（mock 会话同理）
    if (conn.session !== null) {
      await conn.disconnect();
    }
    const jobId = `dfu-${Date.now()}`;
    setJob(jobId);
    try {
      await invokePlugin(PLUGIN_ID, "dfu.upload", { path, image_path: imagePath, job: jobId });
    } catch (e) {
      setBusy(false);
      setMessage({ kind: "critical", text: tf(locale, "firmware.mcuboot.failed", { error: String(e) }) });
    }
  };

  const doConfirm = async () => {
    if (!dfuPath) return;
    try {
      const res = await invokePlugin<{ confirmed: string }>(PLUGIN_ID, "dfu.confirm", { path: dfuPath });
      setMessage({ kind: "ok", text: tf(locale, "firmware.mcuboot.confirmed", { hash: res.confirmed.slice(0, 12) + "..." }) });
    } catch (e) {
      setMessage({ kind: "critical", text: tf(locale, "firmware.mcuboot.failed", { error: String(e) }) });
    }
  };

  const doReset = async () => {
    if (!dfuPath) return;
    try {
      await invokePlugin(PLUGIN_ID, "dfu.reset", { path: dfuPath });
      setMessage({ kind: "ok", text: t(locale, "firmware.mcuboot.resetDone") });
      setUploaded(false);
      setProgress(null);
    } catch (e) {
      setMessage({ kind: "critical", text: tf(locale, "firmware.mcuboot.failed", { error: String(e) }) });
    }
  };

  return (
    <GlassCard className="zd-mcuboot">
      <div className="zd-card-title">
        <PackageOpen size={15} /> {t(locale, "firmware.mcuboot.title")}
      </div>
      <div className="zd-fw-file">
        <Button variant="ghost" onClick={() => void pickBin()}>
          {t(locale, "firmware.mcuboot.pickBin")}
        </Button>
        <span className="zd-fw-path">{imagePath ?? t(locale, "firmware.mcuboot.noFile")}</span>
      </div>
      <p className="zd-hint">{t(locale, "firmware.mcuboot.disconnectNote")}</p>
      {message && <div className={`zd-message zd-message-${message.kind}`}>{message.text}</div>}
      <div className="zd-toolbar">
        <Button
          variant="primary"
          disabled={!imagePath || busy || !conn.path}
          title={!conn.path ? t(locale, "firmware.needConnect") : undefined}
          onClick={() => void doUpload()}
        >
          {busy ? <Spinner size={13} /> : <ArrowDownToLine size={14} />} {t(locale, "firmware.mcuboot.upload")}
        </Button>
        <Button variant="ghost" disabled={!uploaded || busy} onClick={() => void doConfirm()}>
          <ShieldCheck size={13} /> {t(locale, "firmware.mcuboot.confirm")}
        </Button>
        <Button variant="ghost" disabled={!uploaded || busy} onClick={() => void doReset()}>
          <RotateCcw size={13} /> {t(locale, "firmware.mcuboot.reset")}
        </Button>
      </div>
      {progress !== null && (
        <div className="zd-fw-progress">
          <Progress percent={progress} />
          <span>{progress}%</span>
        </div>
      )}
    </GlassCard>
  );
}

function StepBadge({ active, done, index, label }: { active: boolean; done: boolean; index: number; label: string }) {
  return (
    <span className={`zd-step${active ? " is-active" : ""}${done ? " is-done" : ""}`}>
      <span className="zd-step-index">{index}</span> {label}
    </span>
  );
}

function Row({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="zd-kv">
      <span className="zd-kv-k">{k}</span>
      <span className="zd-kv-v">{v}</span>
    </div>
  );
}
