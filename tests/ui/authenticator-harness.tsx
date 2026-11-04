// Disposable browser mock. Codes use public RFC 6238 examples; no native crypto or user data.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import "../../src/style.css";
const w = window as any;
const mode = new URLSearchParams(location.search).get("mode");
w.isTauri = true;
w.fixture = { calls: [], writes: [], active: 0, maximumActive: 0, hold: false, fail: false, codeAllowed: true, scope: "personal-a", refreshes: 0, revisions: {} };
const fixture = w.fixture;
const publicSeed = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const entries = [
  { id: "rfc", revision: 1, label: "alice@example.test", issuer: "Example", algorithm: "SHA1", digits: 8, period: mode === "expiry" ? 1 : 30, code: "07081804", visibility: "private", permissions: ["connect", "reveal", "export"], canRemove: true, canUpdate: mode === "team" },
  ...(mode === "team" ? [
    { id: "team", revision: 2, label: "operations@example.test", issuer: "Team Lab", algorithm: "SHA256", digits: 6, period: 30, code: "012345", visibility: "shared", permissions: ["connect", "reveal", "export"], canRemove: true, canUpdate: true },
    { id: "denied", revision: 3, label: "limited@example.test", issuer: "Restricted", algorithm: "SHA1", digits: 6, period: 30, code: null, visibility: "shared", permissions: [], canRemove: false, canUpdate: false },
  ] : []),
];
function snapshot(scope: string) {
  const now = Math.floor(Date.now() / 1000);
  return { context: scope, serverTime: now, entries: entries.map((entry) => ({ ...entry, revision: fixture.revisions[entry.id] ?? entry.revision, validUntil: now + entry.period, code: fixture.codeAllowed ? entry.code : null, permissions: fixture.codeAllowed ? entry.permissions : [], label: scope === "personal-b" ? "new-account@example.test" : entry.label })) };
}
w.__TAURI_INTERNALS__ = {
  invoke: async (command: string, args: any) => {
    if (command === "plugin:clipboard-manager|write_text") { fixture.writes.push(args.text); return; }
    if (command !== "authenticator" && command !== "team_v2_action") throw Error("Unexpected fixture command");
    const envelope = command === "team_v2_action" ? args.action : args;
    const action = envelope.action;
    const scope = fixture.scope;
    fixture.calls.push({ command, kind: action.kind, context: envelope.context });
    fixture.active++; fixture.maximumActive = Math.max(fixture.maximumActive, fixture.active);
    try {
      if (fixture.hold) await new Promise<void>((resolve, reject) => { fixture.release = resolve; fixture.reject = reject; });
      if (fixture.fail) throw Error("Unlock a vault first");
      if (envelope.context && envelope.context !== fixture.scope) throw Error("Vault changed; reopen Authenticator");
      if (action.kind === "reveal") return { uri: `otpauth://totp/Example:alice%40example.test?secret=${publicSeed}` };
      if (action.kind === "add") {
        const input = action.input;
        if (input.provisioning.startsWith("otpauth-migration:")) throw Error("Google Authenticator migration QR is not supported");
        if (!input.provisioning.startsWith("otpauth://totp/") && !/^[A-Z2-7 -]{16,256}$/i.test(input.provisioning)) throw Error("Invalid Base32 setup secret");
        fixture.lastAdd = input;
        entries.push({ ...entries[0], id: "added", label: input.label || "qr@example.test", issuer: input.issuer || "QR Example" });
      }
      if (action.kind === "delete") {
        fixture.lastDelete = { id: action.id, revision: action.revision };
        const index = entries.findIndex((entry) => entry.id === action.id);
        if (index >= 0) entries.splice(index, 1);
      }
      fixture.refreshes++;
      return snapshot(scope);
    } finally { fixture.active--; }
  },
};
const { default: Authenticator } = await import("../../src/Authenticator");
function Harness() {
  const [visible, setVisible] = useState(true), [mounted, setMounted] = useState(true), [account, setAccount] = useState("personal-a");
  return <main style={{ height: "100vh" }}><div style={{ padding: 12, background: "var(--surface-recessed)", display: "flex", gap: 16, alignItems: "center" }}><strong>MOCK · Authenticator UI preview</strong><button onClick={() => setVisible((value) => !value)}>Toggle visibility</button><button onClick={() => setMounted(false)}>Unmount fixture</button><button onClick={() => { fixture.scope = "personal-b"; setAccount("personal-b"); }}>Switch account</button></div><div style={{ height: "calc(100vh - 48px)" }}>{mounted && <Authenticator key={account} visible={visible} team={mode === "team"} onShare={(row) => { fixture.shared = row.id; }} onExport={(row) => { fixture.exported = row.id; }} onRecover={(row) => { fixture.recovered = row.id; }} />}</div></main>;
}
createRoot(document.getElementById("root")!).render(<Harness />);
