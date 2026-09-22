// Synthetic IPC fixture, served only by Vite for browser acceptance tests.
import React from "react";
import { createRoot } from "react-dom/client";
import "../../src/style.css";
const w = window as any;
const callbacks = new Map<number, (e: any) => void>();
const listeners = new Map<number, { event: string; handler: number }>();
let sequence = 0,
  sessions = 0;
const vault = {
  id: "ui-fixture",
  name: "Terminal test vault",
  path: "ui-fixture.ttvault",
  deviceId: "fixture",
  records: (w.recordsFixture ?? []) as any[],
};
w.testVault = vault;
w.calls = [];
w.isTauri = true;
w.inputs = [];
w.clipboardText = "";
w.clipboardReads = 0;
w.delayClipboard = false;
w.emitTerminal = (id: string, text: string) =>
  w.emitEvent("session-event", {
    id,
    kind: "data",
    detail: btoa(String.fromCharCode(...new TextEncoder().encode(text))),
  });
w.emitEvent = (event: string, payload: any) => {
  for (const [id, listener] of listeners)
    if (listener.event === event)
      callbacks.get(listener.handler)?.({ event, id, payload });
};
w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: (event: string, id: number) => listeners.delete(id),
};
w.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main" },
  },
  transformCallback: (fn: (e: any) => void) => {
    const id = ++sequence;
    callbacks.set(id, fn);
    return id;
  },
  unregisterCallback: (id: number) => callbacks.delete(id),
  invoke: async (command: string, args: any = {}) => {
    w.calls.push({ command, args: structuredClone(args) });
    if (w.failNext === command) {
      w.failNext = "";
      throw Error("Fixture: write failed");
    }
    if (command === "records_save" && w.holdWrites)
      await new Promise((r) => (w.releaseWrite = r));
    if (command === "plugin:event|listen") {
      const id = ++sequence;
      listeners.set(id, args);
      return id;
    }
    if (command === "plugin:event|unlisten") {
      listeners.delete(args.eventId);
      return;
    }
    if (command === "app_info")
      return {
        home: "/home/demo",
        version: "0.3.3-dev.1",
        defaultVaultPath: "test.ttvault",
        platform: {
          os: "windows",
          defaultShell: "powershell.exe",
          agentKinds: [],
          rememberStore: "DPAPI",
        },
      };
    if (command === "vault_info") return vault;
    if (command === "records_save") {
      for (const r of args.records) {
        vault.records = vault.records.filter((v) => v.id !== r.id);
        vault.records.push({ ...r, updatedAt: Date.now() });
      }
      return structuredClone(vault);
    }
    if (command === "records_delete") {
      vault.records = vault.records.filter((r) => !args.ids.includes(r.id));
      return structuredClone(vault);
    }
    if (command === "sftp_connect") return "/srv/demo";
    if (command === "plugin:dialog|save") return "/test/selected.ttbackup";
    if (command === "backup_bundle")
      return { vaults: 1, records: args.ids.length, warnings: [] };
    if (command === "file_list") return [];
    if (command === "session_start") {
      const id = `terminal-${++sessions}`;
      setTimeout(() => {
        w.emitEvent("session-event", { id, kind: "connected" });
        w.emitTerminal(
          id,
          `\x1b[32mdemo@server-${sessions}\x1b[0m:~$ echo hello\r\nCOPY_THIS_TEXT Türkçe ğüşöçıİ\r\n`,
        );
      }, 100);
      return id;
    }
    if (command === "session_input") {
      w.inputs.push(args);
      return;
    }
    if (command === "plugin:clipboard-manager|write_text") {
      w.clipboardText = args.text;
      return;
    }
    if (command === "plugin:clipboard-manager|read_text") {
      w.clipboardReads++;
      if (w.delayClipboard) await new Promise((r) => (w.finishClipboard = r));
      return w.clipboardText;
    }
    if (command === "update_info")
      return {
        version: "0.3.3-dev.1",
        channel: "development",
        enabled: false,
        packageSupported: true,
      };
    if (command === "sync_status") return false;
    return null;
  },
};
const { default: App } = await import("../../src/App");
createRoot(document.getElementById("root")!).render(<App />);
const ready = setInterval(() => {
  if ([...listeners.values()].some((l) => l.event === "vault-changed")) {
    clearInterval(ready);
    w.emitEvent("vault-changed", {});
  }
}, 10);
