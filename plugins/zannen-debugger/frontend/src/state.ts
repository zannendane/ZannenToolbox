/**
 * 调试模块共享状态：连接会话（跨视图复用）。
 *
 * 多会话：`sessions` 保存全部已连接会话，`session/path/label/kind`
 * 为"当前活跃"会话的便捷字段（串口终端、固件刷写等单会话视图使用）。
 */

import { invokePlugin } from "@zannen/plugin-sdk";
import { create } from "zustand";

export const PLUGIN_ID = "zannen.debugger";

export interface DeviceEntry {
  id: string;
  kind?: string | null;
  label: string;
  transports: string[];
  capabilities: string[];
  extra?: { path?: string; address?: string; fw?: string; rssi?: number | null; mock?: boolean };
}

export interface SessionInfo {
  session: number;
  path: string;
  label: string;
  kind: string | null;
}

/** 设备可连接路径：优先串口，否则 BLE 地址走 `ble:` 前缀（后端路由到 BLE 数据面）。 */
export function devicePath(dev: DeviceEntry): string | null {
  if (dev.extra?.path) return dev.extra.path;
  if (dev.transports.includes("ble") && dev.extra?.address) return `ble:${dev.extra.address}`;
  return null;
}

interface ConnectionState {
  /** 当前活跃会话（便捷字段；无连接时全为 null）。 */
  session: number | null;
  path: string | null;
  label: string | null;
  kind: string | null;
  /** 全部已连接会话。 */
  sessions: SessionInfo[];
  connecting: boolean;
  connect: (dev: DeviceEntry) => Promise<number>;
  /** 断开指定会话（缺省为当前活跃会话）。 */
  disconnect: (session?: number) => Promise<void>;
  /** 把指定会话设为活跃。 */
  setActive: (session: number) => void;
  /** 仅从本地列表移除会话（对端已关闭，如 dfu 复位后）。 */
  dropSession: (session: number) => void;
}

function activeFields(info: SessionInfo | null) {
  return {
    session: info?.session ?? null,
    path: info?.path ?? null,
    label: info?.label ?? null,
    kind: info?.kind ?? null,
  };
}

export const useConnection = create<ConnectionState>((set, get) => ({
  session: null,
  path: null,
  label: null,
  kind: null,
  sessions: [],
  connecting: false,

  async connect(dev) {
    const path = devicePath(dev);
    if (!path) throw new Error("device has no serial path or BLE address");
    // 同路径已有会话：直接切换活跃，不重复打开
    const existing = get().sessions.find((s) => s.path === path);
    if (existing) {
      set(activeFields(existing));
      return existing.session;
    }
    set({ connecting: true });
    try {
      const res = await invokePlugin<{ session: number }>(PLUGIN_ID, "device.connect", { path });
      const info: SessionInfo = { session: res.session, path, label: dev.label, kind: dev.kind ?? null };
      set((s) => ({ sessions: [...s.sessions, info], ...activeFields(info), connecting: false }));
      return res.session;
    } catch (e) {
      set({ connecting: false });
      throw e;
    }
  },

  async disconnect(session) {
    const target = session ?? get().session;
    if (target === null || target === undefined) return;
    await invokePlugin(PLUGIN_ID, "device.disconnect", { session: target }).catch(() => {});
    get().dropSession(target);
  },

  setActive(session) {
    const info = get().sessions.find((s) => s.session === session);
    if (info) set(activeFields(info));
  },

  dropSession(session) {
    set((s) => {
      const sessions = s.sessions.filter((x) => x.session !== session);
      if (s.session !== session) return { sessions };
      // 活跃会话被移除：回退到最近一个剩余会话
      return { sessions, ...activeFields(sessions[sessions.length - 1] ?? null) };
    });
  },
}));

export { hexPretty, hexToText } from "./hex";
