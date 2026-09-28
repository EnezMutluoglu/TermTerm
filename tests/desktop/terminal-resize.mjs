import { browser, $, expect } from "@wdio/globals";
import { doubleClick } from "./interaction.mjs";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
const root = process.cwd(),
  work = path.join(root, ".lab", "resize-" + Date.now());
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
const output = (id) =>
  browser.execute((id) => window.__resizeOutput[id] ?? "", id);
let cell;
const expected = (id) =>
  browser.execute(
    (id, cell) => {
      const r = document
        .querySelector(`[data-session-id="${id}"] .xterm-screen`)
        .getBoundingClientRect();
      return {
        cols: Math.round(r.width / cell.width),
        rows: Math.round(r.height / cell.height),
      };
    },
    id,
    cell,
  );
async function calibrate(id) {
  // Ask the real xterm instance for its grid via the standard cursor-position report at the clamped bottom-right cell.
  const tag = "GRID" + Date.now();
  const py = `import os,termios,tty,select,re; f=0; old=termios.tcgetattr(f); tty.setraw(f); os.write(1,bytes([27])+b"7"+bytes([27])+b"[9999;9999H"+bytes([27])+b"[6n"+bytes([27])+b"8"); b=b""
try:
 while not b.endswith(b"R"):
  if not select.select([f],[],[],5)[0]: raise Exception("xterm size report timed out")
  b+=os.read(f,1)
finally: termios.tcsetattr(f,termios.TCSADRAIN,old)
m=re.search(rb"\\[(\\d+);(\\d+)R",b); print("${tag}",int(m[1]),int(m[2]))`;
  await input(
    id,
    `python3 -c 'exec(__import__("base64").b64decode("${Buffer.from(py).toString("base64")}"))'\r`,
  );
  const re = new RegExp(`${tag} (\\d+) (\\d+)\\r?\\n`);
  await browser.waitUntil(async () => re.test(await output(id)), {
    timeout: 10000,
  });
  const m = re.exec(await output(id));
  const rect = await browser.execute((id) => {
    const r = document
      .querySelector(`[data-session-id="${id}"] .xterm-screen`)
      .getBoundingClientRect();
    return { width: r.width, height: r.height };
  }, id);
  cell = {
    width: rect.width / Number(m[2]),
    height: rect.height / Number(m[1]),
  };
}
const input = (id, data) => invoke("session_input", { id, data });
async function measured(id) {
  const tag = crypto.randomUUID().replaceAll("-", "");
  await input(id, `printf '\\137\\137SIZE_${tag}__'; stty size\r`);
  const regex = new RegExp(`__SIZE_${tag}__(\\d+) (\\d+)\\r?\\n`);
  await browser.waitUntil(async () => regex.test(await output(id)), {
    timeout: 10000,
  });
  const result = regex.exec(await output(id));
  return { rows: Number(result[1]), cols: Number(result[2]) };
}
async function assertGeometry(id) {
  await browser.pause(300);
  await browser.waitUntil(async () => !!(await expected(id))?.cols, {
    timeout: 10000,
  });
  await expect(await measured(id)).toEqual(await expected(id));
  const unused = await browser.execute((id) => {
    const el = document.querySelector(
        `[data-session-id="${id}"] .terminal-surface`,
      ),
      screen = el.querySelector(".xterm-screen");
    const r = el.getBoundingClientRect(),
      s = screen.getBoundingClientRect(),
      css = getComputedStyle(el);
    return {
      height:
        r.height -
        parseFloat(css.paddingTop) -
        parseFloat(css.paddingBottom) -
        s.height,
      width:
        r.width -
        parseFloat(css.paddingLeft) -
        parseFloat(css.paddingRight) -
        s.width,
    };
  }, id);
  expect(unused.height).toBeLessThan(27);
  expect(unused.width).toBeLessThan(35);
}
let direct,
  chain,
  id,
  failed = false;
