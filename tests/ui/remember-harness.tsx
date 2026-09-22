// Synthetic UI persistence only; native tests separately exercise DPAPI/files.
import React from "react";
import { createRoot } from "react-dom/client";
import "../../src/style.css";
const w = window as any,
  params = new URLSearchParams(location.search);
const vault = {
  id: "remember-fixture",
  name: "Remember test",
  path: "test.ttvault",
  deviceId: "fixture",
  records: [],
};
let active = false,
  id = 0,
  broken = params.has("broken");
const saved = () => localStorage.getItem("fixture-remembered") === "true";
w.isTauri = true;
w.calls = [];
w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
w.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main" },
  },
  transformCallback: () => ++id,
  unregisterCallback: () => {},
  invoke: async (command: string, args: any = {}) => {
    // Never record password arguments, even for disposable UI fixtures.
    w.calls.push({ command, enabled: args.enabled });
    if (command === "app_info")
      return {
        version: "test",
        home: "/",
        defaultVaultPath: "test.ttvault",
        recent: { path: vault.path },
        platform: {
          os: "windows",
          rememberStore: "Windows DPAPI",
          agentKinds: [],
        },
      };
    if (command === "vault_info") {
      if (!active) throw Error("Unlock a vault first");
      return vault;
    }
    if (command === "vault_remember_status") {
      if (broken) throw Error("Secure storage unavailable");
      return saved();
    }
    if (
      command === "vault_try_open_remembered" ||
      command === "vault_open_remembered"
    ) {
      if (broken) throw Error("Saved password cannot be decrypted");
      if (!saved()) return null;
      active = true;
      return vault;
    }
    if (command === "vault_open" || command === "vault_create") {
      if (args.password !== "Disposable-test-password")
        throw Error("Incorrect vault password");
      active = true;
      return vault;
    }
    if (command === "vault_remember") {
      if (params.has("save-failure")) throw Error("Secure storage unavailable");
      localStorage.setItem("fixture-remembered", String(args.enabled));
      broken = false;
      return null;
    }
    if (command === "vault_lock") {
      active = false;
      return null;
    }
    if (command === "plugin:event|listen") return ++id;
    if (command === "sync_status") return false;
    return null;
  },
};
const { default: App } = await import("../../src/App");
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
