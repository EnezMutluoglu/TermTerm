import { useEffect, useRef } from "react";
import { EditorState, Compartment, StateEffect } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  drawSelection,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { autocompletion } from "@codemirror/autocomplete";
import { sql, PostgreSQL, MSSQL, StandardSQL } from "@codemirror/lang-sql";
import { syntaxHighlighting, HighlightStyle } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import type { DatabaseEngine } from "./types";
export default function SqlEditor({
  value,
  cache,
  engine,
  schema,
  onChange,
  onExecute,
}: {
  value: string;
  cache?: { state?: EditorState; scrollTop?: number; scrollLeft?: number };
  engine: DatabaseEngine;
  schema: Record<string, string[]>;
  onChange: (v: string) => void;
  onExecute: (v: string) => void;
}) {
  const element = useRef<HTMLDivElement>(null),
    view = useRef<EditorView | null>(null),
    language = useRef(new Compartment());
  const callbacks = useRef({ onChange, onExecute });
  callbacks.current = { onChange, onExecute };
  const dialect =
    engine === "postgresql"
      ? PostgreSQL
      : engine === "mssql"
        ? MSSQL
        : StandardSQL;
  useEffect(() => {
    if (!element.current) return;
    const extensions = [
      lineNumbers(),
      history(),
      drawSelection(),
      highlightActiveLine(),
      autocompletion(),
      syntaxHighlighting(
        HighlightStyle.define([
          { tag: tags.keyword, color: "#93beee" },
          { tag: tags.string, color: "#dcc58a" },
          { tag: tags.number, color: "#bdcdec" },
          { tag: tags.comment, color: "#7f95ae", fontStyle: "italic" },
          { tag: tags.operator, color: "#c5d4e6" },
          { tag: tags.typeName, color: "#aabfe0" },
        ]),
      ),
      language.current.of(sql({ dialect, schema })),
      keymap.of([
        {
          key: "Mod-Enter",
          run: (v) => {
            const s = v.state.selection.main;
            callbacks.current.onExecute(
              s.empty ? v.state.doc.toString() : v.state.sliceDoc(s.from, s.to),
            );
            return true;
          },
        },
        ...defaultKeymap,
        ...historyKeymap,
      ]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) callbacks.current.onChange(u.state.doc.toString());
      }),
      EditorView.theme(
        {
          "&": {
            height: "100%",
            background: "#10161e",
            color: "#e6edf3",
            fontSize: "14px",
          },
          ".cm-scroller": {
            overflow: "auto",
            fontFamily: "Cascadia Code, Consolas, monospace",
          },
          ".cm-gutters": {
            background: "#131b24",
            color: "#64758b",
            border: "none",
          },
          ".cm-activeLine": { background: "#192432" },
          ".cm-cursor": { borderLeftColor: "#8abfff" },
          ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
            background: "#284464",
          },
          ".cm-content": { padding: "16px 0" },
          ".cm-tooltip": { background: "#202c3b", color: "#e6edf3" },
        },
        { dark: true },
      ),
    ];
    const state = cache?.state
      ? cache.state.update({ effects: StateEffect.reconfigure.of(extensions) })
          .state
      : EditorState.create({ doc: value, extensions });
    const editor = new EditorView({ parent: element.current, state });
    view.current = editor;
    if (cache) {
      editor.scrollDOM.scrollTop = cache.scrollTop ?? 0;
      editor.scrollDOM.scrollLeft = cache.scrollLeft ?? 0;
    }
    return () => {
      if (cache) {
        cache.state = editor.state;
        cache.scrollTop = editor.scrollDOM.scrollTop;
        cache.scrollLeft = editor.scrollDOM.scrollLeft;
      }
      editor.destroy();
      view.current = null;
    };
  }, []);
  useEffect(() => {
    const v = view.current;
    if (v && v.state.doc.toString() !== value)
      v.dispatch({
        changes: { from: 0, to: v.state.doc.length, insert: value },
      });
  }, [value]);
  useEffect(() => {
    view.current?.dispatch({
      effects: language.current.reconfigure(sql({ dialect, schema })),
    });
  }, [engine, schema]);
  return <div className="db-code" ref={element} />;
}
