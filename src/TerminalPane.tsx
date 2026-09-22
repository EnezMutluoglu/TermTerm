import { useEffect, useRef, useState } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebglAddon } from "@xterm/addon-webgl";
import {
  Search,
  X,
  Terminal,
  ClipboardPaste,
  Copy,
  Trash2,
} from "lucide-react";
import ResourceMonitor from "./ResourceMonitor";
import { shortcutFor, shortcutLabel } from "./shortcuts";
import { terminalTheme } from "./terminalThemes";
import "@xterm/xterm/css/xterm.css";
import { call, onSession, terminalHistory } from "./api";
import type { Session } from "./types";
export default function TerminalPane({
  session,
  focused,
  onFocus,
  onClose,
  onInput,
  fontSize = 14,
  themeId = "graphite",
  visible = true,
  onStats,
  onError,
}: {
  session: Session;
  focused: boolean;
  onFocus: () => void;
  onClose: () => void;
  onInput: (id: string, data: string) => void;
  fontSize?: number;
  themeId?: string;
  visible?: boolean;
  onStats?: (enabled: boolean) => void;
  onError?: (e: unknown) => void;
}) {
  const container = useRef<HTMLDivElement>(null);
  const term = useRef<XTerm | null>(null);
  const finder = useRef<SearchAddon | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const input = useRef(onInput);
  input.current = onInput;
  const [search, setSearch] = useState(false);
  const [query, setQuery] = useState("");
  useEffect(() => {
    if (!container.current) return;
    const t = new XTerm({
      cursorBlink: true,
      fontFamily: "Cascadia Code, Consolas, monospace",
      fontSize,
      lineHeight: 1.25,
      scrollback: 10000,
      allowProposedApi: false,
      theme: terminalTheme(themeId),
    });
    const fit = new FitAddon();
    fitRef.current = fit;
    const searchAddon = new SearchAddon();
    t.loadAddon(fit);
    t.loadAddon(searchAddon);
    t.open(container.current);
    try {
      const gl = new WebglAddon();
      gl.onContextLoss(() => gl.dispose());
      t.loadAddon(gl);
    } catch {
      /* Canvas fallback is supported by xterm. */
    }
    term.current = t;
    finder.current = searchAddon;
    for (const bytes of terminalHistory.get(session.id) ?? []) t.write(bytes);
    const data = t.onData((data) => input.current(session.id, data));
    const resize = () => {
      if (container.current && container.current.clientWidth > 0) {
        fit.fit();
        void call("session_input", {
          id: session.id,
          cols: t.cols,
          rows: t.rows,
        }).catch(() => {});
      }
    };
    const observer = new ResizeObserver(resize);
    observer.observe(container.current);
    resize();
    const unsubscribe = onSession((e) => {
      if (e.id !== session.id) return;
      if (e.kind === "data")
        t.write(Uint8Array.from(atob(e.detail), (c) => c.charCodeAt(0)));
      if (e.kind === "error")
        t.writeln(
          `\r\n\x1b[31m${String(e.detail).replace(/[\x00-\x1f\x7f]/g, " ")}\x1b[0m`,
        );
      if (e.kind === "closed") t.writeln("\r\n\x1b[90mSession closed.\x1b[0m");
    });
    t.attachCustomKeyEventHandler((e) => {
      const action = shortcutFor(e, true);
      if (action === "find") {
        e.stopPropagation();
        e.preventDefault();
        if (e.type === "keydown") setSearch((v) => !v);
        return false;
      }
      if (action === "copy") {
        e.stopPropagation();
        e.preventDefault();
        if (e.type === "keydown" && t.hasSelection())
          void navigator.clipboard
            .writeText(t.getSelection())
            .catch((e) => onError?.(e));
        return false;
      }
      if (action === "paste") {
        e.stopPropagation();
        e.preventDefault();
        if (e.type === "keydown")
          void navigator.clipboard
            .readText()
            .then((s) => t.paste(s))
            .catch((e) => onError?.(e));
        return false;
      }
      if (action) return false; // Shared application shortcuts are handled once by App.
      e.stopPropagation(); // Ctrl+K/Ctrl+N and Escape belong to the remote shell here.
      return true;
    });
    return () => {
      observer.disconnect();
      unsubscribe();
      data.dispose();
      t.dispose();
      term.current = null;
      fitRef.current = null;
    };
  }, [session.id]);
  useEffect(() => {
    if (term.current) {
      term.current.options.fontSize = fontSize;
      term.current.options.theme = terminalTheme(themeId);
      if (container.current?.clientWidth) fitRef.current?.fit();
    }
  }, [fontSize, themeId]);
  useEffect(() => {
    if (focused && visible) term.current?.focus();
  }, [focused, visible]);
  return (
    <section
      className={"terminal-pane " + (focused ? "focused" : "")}
      style={{ background: terminalTheme(themeId).background }}
      onMouseDown={onFocus}
    >
      <header>
        <span className={"status-dot " + (session.connected ? "live" : "")} />
        <Terminal size={14} />
        <strong>{session.label}</strong>
        <span className="terminal-status" title={session.status}>
          {session.status}
        </span>
        <button
          className="icon-btn"
          title={`Search terminal (${shortcutLabel("find")})`}
          onClick={() => setSearch(!search)}
        >
          <Search size={14} />
        </button>
        <button
          className="icon-btn"
          title="Copy terminal selection"
          onClick={() => {
            const t = term.current;
            if (t?.hasSelection())
              void navigator.clipboard
                .writeText(t.getSelection())
                .catch((e) => onError?.(e));
          }}
        >
          <Copy size={14} />
        </button>
        <button
          className="icon-btn"
          title={`Paste (${shortcutLabel("paste")})`}
          onClick={() =>
            void navigator.clipboard
              .readText()
              .then((s) => term.current?.paste(s))
              .catch((e) => onError?.(e))
          }
        >
          <ClipboardPaste size={14} />
        </button>
        <button
          className="icon-btn"
          title="Clear screen"
          onClick={() => term.current?.clear()}
        >
          <Trash2 size={14} />
        </button>
        <label
          className="terminal-stats-toggle"
          title="Show CPU, memory and mounted filesystems"
        >
          <input
            type="checkbox"
            aria-label="Show resource monitor"
            checked={session.statsEnabled !== false}
            onChange={(e) => onStats?.(e.target.checked)}
          />
          Stats
        </label>
        <button className="icon-btn" title="Close terminal" onClick={onClose}>
          <X size={15} />
        </button>
      </header>
      <ResourceMonitor
        sessionId={session.id}
        enabled={session.statsEnabled !== false}
        visible={visible}
        connected={session.connected}
      />
      {search && (
        <div className="terminal-search">
          <input
            autoFocus
            placeholder="Find in terminal…"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              finder.current?.findNext(e.target.value);
            }}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") finder.current?.findNext(query);
              if (e.key === "Escape") setSearch(false);
            }}
          />
          <button
            className="text-btn"
            onClick={() => finder.current?.findPrevious(query)}
          >
            Previous
          </button>
          <button
            className="text-btn"
            onClick={() => finder.current?.findNext(query)}
          >
            Next
          </button>
          <button className="icon-btn" onClick={() => setSearch(false)}>
            <X size={14} />
          </button>
        </div>
      )}
      <div
        className="terminal-surface"
        ref={container}
        style={{ background: terminalTheme(themeId).background }}
      />
    </section>
  );
}
