/**
 * 自绘标题栏：macOS 用 Overlay 风格（保留原生红绿灯，标题栏仅为拖拽区）；
 * Windows 无边框，自绘最小化/最大化/关闭按钮。
 */

import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useT } from "./i18n";

const isWindows = navigator.userAgent.includes("Windows");
const isMac = navigator.userAgent.includes("Mac OS");

export function TitleBar() {
  const [maximized, setMaximized] = useState(false);
  const win = isWindows ? getCurrentWindow() : null;
  const t = useT();

  useEffect(() => {
    if (!win) return;
    let disposed = false;
    win.isMaximized().then((v) => !disposed && setMaximized(v));
    const unlisten = win.onResized(() => {
      win.isMaximized().then((v) => !disposed && setMaximized(v));
    });
    return () => {
      disposed = true;
      unlisten.then((fn) => fn());
    };
  }, [win]);

  return (
    <div className={`titlebar${isMac ? " titlebar-mac" : ""}`} data-tauri-drag-region>
      <span className="titlebar-title" data-tauri-drag-region>
        ZannenToolbox
      </span>
      {win ? (
        <div className="titlebar-controls">
          <button aria-label={t("minimize")} onClick={() => win.minimize()}>
            <Minus size={14} />
          </button>
          <button aria-label={t("maximize")} onClick={() => win.toggleMaximize()}>
            {maximized ? <Square size={12} /> : <Square size={12} />}
          </button>
          <button aria-label={t("close")} className="titlebar-close" onClick={() => win.close()}>
            <X size={14} />
          </button>
        </div>
      ) : null}
    </div>
  );
}