describe("Real SSH terminal resize regression", () => {
  before(async () => {
    await fs.mkdir(work, { recursive: true });
    await browser.setTimeout({ script: 120000 });
    expect((await invoke("app_info")).defaultVaultPath).toContain(".e2e");
    await browser.executeAsync((done) => {
      window.__resizeOutput = {};
      window.__TAURI__.event
        .listen("session-event", ({ payload: e }) => {
          if (e.kind === "data")
            window.__resizeOutput[e.id] =
              (window.__resizeOutput[e.id] ?? "") + atob(e.detail);
        })
        .then(() => done());
    });
    const ssh = JSON.parse(
      await fs.readFile(path.join(root, ".lab/ssh.json"), "utf8"),
    );
    await invoke("vault_create", {
      path: path.join(work, "resize.ttvault"),
      name: "Resize regression lab",
      password: "Disposable-resize-vault-2026",
    });
    direct = record("host", {
      label: "Resize direct SSH",
      address: "127.0.0.1",
      port: 22222,
      username: ssh.username,
      password: ssh.password,
      privateKey: ssh.privateKey,
      protocol: "ssh",
      startup: "printf '\\137\\137START_SIZE__'; stty size",
    });
    chain = record("host", {
      ...direct.data,
      label: "Resize chain SSH",
      chain: [direct.id],
    });
    await invoke("records_save", {
      records: [
        direct,
        chain,
        record("knownHost", {
          label: "Lab key",
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
  beforeEach(function () {
    if (failed) this.skip();
  });
  afterEach(async function () {
    failed = this.currentTest.state === "failed";
    if (this.currentTest.state === "failed")
      console.log("Resize test output:", (await output(id)).slice(-3500));
  });
  it("matches the real remote PTY from first connection through grow/shrink and Stats", async () => {
    await browser.setWindowSize(1100, 740);
    await doubleClick($(".record-card*=Resize direct SSH"));
    await $(".terminal-status=Connected").waitForDisplayed({ timeout: 45000 });
    id = await $(".terminal-pane.focused").getAttribute("data-session-id");
    await calibrate(id);
    await assertGeometry(id);
    const small = await expected(id);
    // Startup executes with the dimensions received while SSH authentication was pending.
    await browser.waitUntil(
      async () => /__START_SIZE__\d+ \d+/.test(await output(id)),
      { timeout: 10000 },
    );
    const start = /__START_SIZE__(\d+) (\d+)/.exec(await output(id));
    expect({ rows: Number(start[1]), cols: Number(start[2]) }).toEqual(small);
    await browser.setWindowSize(1850, 1000);
    await browser.waitUntil(
      async () => (await expected(id)).cols > small.cols,
      { timeout: 10000 },
    );
    await assertGeometry(id);
    await $('[aria-label="Show resource monitor"]').click();
    await assertGeometry(id);
    await browser.setWindowSize(1200, 780);
    await browser.waitUntil(async () => (await expected(id)).cols < 150, {
      timeout: 10000,
    });
    await assertGeometry(id);
  });
  it("Vim adopts the maximized terminal dimensions", async () => {
    await input(id, "vim -Nu NONE -n\r");
    await browser.waitUntil(async () => (await output(id)).includes("VIM"), {
      timeout: 10000,
    });
    await browser.maximizeWindow();
    await browser.pause(250);
    const vimSize = await expected(id);
    const tag = "VIM" + Date.now();
    await input(id, `:echo "${tag}" &lines &columns "END"\r`);
    await browser.waitUntil(
      async () =>
        new RegExp(`${tag}\\s+(\\d+)\\s+(\\d+)\\s+END`).test(
          (await output(id)).replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, ""),
        ),
      { timeout: 10000 },
    );
    const match = new RegExp(`${tag}\\s+(\\d+)\\s+(\\d+)\\s+END`).exec(
      (await output(id)).replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, ""),
    );
    expect({ rows: Number(match[1]), cols: Number(match[2]) }).toEqual(vimSize);
    await browser.saveScreenshot(path.join(root, "artifacts/resize-vim.png"));
    await input(id, ":q!\r");
    await assertGeometry(id);
  });
  it("pg_activity stays full size when the terminal is resized", async () => {
    await input(
      id,
      "PGPASSFILE=/var/lib/termterm-ssh/home/.resize-monitor/pgpass PGSSLMODE=verify-full PGSSLROOTCERT=/var/lib/termterm-ssh/home/.resize-monitor/ca.crt pg_activity -h localhost -p 55432 -U termterm_resize_monitor -d termterm_e2e --no-db-size\r",
    );
    await browser.waitUntil(
      async () => (await output(id)).includes("RUNNING QUERIES"),
      {
        timeout: 15000,
      },
    );
    await browser.setWindowSize(1300, 800);
    await browser.waitUntil(
      async () =>
        (await output(id)).includes(`\x1b[${(await expected(id)).rows};1HF1/1`),
      { timeout: 10000 },
    );
    const offset = (await output(id)).length;
    await browser.maximizeWindow();
    await browser.waitUntil(
      async () =>
        (await output(id))
          .slice(offset)
          .includes(`\x1b[${(await expected(id)).rows};1HF1/1`),
      { timeout: 10000 },
    );
    await browser.saveScreenshot(
      path.join(root, "artifacts/resize-pg-activity.png"),
    );
    await input(id, "q");
    await assertGeometry(id);
  });
  it("chain, split and hidden-tab return preserve the correct PTY dimensions", async () => {
    await $("button=Vault").click();
    await doubleClick($(".record-card*=Resize chain SSH"));
    await browser.waitUntil(
      async () =>
        (await $(".terminal-pane.focused .terminal-status").getText()) ===
        "Connected",
      { timeout: 45000 },
    );
    const chainId = await $(".terminal-pane.focused").getAttribute(
      "data-session-id",
    );
    await assertGeometry(chainId);
    const full = await expected(chainId);
    await $('[title="Split terminals"]').click();
    await browser.waitUntil(
      async () => (await expected(chainId)).cols < full.cols,
      { timeout: 10000 },
    );
    await assertGeometry(chainId);
    await assertGeometry(id);
    await $('[title="Focus terminal"]').click();
    await assertGeometry(chainId);
    await $("button=Vault").click();
    await browser.setWindowSize(1400, 900);
    await $(".session-tab-label*=Resize direct").click();
    await assertGeometry(id);
    await browser.saveScreenshot(
      path.join(root, "artifacts/resize-restored.png"),
    );
    await invoke("session_input", { id, close: true });
    await invoke("session_input", { id: chainId, close: true });
  });
});
