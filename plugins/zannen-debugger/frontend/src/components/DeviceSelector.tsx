/**
 * 调试设备选择下拉框：调试插件专属。
 *
 * - 按钮经 createPortal 渲染进壳顶栏插槽（`.topbar-slot`，与插件名面包屑同一行
 *   水平）；插槽未就绪时退化为视图右上角浮放。
 * - 下拉菜单独立传送门到 document.body 并以 position:fixed 按按钮位置定位，
 *   与所在栏位完全解耦——不受栏位 overflow/stacking context 裁剪与位移影响。
 */

import { AnimatePresence, motion } from "framer-motion";
import { Badge, useLocale } from "@zannen/plugin-sdk";
import { ChevronDown, Cpu, Unplug } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { t } from "../i18n";
import { PLUGIN_ID, useConnection } from "../state";

function useSlot(): HTMLElement | null {
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  useEffect(() => {
    let tries = 0;
    let raf = 0;
    const find = () => {
      const el = document.querySelector<HTMLElement>(`.topbar-slot[data-plugin-slot="${PLUGIN_ID}"]`);
      if (el) {
        setSlot(el);
      } else if (tries++ < 120) {
        raf = requestAnimationFrame(find);
      }
    };
    find();
    return () => cancelAnimationFrame(raf);
  }, []);
  return slot;
}

export function DeviceSelector() {
  const locale = useLocale();
  const conn = useConnection();
  const slot = useSlot();
  const [open, setOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ right: number; top: number } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const updateMenuPos = useCallback(() => {
    const r = btnRef.current?.getBoundingClientRect();
    if (r) {
      setMenuPos({ right: window.innerWidth - r.right, top: r.bottom + 8 });
    }
  }, []);

  // 打开时定位；打开期间跟随窗口滚动 / 缩放重定位
  useEffect(() => {
    if (!open) return;
    updateMenuPos();
    const onReposition = () => updateMenuPos();
    window.addEventListener("resize", onReposition);
    window.addEventListener("scroll", onReposition, true);
    return () => {
      window.removeEventListener("resize", onReposition);
      window.removeEventListener("scroll", onReposition, true);
    };
  }, [open, updateMenuPos]);

  // 点击按钮与菜单之外任意处关闭
  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      const target = e.target as Node;
      if (rootRef.current?.contains(target)) return;
      if (menuRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, []);

  const active = conn.sessions.find((s) => s.session === conn.session);

  const dropdown = (
    <div className={`zd-devsel${slot ? " is-inline" : ""}`} ref={rootRef}>
      <button
        className="topbar-device-btn"
        ref={btnRef}
        onClick={() => setOpen((v) => !v)}
      >
        <Cpu size={14} />
        <span>{active ? active.label : t(locale, "devices.selNone")}</span>
        <span className={`topbar-device-dot${active ? " is-online" : ""}`} />
        <ChevronDown size={12} />
      </button>
    </div>
  );

  const menu = (
    <AnimatePresence>
      {open && menuPos && (
        <motion.div
          className="topbar-device-menu zt-glass zd-devsel-menu"
          ref={menuRef}
          style={{ position: "fixed", top: menuPos.top, right: menuPos.right }}
          initial={{ opacity: 0, y: -6, scale: 0.98 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: -6, scale: 0.98 }}
          transition={{ duration: 0.16 }}
        >
          {conn.sessions.length === 0 && (
            <div className="topbar-device-empty">{t(locale, "devices.selEmpty")}</div>
          )}
          {conn.sessions.map((s) => (
            <div key={s.session} className="topbar-device-item zd-devsel-item">
              <button
                className="zd-devsel-main"
                onClick={() => {
                  conn.setActive(s.session);
                  setOpen(false);
                }}
              >
                <span className="topbar-device-label">
                  {s.label}
                  {s.session === conn.session && (
                    <Badge tone="ok">{t(locale, "common.active")}</Badge>
                  )}
                </span>
                <span className="topbar-device-id">
                  {s.path} · #{s.session}
                </span>
              </button>
              <button
                className="zd-devsel-close"
                title={t(locale, "common.disconnect")}
                onClick={() => {
                  void conn.disconnect(s.session);
                }}
              >
                <Unplug size={12} />
              </button>
            </div>
          ))}
        </motion.div>
      )}
    </AnimatePresence>
  );

  return (
    <>
      {slot ? createPortal(dropdown, slot) : dropdown}
      {createPortal(menu, document.body)}
    </>
  );
}
