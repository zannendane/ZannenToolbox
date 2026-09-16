/**
 * 设备总览：扫描（串口 identify + BLE 指纹）→ 设备卡片 → 一键连接。
 *
 * 多会话：可并存连接多台设备；`serial.closed` 总线事件（含 dfu 复位）自动清理本地会话。
 * BLE 直连：`transports` 含 "ble" 且无串口路径时，以 `ble:<address>` 为 path 连接。
 */

import {
  Badge,
  Button,
  EmptyState,
  GlassCard,
  SectionTitle,
  Spinner,
  invokePlugin,
  invokeShell,
  useBusEvent,
  useLocale,
} from "@zannen/plugin-sdk";
import { Bluetooth, BluetoothOff, Cable, Radio, RefreshCw, Unplug, Usb } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { t, tf } from "../i18n";
import { PLUGIN_ID, devicePath, useConnection, type DeviceEntry } from "../state";

/** macOS 蓝牙授权前置告知：系统授权框出现前先给上下文（TCC 首次触发会弹系统框）。 */
const BLE_ACK_KEY = "zannen.debugger.ble-ack";
const isMac = navigator.userAgent.includes("Mac OS");

/** 判断 BLE 错误是否为权限被拒（宿主服务返回 [E3201]，或 btleplug 原文）。 */
function isBlePermissionError(text: string): boolean {
  const s = text.toLowerCase();
  return s.includes("e3201") || s.includes("permission denied") || s.includes("not authorized");
}

