/**
 * React hooks：总线订阅与插件调用的 React 绑定。
 */

import { useEffect, useRef, useState } from "react";
import { onBus } from "./ipc";

/** 订阅总线主题，组件卸载自动取消。handler 以 ref 保存，不触发重复订阅。 */
export function useBusEvent<T = unknown>(
  topic: string | ((topic: string) => boolean),
  handler: (payload: T, topic: string) => void,
): void {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onBus<T>(topic, (payload, t) => handlerRef.current(payload, t)).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // topic 标识应稳定；谓词形式请用 useCallback 固化。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [typeof topic === "string" ? topic : topic]);
}

export type ResolvedTheme = "dark" | "light";

/**
 * 订阅壳的解析后主题（深/浅）。壳切换主题时派发自定义事件 `zannen-theme`，
 * 插件与壳同一文档，天然同步。
 */
export function useTheme(): ResolvedTheme {
  const [theme, setTheme] = useState<ResolvedTheme>(() =>
    document.documentElement.dataset.theme === "light" ? "light" : "dark",
  );
  useEffect(() => {
    const handler = (e: Event) => setTheme((e as CustomEvent<ResolvedTheme>).detail);
    window.addEventListener("zannen-theme", handler);
    return () => window.removeEventListener("zannen-theme", handler);
  }, []);
  return theme;
}

