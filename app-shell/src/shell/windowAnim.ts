/**
 * 窗口几何动画。
 *
 * - macOS：走壳命令 `animate_resize`（NSWindow setFrame:display:animate:，
 *   系统级平滑、无逐帧 IPC、webview 不做中间态重排——消除泛白托尾与卡顿）；
 * - 其他平台：低频步进插值（12 步 / ~260ms，避免逐帧全量重排）；
 * - prefers-reduced-motion：直接到位。
 */

import { invoke } from "@tauri-apps/api/core";
import { useThemeStore } from "./theme";
import { PhysicalPosition, PhysicalSize, type Window } from "@tauri-apps/api/window";

let currentTimer: ReturnType<typeof setInterval> | null = null;

/**
 * 窗口几何动画期间暂停玻璃模糊（backdrop-filter 在 resize 逐帧重排中
 * 是主要掉帧来源）。引用计数支持动画重叠；动画结束后再保留约 80ms 余量恢复。
 */
const GLASS_PAUSE_MS = 560;
let glassPauseCount = 0;

function pauseGlassEffects(): void {
  glassPauseCount += 1;
  document.documentElement.classList.add("zt-animating");
  window.setTimeout(() => {
    glassPauseCount = Math.max(0, glassPauseCount - 1);
    if (glassPauseCount === 0) {
      document.documentElement.classList.remove("zt-animating");
    }
  }, GLASS_PAUSE_MS);
}

export async function animateWindowResize(
  win: Window,
  targetLogicalW: number,
  targetLogicalH: number,
): Promise<void> {
  pauseGlassEffects();
  // 优先原生动画（macOS）
  try {
    const res = await invoke<{ animated: boolean }>("animate_resize", {
      width: targetLogicalW,
      height: targetLogicalH,
      theme: useThemeStore.getState().resolved,
    });
    if (res.animated) return;
  } catch {
    // 命令不可用 → 走步进插值
  }

  const scale = await win.scaleFactor();
  const newW = Math.round(targetLogicalW * scale);
  const newH = Math.round(targetLogicalH * scale);
  const pos = await win.outerPosition();
  const cur = await win.outerSize();
  const cx = pos.x + cur.width / 2;
  const cy = pos.y + cur.height / 2;
  const x1 = Math.round(cx - newW / 2);
  const y1 = Math.round(cy - newH / 2);

  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduce) {
    await win.setSize(new PhysicalSize(newW, newH));
    await win.setPosition(new PhysicalPosition(x1, y1));
    return;
  }

  // 步进插值：12 步 ≈ 260ms，缓动与 macOS 一致（放大回弹 / 缩小强减速）
  if (currentTimer !== null) clearInterval(currentTimer);
  const [w0, h0, x0, y0] = [cur.width, cur.height, pos.x, pos.y];
  const expand = newW * newH > cur.width * cur.height;
  const t0 = performance.now();
  const easeOutBack = (t: number) => {
    const c1 = 1.1;
    const u = t - 1;
    return 1 + (c1 + 1) * u * u * u + c1 * u * u;
  };
  const easeOutQuint = (t: number) => 1 - Math.pow(1 - t, 5);
  const ease = (t: number) => (expand ? easeOutBack(t) : easeOutQuint(t));
  currentTimer = setInterval(() => {
    const t = Math.min(1, (performance.now() - t0) / 260);
    const e = ease(t);
    void win.setSize(new PhysicalSize(Math.round(w0 + (newW - w0) * e), Math.round(h0 + (newH - h0) * e)));
    void win.setPosition(new PhysicalPosition(Math.round(x0 + (x1 - x0) * e), Math.round(y0 + (y1 - y0) * e)));
    if (t >= 1 && currentTimer !== null) {
      clearInterval(currentTimer);
      currentTimer = null;
    }
  }, 22);
}
