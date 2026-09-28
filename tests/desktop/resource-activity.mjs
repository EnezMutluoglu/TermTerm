import { browser, $, expect } from "@wdio/globals";
import { doubleClick } from "./interaction.mjs";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
const root = process.cwd(),
  work = path.join(root, ".lab", "activity-" + Date.now());
const record = (kind, data) => ({
  id: crypto.randomUUID(),
  kind,
  data,
  updatedAt: Date.now(),
});
async function invoke(cmd, args = {}) {
  const r = await browser.executeAsync(
    (cmd, args, done) =>
      window.__TAURI__.core.invoke(cmd, args).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) }),
      ),
    cmd,
    args,
  );
  if (r.error) throw Error(r.error);
  return r.value;
}
const sample = (id) => browser.execute((id) => window.__activity[id], id);
const output = (id) =>
  browser.execute((id) => window.__activityOutput[id] ?? "", id);
let linuxId, localId;
describe("Network and disk throughput through real sessions", () => {
  before(async () => {
    await fs.mkdir(work, { recursive: true });
    await browser.setTimeout({ script: 120000 });
    expect((await invoke("app_info")).defaultVaultPath).toContain(".e2e");
    await browser.executeAsync((done) => {
      window.__activity = {};
      window.__activityOutput = {};
      window.__activityHistory = {};
      Promise.all([
        window.__TAURI__.event.listen("session-metrics", ({ payload: s }) => {
          window.__activity[s.sessionId] = s;
          (window.__activityHistory[s.sessionId] ??= []).push(s);
        }),
        window.__TAURI__.event.listen("session-event", ({ payload: e }) => {
          if (e.kind === "data")
            window.__activityOutput[e.id] =
              (window.__activityOutput[e.id] ?? "") + atob(e.detail);
        }),
      ]).then(() => done());
    });
    await invoke("vault_create", {
      path: path.join(work, "activity.ttvault"),
      name: "Kaynak göstergesi testi",
      password: "Disposable-activity-test-2026",
    });
    const ssh = JSON.parse(
      await fs.readFile(path.join(root, ".lab/ssh.json"), "utf8"),
    );
    const jump = record("host", {
      label: "Lab jump",
      address: "127.0.0.1",
      port: 22222,
      username: ssh.username,
      password: ssh.password,
      protocol: "ssh",
    });
    const host = record("host", {
      ...jump.data,
      label: "Linux I/O lab",
      chain: [jump.id],
    });
    await invoke("records_save", {
      records: [
        jump,
        host,
        record("knownHost", {
          address: "[127.0.0.1]:22222",
          publicKey: ssh.publicKey,
        }),
      ],
    });
    await browser.executeAsync((done) =>
      window.__TAURI__.event.emit("vault-changed").then(() => done()),
    );
    await $("h1=Hosts").waitForDisplayed();
    await $(".group-main*=Ungrouped").click();
  });
  it("reads Linux chain throughput, captures real writes and pauses cleanly", async () => {
    await doubleClick($(".record-card*=Linux I/O lab"));
    await $(".terminal-status=Connected").waitForDisplayed({ timeout: 45000 });
    linuxId = await $(".terminal-pane.focused").getAttribute("data-session-id");
    await browser.waitUntil(
      async () => {
        const s = await sample(linuxId);
        return (
          s?.activity?.disks.some((d) => d.readBytesPerSecond !== null) &&
          s?.activity?.network.some((n) => n.readBytesPerSecond !== null)
        );
      },
      { timeout: 30000 },
    );
    let s = await sample(linuxId);
    expect(s.activityError).toBe(null);
    expect(s.activity.diskError).toBe(null);
    expect(s.activity.networkError).toBe(null);
    const before = await output(linuxId);
    await browser.pause(2500);
    expect(await output(linuxId)).toBe(before);
    const name = ".termterm-io-" + crypto.randomUUID();
    await invoke("session_input", {
      id: linuxId,
      data: `dd if=/dev/zero of="$HOME/${name}" bs=1M count=8 conv=fsync status=none && dd if="$HOME/${name}" of=/dev/null bs=1M iflag=direct status=none; rm -f -- "$HOME/${name}"; printf '\nIO_FIXTURE_DONE\n'\r`,
    });
    await browser.waitUntil(
      async () => (await output(linuxId)).includes("IO_FIXTURE_DONE\r\n"),
      { timeout: 10000 },
    );
    await browser.waitUntil(
      async () =>
        (await sample(linuxId))?.activity?.disks.some(
          (d) => d.writeBytesPerSecond > 0,
        ),
      { timeout: 15000 },
    );
    await $('[data-testid="network-throughput"]').click();
    await $('[aria-label="Network and disk activity"]').waitForDisplayed();
    await browser.saveScreenshot(
      path.join(root, "artifacts/activity-native-linux.png"),
    );
    await $('[aria-label="Close activity details"]').click();
    await $('[aria-label="Show resource monitor"]').click();
    await browser.pause(600);
    const t = (await sample(linuxId)).activityAt;
    await browser.pause(2500);
    expect((await sample(linuxId)).activityAt).toBe(t);
    await $('[aria-label="Show resource monitor"]').click();
    await browser.waitUntil(
      async () => (await sample(linuxId)).activityAt > t,
      { timeout: 15000 },
    );
  });
  it("reads actual Windows network and physical-disk rates in a local terminal", async () => {
    await $('[title="New local terminal"]').click();
    localId = await $(".terminal-pane.focused").getAttribute("data-session-id");
    await browser.waitUntil(
      async () => {
        const s = await sample(localId);
        return (
          s?.activity?.disks.some((d) => d.readBytesPerSecond !== null) &&
          s?.activity?.network.some((n) => n.readBytesPerSecond !== null)
        );
      },
      { timeout: 60000 },
    );
    const s = await sample(localId);
    expect(s.os).toBe("windows");
    expect(s.activityError).toBe(null);
    expect(s.activity.diskError).toBe(null);
    expect(s.activity.networkError).toBe(null);
    await $('.terminal-pane.focused [data-testid="disk-throughput"]').click();
    await browser.saveScreenshot(
      path.join(root, "artifacts/activity-native-windows.png"),
    );
    await invoke("session_input", { id: localId, close: true });
    await invoke("session_input", { id: linuxId, close: true });
  });
});
