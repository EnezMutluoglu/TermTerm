import { tr } from "./i18n";
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
import { terminalTheme, terminalFontFamily } from "./terminalThemes";
import { copyText, readClipboard } from "./clipboard";
import "@xterm/xterm/css/xterm.css";
import { call, onSession, terminalHistory } from "./api";
import type { Session } from "./types";
export default function TerminalPane({
  session,
  focused,
  onFocus,
  onClose,
  onInput,
  fontSize = 15,
  themeId = "graphite",
  visible = true,
  onStats,
  onError,
  copyOnSelect = true,
  rightClickPaste = true,
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
  copyOnSelect?: boolean;
  rightClickPaste?: boolean;
}) {
  const container = useRef<HTMLDivElement>(null);
  const term = useRef<XTerm | null>(null);
  const finder = useRef<SearchAddon | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const input = useRef(onInput);
  input.current = onInput;
  const latest = useRef({
    onError,
    onFocus,
    copyOnSelect,
    rightClickPaste,
    closed: session.closed,
  });
  latest.current = {
    onError,
    onFocus,
    copyOnSelect,
    rightClickPaste,
    closed: session.closed,
  };
  function copySelection() {
    const text = term.current?.getSelection();
    if (text) void copyText(text).catch((e) => latest.current.onError?.(e));
  }
  async function paste() {
    const target = term.current;
    if (!target || latest.current.closed) return;
    try {
      const text = await readClipboard();
      if (term.current === target && !latest.current.closed) {
        target.paste(text);
        target.focus();
      }
    } catch (e) {
      latest.current.onError?.(e);
    }
  }
  const [search, setSearch] = useState(false);
  const [query, setQuery] = useState("");
  useEffect(() => {
    if (!container.current) return;
    const t = new XTerm({
      cursorBlink: true,
      fontFamily: terminalFontFamily,
      fontSize,
      lineHeight: 1.25,
      minimumContrastRatio: 4.5,
      fontWeight: 400,
      fontWeightBold: 700,
      rightClickSelectsWord: false,
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
    const resized = t.onResize(({ cols, rows }) => {
      if (!latest.current.closed)
        void call("session_input", { id: session.id, cols, rows }).catch(
          () => {},
        );
    });
    const resize = () => {
      if (container.current && container.current.clientWidth > 0) {
        fit.fit();
      }
    };
    const observer = new ResizeObserver(resize);
    observer.observe(container.current);
    resize();
    const surface = container.current;
    let selecting = false;
    let selectionFrame = 0;
    const beginSelection = (e: PointerEvent) => {
      if (e.button === 0) selecting = true;
    };
    const endSelection = (e: PointerEvent) => {
      if (e.button !== 0 || !selecting) return;
      selecting = false;
      cancelAnimationFrame(selectionFrame);
      selectionFrame = requestAnimationFrame(() => {
        if (latest.current.copyOnSelect) copySelection();
      });
    };
    const interceptRight = (e: MouseEvent) => {
      if (
        e.button === 2 &&
        latest.current.rightClickPaste &&
        (t.modes.mouseTrackingMode === "none" || e.shiftKey)
      ) {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (e.type === "contextmenu") {
          latest.current.onFocus();
          void paste();
        }
      }
    };
    surface.addEventListener("pointerdown", beginSelection);
    window.addEventListener("pointerup", endSelection);
    for (const event of ["mousedown", "mouseup", "contextmenu"])
      surface.addEventListener(event, interceptRight as EventListener, true);
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
        if (e.type === "keydown" && !e.repeat) setSearch((v) => !v);
        return false;
      }
      if (action === "copy") {
        e.stopPropagation();
        e.preventDefault();
        if (e.type === "keydown" && !e.repeat) copySelection();
        return false;
      }
      if (action === "paste") {
        e.stopPropagation();
        e.preventDefault();
        if (e.type === "keydown" && !e.repeat) void paste();
        return false;
      }
      if (action === "scrollUp" || action === "scrollDown") {
        e.stopPropagation();
        e.preventDefault();
        if (e.type === "keydown") t.scrollPages(action === "scrollUp" ? -1 : 1);
        return false;
      }
      if (action) return false; // Shared application shortcuts are handled once by App.
      e.stopPropagation(); // Ctrl+K/Ctrl+N and Escape belong to the remote shell here.
      return true;
    });
    return () => {
      observer.disconnect();
      cancelAnimationFrame(selectionFrame);
      surface.removeEventListener("pointerdown", beginSelection);
      window.removeEventListener("pointerup", endSelection);
      for (const event of ["mousedown", "mouseup", "contextmenu"])
        surface.removeEventListener(
          event,
          interceptRight as EventListener,
          true,
        );
      unsubscribe();
      resized.dispose();
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
    if (!visible) return;
    const frame = requestAnimationFrame(() => {
      if (container.current?.clientWidth) fitRef.current?.fit();
      if (focused && !search) term.current?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [focused, visible, search]);
  return (
    <section
      className={"terminal-pane " + (focused ? "focused" : "")}
      data-session-id={session.id}
      aria-label={`Terminal ${session.label}`}
      style={{ background: terminalTheme(themeId).background }}
      onMouseDown={onFocus}
    >
      <header>
        <span className={"status-dot " + (session.connected ? "live" : "")} />
        <Terminal size={14} />
        <strong>{session.label}</strong>
        <span className="terminal-status" title={tr(session.status ?? "")}>
          {tr(session.status ?? "")}
        </span>
        <button
          className="icon-btn"
          title={tr("Search terminal ({shortcut})", {
            shortcut: shortcutLabel("find"),
          })}
          onClick={() => setSearch(!search)}
        >
          <Search size={14} />
        </button>
        <button
          className="icon-btn"
          title={tr("Copy terminal selection")}
          onClick={copySelection}
        >
          <Copy size={14} />
        </button>
        <button
          className="icon-btn"
          title={tr("Paste ({shortcut})", { shortcut: shortcutLabel("paste") })}
          disabled={session.closed}
          onClick={() => void paste()}
        >
          <ClipboardPaste size={14} />
        </button>
        <button
          className="icon-btn"
          title={tr("Clear screen")}
          onClick={() => term.current?.clear()}
        >
          <Trash2 size={14} />
        </button>
        <label
          className="terminal-stats-toggle"
          title={tr("Show CPU, memory and mounted filesystems")}
        >
          <input
            type="checkbox"
            aria-label={tr("Show resource monitor")}
            checked={session.statsEnabled !== false}
            onChange={(e) => onStats?.(e.target.checked)}
          />
          {tr("Stats")}
        </label>
        <button
          className="icon-btn"
          title={tr("Close terminal")}
          onClick={onClose}
        >
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
            placeholder={tr("Find in terminal…")}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              finder.current?.findNext(e.target.value);
            }}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter")
                e.shiftKey
                  ? finder.current?.findPrevious(query)
                  : finder.current?.findNext(query);
              if (e.key === "Escape") setSearch(false);
            }}
          />
          <button
            className="text-btn"
            onClick={() => finder.current?.findPrevious(query)}
          >
            {tr("Previous")}
          </button>
          <button
            className="text-btn"
            onClick={() => finder.current?.findNext(query)}
          >
            {tr("Next")}
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