export function DevicesView() {
  const locale = useLocale();
  const [devices, setDevices] = useState<DeviceEntry[]>([]);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [bleAcked, setBleAcked] = useState(() => localStorage.getItem(BLE_ACK_KEY) === "1");
  const [showBleNotice, setShowBleNotice] = useState(false);
  // BLE 权限被拒：非空时中止 BLE 功能并展示恢复指引（权限恢复后自动清除）
  const [bleDenied, setBleDenied] = useState<string | null>(null);
  const conn = useConnection();

  // 对端会话关闭（拔出/dfu 复位）时同步清理本地列表
  useBusEvent<{ session: number }>("serial.closed", ({ session }) => {
    conn.dropSession(session);
  });

  const scan = useCallback(
    async (opts?: { skipBle?: boolean; retryBle?: boolean }) => {
      setScanning(true);
      setError(null);
      // 权限被拒期间默认跳过 BLE（中止相关功能），显式"重试权限检测"除外
      const skipBle = opts?.skipBle || (bleDenied !== null && !opts?.retryBle);
      try {
        const res = await invokePlugin<{ devices: DeviceEntry[]; ble_error?: string | null }>(
          PLUGIN_ID,
          "device.scan",
          { ...(skipBle ? { skip_ble: true } : {}) },
        );
        setDevices(res.devices);
        // 权限状态跟进：被拒 → 记录并中止；本次无错误 → 视为已恢复
        if (res.ble_error && isBlePermissionError(res.ble_error)) {
          setBleDenied(res.ble_error);
        } else if (!skipBle) {
          setBleDenied(null);
        }
        // 也拉一次注册表（BLE 设备经 report 落表）
        const reg = await invokePlugin<DeviceEntry[]>(PLUGIN_ID, "device.list");
        setDevices((scanned) => {
          const map = new Map<string, DeviceEntry>();
          for (const d of [...reg, ...scanned]) map.set(d.id, d);
          return [...map.values()];
        });
      } catch (e) {
        setError(String(e));
      } finally {
        setScanning(false);
      }
    },
    [bleDenied],
  );

  // 挂载：macOS 且未确认过蓝牙告知 → 先串口扫描 + 弹告知框；否则直接全量扫描
  useEffect(() => {
    if (isMac && !bleAcked) {
      setShowBleNotice(true);
      void scan({ skipBle: true });
    } else {
      void scan();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const allowBle = () => {
    localStorage.setItem(BLE_ACK_KEY, "1");
    setBleAcked(true);
    setShowBleNotice(false);
    void scan(); // 系统授权框将随后出现
  };

  const skipBle = () => {
    setShowBleNotice(false); // 不持久化：下次启动仍会询问
  };

  /** 重试 BLE 权限检测（用户可能已在系统设置中恢复授权）。 */
  const retryBlePermission = () => void scan({ retryBle: true });

  /** 打开系统蓝牙权限设置页（被拒后手动恢复授权）。 */
  const openBluetoothSettings = () =>
    void invokeShell("open_permission_settings", { kind: "bluetooth" }).catch((e) =>
      setError(String(e)),
    );

  return (
    <div className="zd-view">
      <SectionTitle title={t(locale, "devices.title")} desc={t(locale, "devices.desc")} />
      <div className="zd-toolbar">
        <Button variant="primary" onClick={() => void scan()} disabled={scanning}>
          {scanning ? <Spinner size={13} /> : <RefreshCw size={14} />} {t(locale, "devices.scan")}
        </Button>
        {conn.sessions.length > 0 && (
          <span className="zd-session-chips">
            {conn.sessions.map((s) => (
              <span
                key={s.session}
                className={`zd-session-chip${s.session === conn.session ? " is-active" : ""}`}
              >
                <button
                  type="button"
                  className="zd-session-chip-label"
                  title={t(locale, "common.setActive")}
                  onClick={() => conn.setActive(s.session)}
                >
                  {s.label} · {s.session}
                </button>
                <button
                  type="button"
                  className="zd-session-chip-close"
                  title={t(locale, "common.disconnect")}
                  onClick={() => void conn.disconnect(s.session)}
                >
                  <Unplug size={11} />
                </button>
              </span>
            ))}
          </span>
        )}
      </div>

      {error && <div className="zd-error">{error}</div>}

      {/* 权限被拒通知：说明权限类型/用处/可能原因，BLE 功能中止直至恢复 */}
      {bleDenied && (
        <GlassCard className="zd-perm-notice">
          <div className="zd-perm-notice-head">
            <BluetoothOff size={16} />
            <span>{t(locale, "devices.bleDenied.title")}</span>
          </div>
          <p className="zd-perm-notice-desc">{t(locale, "devices.bleDenied.desc")}</p>
          <p className="zd-perm-notice-reasons">{t(locale, "devices.bleDenied.reasons")}</p>
          <div className="zd-perm-notice-code">{bleDenied}</div>
          <div className="zd-perm-notice-actions">
            <Button variant="primary" onClick={retryBlePermission} disabled={scanning}>
              {scanning ? <Spinner size={13} /> : <RefreshCw size={13} />}
              {t(locale, "devices.bleDenied.retry")}
            </Button>
            <Button variant="ghost" onClick={openBluetoothSettings}>
              {t(locale, "devices.bleDenied.openSettings")}
            </Button>
          </div>
        </GlassCard>
      )}

      {!scanning && devices.length === 0 && !error && (
        <GlassCard>
          <EmptyState
            icon={<Usb size={26} />}
            title={t(locale, "devices.empty")}
            desc={t(locale, "devices.emptyDesc")}
          />
        </GlassCard>
      )}

      <div className="zd-device-grid">
        {devices.map((d) => {
          const path = devicePath(d);
          const session = conn.sessions.find((s) => s.path === path);
          return (
            <GlassCard key={d.id} className="zd-device-card">
              <div className="zd-device-head">
                <span className="zd-device-name">{d.label}</span>
                {d.kind ? <Badge tone="accent">{d.kind}</Badge> : <Badge>{t(locale, "devices.unidentified")}</Badge>}
              </div>
              <div className="zd-device-id">{d.id}</div>
              <div className="zd-device-meta">
                {d.transports.includes("serial") && (
                  <Badge tone="muted">
                    <Cable size={11} /> {t(locale, "devices.serial")}
                  </Badge>
                )}
                {d.transports.includes("ble") && (
                  <Badge tone="muted">
                    <Radio size={11} /> BLE
                  </Badge>
                )}
                {d.extra?.fw ? <Badge tone="muted">fw {String(d.extra.fw)}</Badge> : null}
                {d.extra?.mock ? <Badge tone="warn">{t(locale, "devices.mock")}</Badge> : null}
              </div>
              {d.capabilities.length > 0 && (
                <div className="zd-device-caps">
                  {d.capabilities.map((c) => (
                    <span key={c} className="zd-cap">
                      {c}
                    </span>
                  ))}
                </div>
              )}
              <div className="zd-device-actions">
                {session ? (
                  <>
                    <Badge tone="ok">{tf(locale, "common.connectedSession", { n: session.session })}</Badge>
                    {session.session !== conn.session && (
                      <Button variant="ghost" onClick={() => conn.setActive(session.session)}>
                        {t(locale, "common.setActive")}
                      </Button>
                    )}
                    <Button variant="ghost" onClick={() => void conn.disconnect(session.session)}>
                      {t(locale, "common.disconnect")}
                    </Button>
                  </>
                ) : (
                  <Button
                    variant="ghost"
                    disabled={path === null || conn.connecting}
                    title={path === null ? t(locale, "devices.noPath") : undefined}
                    onClick={() => void conn.connect(d).catch((e) => setError(String(e)))}
                  >
                    {t(locale, "common.connect")}
                  </Button>
                )}
              </div>
            </GlassCard>
          );
        })}
      </div>

      {/* 蓝牙授权前置告知（macOS，系统授权框前给上下文） */}
      {showBleNotice && (
        <div className="zd-modal-overlay" onClick={skipBle}>
          <div className="zd-modal zt-glass" onClick={(e) => e.stopPropagation()}>
            <div className="zd-modal-icon">
              <Bluetooth size={22} />
            </div>
            <div className="zd-modal-title">{t(locale, "devices.bleNotice.title")}</div>
            <p className="zd-modal-desc">{t(locale, "devices.bleNotice.desc")}</p>
            <div className="zd-modal-actions">
              <Button variant="primary" onClick={allowBle}>
                {t(locale, "devices.bleNotice.allow")}
              </Button>
              <Button variant="ghost" onClick={skipBle}>
                {t(locale, "devices.bleNotice.later")}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
