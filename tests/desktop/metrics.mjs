import { browser, $, $$, expect } from "@wdio/globals";
import { doubleClick } from './interaction.mjs';
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";
const root = process.cwd(),
  work = path.join(root, ".lab", "metrics-" + Date.now()),
  password = "Disposable-metrics-vault-password";
const record = (kind, data) => ({
  id: crypto.randomUUID(),
  kind,
  data,
  updatedAt: Date.now(),
});
async function invoke(command, args = {}) {
  const r = await browser.executeAsync(
    (command, args, done) =>
      window.__TAURI__.core.invoke(command, args).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) }),
      ),
    command,
    args,
  );
  if (r.error) throw Error(r.error);
  return r.value;
}
const sample = (id) => browser.execute((id) => window.__metrics[id], id);
const output = (id) => browser.execute((id) => window.__output[id] ?? "", id);
let host, id, ssh;
describe("Terminal resource counters and input isolation", () => {
  before(async () => {
    await fs.mkdir(work, { recursive: true });
    ssh = JSON.parse(
      await fs.readFile(path.join(root, ".lab/ssh.json"), "utf8"),
    );
    await browser.setTimeout({ script: 120000 });
    await browser.executeAsync((done) => {
      window.__metrics = {};
      window.__output = {};
      window.__counts = {};
      window.__states = {};
      Promise.all([
        window.__TAURI__.event.listen("session-metrics", ({ payload: s }) => {
          window.__metrics[s.sessionId] = s;
          window.__counts[s.sessionId] =
            (window.__counts[s.sessionId] ?? 0) + 1;
        }),
        window.__TAURI__.event.listen("session-event", ({ payload: e }) => {
          if (e.kind === "data")
            window.__output[e.id] = (
              (window.__output[e.id] ?? "") + atob(e.detail)
            ).slice(-20000);
          else window.__states[e.id] = e.kind;
        }),
      ]).then(() => done(true));
    });
    await $("button*=Create local vault").click();
    await $('input[placeholder="Choose where to save your vault"]').setValue(
      path.join(work, "metrics.ttvault"),
    );
    const pw = await $$("input[type=password]");
    await pw[0].setValue(password);
    await pw[1].setValue(password);
    await $("button=Create vault").click();
    await $("h1=Hosts").waitForDisplayed({ timeout: 120000 });
    host = record("host", {
      label: "Metrics fixture",
      address: "127.0.0.1",
      port: 22222,
      protocol: "ssh",
      username: ssh.username,
      password: ssh.password,
    });
    await invoke("records_save", { records: [host] });
    // Refresh UI data without restarting the app and losing the test listeners.
    await browser.executeAsync((done) =>
      window.__TAURI__.event.emit("vault-changed").then(() => done(true)),
    );
  });
  it("reads Linux through a separate SSH channel and pauses hidden/disabled panels", async () => {
    await doubleClick($('.group-main*=Ungrouped'));
    await $("h3=Metrics fixture").waitForDisplayed();
    await $("button=Connect").click();
    await $("button=Trust and connect").waitForDisplayed({ timeout: 30000 });
    await $("button=Trust and connect").click();
    await $(".terminal-status=Connected").waitForDisplayed({ timeout: 30000 });
    await browser.waitUntil(
      async () => {
        const all = await browser.execute(() =>
          Object.values(window.__metrics),
        );
        return all.some(
          (s) =>
            s.os === "linux" &&
            s.cpuPercent !== null &&
            s.disks.some((d) => d.root),
        );
      },
      { timeout: 25000 },
    );
    id = (
      await browser.execute(() =>
        Object.values(window.__metrics).find((s) => s.os === "linux"),
      )
    ).sessionId;
    const s = await sample(id);
    expect(s.memory.total).toBeGreaterThan(0);
    expect(s.disks[0].mountPoint).toBe("/");
    expect(s.error).toBe(null);
    const before = await output(id);
    await browser.pause(4500);
    expect(await output(id)).toBe(before);
    await $(".resource-disk").click();
    await $(
      '[role=dialog][aria-label="Mounted filesystems"]',
    ).waitForDisplayed();
    await $('input[type=checkbox][aria-label="Show resource monitor"]').click();
    expect(await $$(".resource-monitor")).toHaveLength(0);
    await browser.pause(500);
    const count = await browser.execute((id) => window.__counts[id], id);
    await browser.pause(2500);
    expect(await browser.execute((id) => window.__counts[id], id)).toBe(count);
    await $('input[aria-label="Show resource monitor"]').click();
    await browser.waitUntil(
      async () =>
        (await browser.execute((id) => window.__counts[id], id)) > count,
    );
    await $('input[aria-label="Show resource monitor"]').click();
    await $('button[title="Save workspace"]').click();
    await $(".editor").waitForDisplayed();
    await $(".editor input").setValue("Stats saved workspace");
    await $("button=Save").click();
    const v = await invoke("vault_info");
    expect(
      v.records.find((r) => r.kind === "workspace").data.statsEnabled,
    ).toEqual([false]);
    await $('input[aria-label="Show resource monitor"]').click();
    await browser.saveScreenshot(path.join(root, "artifacts/metrics-live.png"));
  });
  it("preserves function keys, Ctrl+K/N and terminal modes as real SSH bytes", async () => {
    const code =
      "import os,tty,termios,select,base64; fd=0; old=termios.tcgetattr(fd); tty.setraw(fd); print('KEY_CAPTURE_READY',flush=True); data=b'';\nwhile select.select([fd],[],[],3)[0]:\n data+=os.read(fd,4096)\ntermios.tcsetattr(fd,termios.TCSADRAIN,old); print('KEY_BYTES:'+base64.b64encode(data).decode(),flush=True)";
    await invoke("session_input", {
      id,
      data: `python3 -c '${code.replaceAll("'", "'\\''")}'\r`,
    });
    await browser.waitUntil(
      async () =>
        (await output(id)).includes("KEY_CAPTURE_READY\n") ||
        (await output(id)).includes("KEY_CAPTURE_READY\r\n"),
    );
    await $(".xterm-helper-textarea").click();
    await browser.execute(() => {
      const el = document.querySelector(".xterm-helper-textarea");
      for (let i = 1; i <= 12; i++)
        el.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "F" + i,
            code: "F" + i,
            keyCode: 111 + i,
            which: 111 + i,
            bubbles: true,
            cancelable: true,
          }),
        );
      for (const [key, code, keyCode, ctrlKey] of [
        ["ArrowUp", "ArrowUp", 38, false],
        ["Home", "Home", 36, false],
        ["End", "End", 35, false],
        ["Insert", "Insert", 45, false],
        ["Delete", "Delete", 46, false],
        ["PageUp", "PageUp", 33, false],
        ["PageDown", "PageDown", 34, false],
        ["Tab", "Tab", 9, false],
        ["Escape", "Escape", 27, false],
        ["k", "KeyK", 75, true],
        ["n", "KeyN", 78, true],
      ])
        el.dispatchEvent(
          new KeyboardEvent("keydown", {
            key,
            code,
            keyCode,
            which: keyCode,
            ctrlKey,
            bubbles: true,
            cancelable: true,
          }),
        );
    });
    await browser.waitUntil(
      async () => /KEY_BYTES:([A-Za-z0-9+/=]+)\r?\n/.test(await output(id)),
      { timeout: 10000 },
    );
    const bytes = Buffer.from(
      (await output(id)).match(/KEY_BYTES:([A-Za-z0-9+/=]+)\r?\n/)[1],
      "base64",
    ).toString("binary");
    expect(bytes).toBe(
      "\x1bOP\x1bOQ\x1bOR\x1bOS\x1b[15~\x1b[17~\x1b[18~\x1b[19~\x1b[20~\x1b[21~\x1b[23~\x1b[24~\x1b[A\x1b[H\x1b[F\x1b[2~\x1b[3~\x1b[5~\x1b[6~\t\x1b\x0b\x0e",
    );
    expect(await $(".terminal-pane").isDisplayed()).toBe(true);
  });
  it("measures the native local system and cleans up on vault lock", async () => {
    const local = await invoke("session_start", { hostId: null, shell: null });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) => window.__states[id] === "connected",
          local,
        ),
    );
    await invoke("session_metrics_set", { sessionId: local, enabled: true });
    await browser.waitUntil(
      async () => {
        const s = await sample(local);
        return (
          s?.cpuPercent !== null &&
          s?.memory?.total > 0 &&
          s?.disks?.some((d) => d.root)
        );
      },
      { timeout: 30000 },
    );
    const s = await sample(local);
    expect(s.os).toBe(
      process.platform === "win32"
        ? "windows"
        : process.platform === "darwin"
          ? "macos"
          : "linux",
    );
    await invoke("session_input", { id: local, close: true });
  });
  it("runs sixteen monitors without terminal output or unbounded event retention", async function () {
    const duration = Number(process.env.TERMTERM_SOAK_SECONDS ?? 30);
    this.timeout((duration + 90) * 1000);
    await $('button[title="Close terminal"]').click();
    const workspace = record("workspace", {
      label: "Sixteen monitor soak",
      hostIds: Array(16).fill(host.id),
      layout: "split",
    });
    await invoke("records_save", { records: [workspace] });
    await browser.executeAsync((done) =>
      window.__TAURI__.event.emit("vault-changed").then(() => done(true)),
    );
    await $("button=Workspaces").click();
    await $("h3=Sixteen monitor soak").waitForDisplayed();
    await browser.execute(() => {
      const card = Array.from(document.querySelectorAll(".record-card")).find(
        (e) => e.textContent.includes("Sixteen monitor soak"),
      );
      Array.from(card.querySelectorAll("button"))
        .find((e) => e.textContent.trim() === "Open")
        .click();
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () =>
            Array.from(document.querySelectorAll(".terminal-status")).filter(
              (e) => e.textContent === "Connected",
            ).length === 16,
        ),
      { timeout: 60000 },
    );
    const ids = await browser.execute(() =>
      Object.entries(window.__states)
        .filter(([, state]) => state === "connected")
        .map(([id]) => id),
    );
    expect(ids).toHaveLength(16);
    await browser.waitUntil(
      async () => {
        const samples = await browser.execute(
          (ids) => ids.map((id) => window.__metrics[id]),
          ids,
        );
        return samples.every((s) => s?.sampledAt && s.cpuPercent !== null);
      },
      { timeout: 20000 },
    );
    const started = Date.now(),
      snapshots = [];
    const baseline = await browser.execute(
      (ids) => ids.map((id) => window.__output[id]),
      ids,
    );
    await browser.saveScreenshot(
      path.join(root, "artifacts/metrics-sixteen.png"),
    );
    console.log("Sixteen-panel monitor soak started for", duration, "seconds");
    while (Date.now() - started < duration * 1000) {
      await browser.pause(
        Math.min(30000, duration * 1000 - (Date.now() - started)),
      );
      const sample = await browser.execute(
        (ids) => ({
          at: Date.now(),
          counts: ids.map((id) => window.__counts[id]),
          ages: ids.map(
            (id) => Date.now() - (window.__metrics[id]?.sampledAt ?? 0),
          ),
          errors: ids.map((id) => window.__metrics[id]?.error),
          retained: Object.keys(window.__metrics).length,
          heap: performance.memory?.usedJSHeapSize ?? null,
          output: ids.map((id) => window.__output[id]),
        }),
        ids,
      );
      snapshots.push(sample);
      expect(sample.output).toEqual(baseline);
      delete sample.output;
      if (
        process.platform === "win32" &&
        process.env.TERMTERM_BINARY?.includes("TermTermMetrics")
      )
        sample.process = JSON.parse(
          execFileSync(
            "powershell.exe",
            [
              "-NoProfile",
              "-Command",
              'Get-Process TermTermMetrics | Select-Object WorkingSet64,PrivateMemorySize64,HandleCount,@{n="Threads";e={$_.Threads.Count}} | ConvertTo-Json -Compress',
            ],
            { encoding: "utf8" },
          ),
        );
      await fs.writeFile(
        path.join(root, "artifacts/metrics-soak-progress.json"),
        JSON.stringify(
          {
            durationSeconds: duration,
            elapsedSeconds: (Date.now() - started) / 1000,
            terminals: 16,
            snapshots,
          },
          null,
          2,
        ),
      );
      expect(sample.ages.every((age) => age < 15000)).toBe(true);
      expect(sample.errors.every((e) => !e)).toBe(true);
    }
    await invoke("vault_lock");
    const counts = await browser.execute(() => ({ ...window.__counts }));
    await browser.pause(3000);
    expect(await browser.execute(() => window.__counts)).toEqual(counts);
    await fs.writeFile(
      path.join(root, "artifacts/metrics-soak.json"),
      JSON.stringify(
        { durationSeconds: duration, terminals: 16, snapshots },
        null,
        2,
      ),
    );
  });
});
