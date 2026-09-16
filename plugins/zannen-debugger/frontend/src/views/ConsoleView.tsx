/**
 * 串口终端：RX 滚动视图（文本/HEX）+ TX 输入行。
 * 高频保护：接收先进 ref 缓冲，100ms 批量落 state。
 * 输入行支持 ↑/↓ 命令历史（到底回到当前草稿）；日志可经保存对话框导出。
 */

import { save } from "@tauri-apps/plugin-dialog";
import {
  Button,
  EmptyState,
  GlassCard,
  Segmented,
  SectionTitle,
  Toggle,
  invokePlugin,
  invokeShell,
  useBusEvent,
  useLocale,
} from "@zannen/plugin-sdk";
import { Download, Eraser, Send, TerminalSquare } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { fileStamp, formatLogLines, type LogLine } from "../export";
import { t, tf } from "../i18n";
import { PLUGIN_ID, hexPretty, hexToText, useConnection } from "../state";

type Encoding = "text" | "hex";
type Append = "none" | "lf" | "crlf";

const MAX_LINES = 4000;
const MAX_HISTORY = 200;

export function ConsoleView() {
  const locale = useLocale();
  const conn = useConnection();
  const [lines, setLines] = useState<LogLine[]>([]);
  const [input, setInput] = useState("");
  const [encoding, setEncoding] = useState<Encoding>("text");
  const [append, setAppend] = useState<Append>("lf");
  const [showTs, setShowTs] = useState(true);
  const pendingRef = useRef<LogLine[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickBottomRef = useRef(true);
  // 命令历史：会话内内存保存；histIdx 为 null 表示未在翻阅
  const historyRef = useRef<string[]>([]);
  const histIdxRef = useRef<number | null>(null);
  const draftRef = useRef("");

  // 批量落盘，避免每个 rx 块都触发渲染
  useEffect(() => {
    const timer = setInterval(() => {
      if (pendingRef.current.length === 0) return;
      const batch = pendingRef.current;
      pendingRef.current = [];
      setLines((prev) => {
        const next = [...prev, ...batch];
        return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
      });
    }, 100);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    if (stickBottomRef.current) {
      scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
    }
  }, [lines]);

  useBusEvent<{ session: number; hex: string }>("serial.rx", (payload) => {
    if (conn.session === null || payload.session !== conn.session) return;
    const text = encoding === "hex" ? hexPretty(payload.hex) : hexToText(payload.hex);
    for (const line of text.replace(/\r/g, "").split("\n")) {
      if (line === "") continue;
      pendingRef.current.push({ dir: "rx", text: line, ts: timeNow() });
    }
  });

  useBusEvent<{ session: number; error?: string }>(
    (t) => t === "serial.error" || t === "serial.closed",
    (payload, topic) => {
      if (conn.session === null || payload.session !== conn.session) return;
      pendingRef.current.push({
        dir: "sys",
        text:
          topic === "serial.closed"
            ? t(locale, "console.sysClosed")
            : tf(locale, "console.sysError", { error: payload.error ?? "" }),
        ts: timeNow(),
      });
    },
  );

  const send = async () => {
    if (conn.session === null || input === "") return;
    const data = input;
    setInput("");
    const h = historyRef.current;
    if (h[h.length - 1] !== data) {
      h.push(data);
      if (h.length > MAX_HISTORY) h.splice(0, h.length - MAX_HISTORY);
    }
    histIdxRef.current = null;
    pendingRef.current.push({
      dir: "tx",
      text: encoding === "hex" ? hexPretty(data.replace(/\s/g, "")) : data,
      ts: timeNow(),
    });
    try {
      await invokePlugin(PLUGIN_ID, "serial.write", {
        session: conn.session,
        data: encoding === "hex" ? data.replace(/\s/g, "") : data,
        encoding,
        append,
      });
    } catch (e) {
      pendingRef.current.push({
        dir: "sys",
        text: tf(locale, "console.sendFailed", { error: String(e) }),
        ts: timeNow(),
      });
    }
  };

  const onInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      void send();
      return;
    }
    const h = historyRef.current;
    if (e.key === "ArrowUp") {
      if (h.length === 0) return;
      e.preventDefault();
      if (histIdxRef.current === null) {
        draftRef.current = input;
        histIdxRef.current = h.length - 1;
      } else if (histIdxRef.current > 0) {
        histIdxRef.current -= 1;
      }
      setInput(h[histIdxRef.current]);
    } else if (e.key === "ArrowDown") {
      if (histIdxRef.current === null) return;
      e.preventDefault();
      if (histIdxRef.current < h.length - 1) {
        histIdxRef.current += 1;
        setInput(h[histIdxRef.current]);
      } else {
        // 越过最新一条：回到进入历史前的草稿
        histIdxRef.current = null;
        setInput(draftRef.current);
      }
    }
  };

  const exportLog = async () => {
    const path = await save({
      filters: [{ name: t(locale, "console.exportFilter"), extensions: ["log", "txt"] }],
      defaultPath: `zannen-console-${fileStamp()}.log`,
    });
    if (!path) return;
    try {
      await invokeShell("export_text_file", { path, content: formatLogLines(lines, showTs) });
      pendingRef.current.push({
        dir: "sys",
        text: tf(locale, "console.exported", { n: lines.length, path }),
        ts: timeNow(),
      });
    } catch (e) {
      pendingRef.current.push({
        dir: "sys",
        text: tf(locale, "console.exportFailed", { error: String(e) }),
        ts: timeNow(),
      });
    }
  };

  if (conn.session === null) {
    return (
      <div className="zd-view">
        <SectionTitle title={t(locale, "console.title")} desc={t(locale, "console.descIdle")} />
        <GlassCard>
          <EmptyState
            icon={<TerminalSquare size={26} />}
            title={t(locale, "common.notConnected")}
            desc={t(locale, "console.emptyDesc")}
          />
        </GlassCard>
      </div>
    );
  }

  return (
    <div className="zd-view zd-console-wrap">
      <SectionTitle
        title={t(locale, "console.title")}
        desc={tf(locale, "console.descConnected", { label: conn.label ?? "", n: conn.session })}
      />
      <div className="zd-toolbar">
        <Segmented
          options={[
            { value: "text", label: t(locale, "console.encoding.text") },
            { value: "hex", label: "HEX" },
          ]}
          value={encoding}
          onChange={setEncoding}
        />
        <Segmented
          options={[
            { value: "none", label: t(locale, "console.append.none") },
            { value: "lf", label: "LF" },
            { value: "crlf", label: "CRLF" },
          ]}
          value={append}
          onChange={setAppend}
        />
        <Toggle checked={showTs} onChange={setShowTs} label={t(locale, "console.timestamps")} />
        <Button variant="ghost" onClick={() => void exportLog()} disabled={lines.length === 0}>
          <Download size={13} /> {t(locale, "console.export")}
        </Button>
        <Button variant="ghost" onClick={() => setLines([])}>
          <Eraser size={13} /> {t(locale, "common.clear")}
        </Button>
      </div>

      <GlassCard padded={false} className="zd-console">
        <div
          ref={scrollRef}
          className="zd-console-scroll"
          onScroll={(e) => {
            const el = e.currentTarget;
            stickBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
          }}
        >
          {lines.map((l, i) => (
            <div key={i} className={`zd-console-line zd-console-${l.dir}`}>
              {showTs && <span className="zd-console-ts">{l.ts}</span>}
              <span className="zd-console-text">{l.text}</span>
            </div>
          ))}
        </div>
        <div className="zd-console-input-row">
          <span className="zd-console-prompt">›</span>
          <input
            className="zd-console-input"
            placeholder={
              encoding === "hex" ? t(locale, "console.placeholderHex") : '{"cmd":"ping"}'
            }
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onInputKeyDown}
            spellCheck={false}
          />
          <Button variant="primary" onClick={() => void send()} disabled={input === ""}>
            <Send size={13} /> {t(locale, "common.send")}
          </Button>
        </div>
      </GlassCard>
    </div>
  );
}

function timeNow(): string {
  const d = new Date();
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}
