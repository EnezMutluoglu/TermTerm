import { browser, $, $$, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";
import { proxy, banner } from "./proxy-fixtures.mjs";
const root = process.cwd();
const work = path.join(root, ".lab", "desktop-" + Date.now());
const password = "Disposable-desktop-test-password";
async function invoke(command, args = {}) {
  const result = await browser.executeAsync(
    (command, args, done) => {
      window.__TAURI__.core.invoke(command, args).then(
        (value) => done({ value }),
        (error) => done({ failure: String(error) }),
      );
    },
    command,
    args,
  );
  if (result.failure) throw new Error(result.failure);
  return result.value;
}
const record = (kind, data) => ({
  id: crypto.randomUUID(),
  kind,
  data,
  updatedAt: Date.now(),
});
let ssh, host, jump;
describe("TermTerm Windows native acceptance", () => {
  before(async () => {
    await fs.mkdir(work, { recursive: true });
    ssh = JSON.parse(
      await fs.readFile(path.join(root, ".lab", "ssh.json"), "utf8"),
    );
    await browser.setTimeout({ script: 120000 });
    await browser.executeAsync((done) => {
      window.__nativeEvents = {};
      window.__TAURI__.event
        .listen("session-event", ({ payload: e }) => {
          const event = window.__nativeEvents[e.id] ?? {
            output: "",
            status: "",
          };
          if (e.kind === "data") {
            const output = atob(e.detail);
            event.output += output;
            if (output.includes("\u001b[6n"))
              void window.__TAURI__.core.invoke("session_input", {
                id: e.id,
                data: "\u001b[1;1R",
              });
          } else event.status = e.kind + ":" + JSON.stringify(e.detail);
          window.__nativeEvents[e.id] = event;
        })
        .then(() => done(true));
    });
  });
  it("creates a real encrypted vault through the desktop UI and saves a host", async () => {
    await $("button*=Create local vault").click();
    await $('input[placeholder="Choose where to save your vault"]').setValue(
      path.join(work, "test.ttvault"),
    );
    const inputs = await $$("input[type=password]");
    await inputs[0].setValue(password);
    await inputs[1].setValue(password);
    await $("button=Create vault").click();
    await $("h1=Hosts").waitForDisplayed({ timeout: 120000 });
    await $("button=New host").click();
    await $('input[placeholder="e.g. Production web server"]').setValue(
      "WSL Integration",
    );
    await $('input[placeholder="Hostname, IP address or device path"]').setValue(
      "127.0.0.1",
    );
    await $("button=Connection").click();
    await $('input[placeholder="22 (SSH) / 23 (Telnet)"]').setValue("22222");
    await $('input[placeholder="Inherited or requested on connect"]').setValue(
      ssh.username,
    );
    await $("input[type=password]").setValue(ssh.password);
    await $("button=Save").click();
    await $("h3=WSL Integration").waitForDisplayed();
    const v = await invoke("vault_info");
    host = v.records.find((r) => r.kind === "host");
    expect(host.data.port).toBe(22222);
    const bytes = await fs.readFile(path.join(work, "test.ttvault"));
    expect(bytes.includes(Buffer.from(ssh.password))).toBe(false);
    expect(bytes.includes(Buffer.from("WSL Integration"))).toBe(false);
    await browser.saveScreenshot(
      path.join(root, "artifacts", "native-hosts.png"),
    );
  });
  it("verifies host identity, connects by password and runs a command", async () => {
    await $("button=Connect").click();
    await $("button=Trust and connect").waitForDisplayed({ timeout: 45000 });
    await $("button=Trust and connect").click();
    await $(".terminal-status=Connected").waitForDisplayed({ timeout: 30000 });
    await browser.execute(() => {
      window.__testOutput = "";
      window.__TAURI__.event.listen("session-event", ({ payload }) => {
        if (payload.kind === "data")
          window.__testOutput += atob(payload.detail);
      });
    });
    const session = await browser.execute(() => window.__testOutput);
    expect(typeof session).toBe("string");
    const terminal = await $(".xterm-helper-textarea");
    await terminal.click();
    await browser.execute((text) => {
      const data = new DataTransfer();
      data.setData("text/plain", text);
      document.querySelector(".xterm-helper-textarea").dispatchEvent(
        new ClipboardEvent("paste", {
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        }),
      );
    }, 'printf "TERMTTERM_NATIVE_SSH_OK\\n"');
    await browser.keys("Enter");
    await browser.waitUntil(
      async () =>
        /[\r\n]TERMTTERM_NATIVE_SSH_OK\r?\n/.test(
          String(await browser.execute(() => window.__testOutput)),
        ),
      { timeout: 20000, timeoutMsg: "SSH output marker missing" },
    );
    await browser.saveScreenshot(
      path.join(root, "artifacts", "native-terminal.png"),
    );
    await $('button[title="Close terminal"]').click();
  });
  it("uses a two-hop chain, public key auth, all tunnel modes, and SFTP byte-preserving transfers", async () => {
    const known = (await invoke("vault_info")).records.find(
      (r) => r.kind === "knownHost",
    );
    expect(known).toBeDefined();
    const identity = record("credential", {
      label: "Test Ed25519",
      username: ssh.username,
      privateKey: ssh.privateKey,
    });
    jump = record("host", {
      ...host.data,
      label: "Jump",
      password: "",
      credentialId: identity.id,
    });
    const chained = record("host", {
      ...host.data,
      label: "Chained destination",
      password: "",
      credentialId: identity.id,
      chain: [jump.id],
    });
    await invoke("records_save", { records: [identity, jump, chained] });
    const id = crypto.randomUUID();
    const home = await invoke("sftp_connect", { id, hostId: chained.id });
    expect(home).toBe(ssh.home);
    const source = path.join(work, "İstanbul-東京.bin");
    const payload = crypto.randomBytes(1024 * 1024 + 123);
    await fs.writeFile(source, payload);
    const remote = home + "/roundtrip-" + crypto.randomUUID() + ".bin";
    await invoke("file_transfer", {
      id: crypto.randomUUID(),
      source: { connection: "local", path: source },
      dest: { connection: id, path: remote },
      overwrite: false,
    });
    const target = path.join(work, "download.bin");
    await invoke("file_transfer", {
      id: crypto.randomUUID(),
      source: { connection: id, path: remote },
      dest: { connection: "local", path: target },
      overwrite: false,
    });
    expect(await fs.readFile(target)).toEqual(payload);
    const list = await invoke("file_list", {
      endpoint: { connection: id, path: home },
    });
    expect(list.some((e) => e.path === remote)).toBe(true);
    await invoke("file_action", {
      endpoint: { connection: id, path: remote },
      action: "chmod",
      permissions: 384,
      target: null,
    });
    for (const mode of ["local", "remote", "dynamic"]) {
      const id = crypto.randomUUID();
      await invoke("tunnel_start", {
        id,
        tunnel: {
          hostId: host.id,
          mode,
          bindAddress: "127.0.0.1",
          bindPort: 0,
          targetAddress: "127.0.0.1",
          targetPort: 22222,
        },
      });
      await invoke("tunnel_stop", { id });
    }
    await invoke("file_action", {
      endpoint: { connection: id, path: remote },
      action: "remove",
      permissions: null,
      target: null,
    });
    await invoke("sftp_disconnect", { id });
  });
  it("authenticates HTTP/SOCKS proxies, verifies actual tunnel traffic, and protects API Bridge access", async () => {
    const original = (await invoke("vault_info")).records.find(
      (r) => r.kind === "knownHost",
    );
    const wrong = await invoke("key_generate");
    await invoke("records_save", {
      records: [
        { ...original, data: { ...original.data, publicKey: wrong.publicKey } },
      ],
    });
    await expect(
      invoke("sftp_connect", { id: crypto.randomUUID(), hostId: host.id }),
    ).rejects.toThrow(/HOST KEY CHANGED/);
    const salt = crypto.randomBytes(20);
    const hashed =
      "|1|" +
      salt.toString("base64") +
      "|" +
      crypto
        .createHmac("sha1", salt)
        .update("[127.0.0.1]:22222")
        .digest("base64");
    await invoke("records_save", {
      records: [{ ...original, data: { ...original.data, address: hashed } }],
    });
    for (const kind of ["http", "socks5"]) {
      const server = await proxy(kind);
      try {
        const proxied = record("host", {
          ...host.data,
          label: kind + " proxy test",
          proxy: {
            kind,
            host: "127.0.0.1",
            port: server.port,
            username: "test",
            password: "proxy-pass",
          },
        });
        await invoke("records_save", { records: [proxied] });
        const id = crypto.randomUUID();
        expect(await invoke("sftp_connect", { id, hostId: proxied.id })).toBe(
          ssh.home,
        );
        await invoke("sftp_disconnect", { id });
      } finally {
        server.close();
      }
    }
    for (const [mode, port] of [
      ["local", 22441],
      ["dynamic", 22442],
      ["remote", 22443],
    ]) {
      const id = crypto.randomUUID();
      try {
        await invoke("tunnel_start", {
          id,
          tunnel: {
            hostId: host.id,
            mode,
            bindAddress: "127.0.0.1",
            bindPort: port,
            targetAddress: "127.0.0.1",
            targetPort: 22222,
          },
        });
        let forwarded = "";
        await browser.waitUntil(
          async () => {
            try {
              forwarded =
                mode === "remote"
                  ? execFileSync(
                      "wsl.exe",
                      [
                        "-d",
                        "Ubuntu-24.04",
                        "--",
                        "python3",
                        "-c",
                        "import socket; s=socket.create_connection(('127.0.0.1',22443),3); print(s.recv(512).decode()); s.close()",
                      ],
                      { encoding: "utf8", timeout: 5000 },
                    )
                  : await banner(port, mode === "dynamic");
              return forwarded.startsWith("SSH-2.0-");
            } catch {
              return false;
            }
          },
          {
            timeout: 15000,
            timeoutMsg: mode + " tunnel did not forward the SSH banner",
          },
        );
        expect(forwarded).toMatch(/^SSH-2.0-/);
      } finally {
        await invoke("tunnel_stop", { id });
      }
    }
    const bridge = await invoke("api_bridge", { enabled: true });
    const headers = {
      Authorization: "Bearer " + bridge.token,
      "Content-Type": "application/json",
    };
    const unauthorized = await fetch(bridge.url + "/v1/hosts");
    expect(unauthorized.ok).toBe(false);
    const invalid = await fetch(bridge.url + "/v1/hosts", {
      method: "POST",
      headers,
      body: JSON.stringify({
        label: "No commands",
        address: "localhost",
        startup: "must-not-run",
      }),
    });
    expect(invalid.ok).toBe(false);
    const response = await fetch(bridge.url + "/v1/hosts", {
      method: "POST",
      headers,
      body: JSON.stringify({
        label: "Bridge host",
        address: "192.0.2.10",
        username: "example",
      }),
    });
    expect(response.ok).toBe(true);
    const listed = await (
      await fetch(bridge.url + "/v1/hosts", { headers })
    ).json();
    expect(listed.hosts.some((h) => h.label === "Bridge host")).toBe(true);
    expect(JSON.stringify(listed)).not.toContain(ssh.password);
    const origin = await fetch(bridge.url + "/v1/hosts", {
      headers: { ...headers, Origin: "https://example.com" },
    });
    expect(origin.ok).toBe(false);
    await invoke("api_bridge", { enabled: false });
  });
  it("runs local ConPTY and shares a real SSH session with exclusive editor control", async () => {
    const local = await invoke("session_start", {
      hostId: null,
      shell: "powershell.exe",
    });
    await browser.waitUntil(
      async () =>
        String(
          await browser.execute(
            (id) => window.__nativeEvents[id]?.status,
            local,
          ),
        ).startsWith("connected"),
      { timeout: 30000 },
    );
    await invoke("session_input", {
      id: local,
      data: "Write-Output ('LOCAL_' + 'CONPTY_OK')\r",
    });
    await browser.waitUntil(
      async () =>
        String(
          await browser.execute(
            (id) => window.__nativeEvents[id]?.output,
            local,
          ),
        ).includes("LOCAL_CONPTY_OK"),
      { timeout: 20000 },
    );
    await invoke("session_input", { id: local, close: true });
    const lab = JSON.parse(
      await fs.readFile(path.join(root, ".lab", "connection.json"), "utf8"),
    );
    const profile = {
      ...lab,
      database: "termterm_e2e",
      caPath: path.join(root, ".lab", "ca.crt"),
    };
    delete profile.accounts;
    await invoke("sync_upload", { profile });
    await invoke("sync_run", { profile });
    await invoke("team_set", {
      profile,
      username: "termterm_editor",
      role: "editor",
      password: "Shared-terminal-editor-password",
    });
    const editor = {
      ...profile,
      username: "termterm_editor",
      password: lab.accounts.termterm_editor,
    };
    const owner = await invoke("session_start", {
      hostId: host.id,
      shell: null,
    });
    await browser.waitUntil(
      async () =>
        String(
          await browser.execute(
            (id) => window.__nativeEvents[id]?.status,
            owner,
          ),
        ).startsWith("connected"),
      { timeout: 30000 },
    );
    const share = await invoke("share_start", {
      profile,
      localId: owner,
      remoteId: null,
    });
    const guest = await invoke("share_start", {
      profile: editor,
      localId: null,
      remoteId: share.id,
    });
    await invoke("share_control", {
      profile,
      id: share.id,
      writer: "termterm_editor",
      finish: false,
    });
    await browser.waitUntil(async () => {
      const sessions = await invoke("share_list", { profile });
      return sessions.some((s) => s.writer === "termterm_editor");
    });
    // The guest command contains a split marker, so only executed output matches.
    await browser.pause(300);
    await invoke("session_input", {
      id: guest.sessionId,
      data: "printf 'SHARED_%s\\n' 'TERMINAL_OK'\r",
    });
    await browser.waitUntil(
      async () =>
        String(
          await browser.execute(
            (id) => window.__nativeEvents[id]?.output,
            guest.sessionId,
          ),
        ).includes("SHARED_TERMINAL_OK"),
      { timeout: 20000 },
    );
    await expect(
      invoke("session_input", { id: owner, data: "must-not-run\r" }),
    ).rejects.toThrow();
    await invoke("share_control", {
      profile,
      id: share.id,
      writer: profile.username,
      finish: true,
    });
    await invoke("session_input", { id: owner, close: true });
  });
  it("runs the packaged Mosh client over a real WSL UDP session", async () => {
    const ip = execFileSync(
      "wsl.exe",
      ["-d", "Ubuntu-24.04", "--", "hostname", "-I"],
      { encoding: "utf8" },
    )
      .trim()
      .split(/\s+/)[0];
    const mosh = record("host", {
      ...host.data,
      label: "Mosh test",
      protocol: "mosh",
      moshAddress: ip,
    });
    await invoke("records_save", { records: [mosh] });
    const id = await invoke("session_start", { hostId: mosh.id, shell: null });
    try {
      await browser.waitUntil(
        async () =>
          String(
            await browser.execute(
              (id) => window.__nativeEvents[id]?.output,
              id,
            ),
          ).includes("termterm_test@"),
        { timeout: 30000, timeoutMsg: "Mosh prompt did not arrive" },
      );
      await invoke("session_input", {
        id,
        data: "printf 'MOSH_%s\\n' 'UDP_OK'\r",
      });
      await browser.waitUntil(
        async () =>
          String(
            await browser.execute(
              (id) => window.__nativeEvents[id]?.output,
              id,
            ),
          ).includes("MOSH_UDP_OK"),
        { timeout: 15000 },
      );
    } finally {
      await invoke("session_input", { id, close: true }).catch(() => {});
    }
  });
  it("renders sixteen real SSH panels and broadcasts input to all of them", async () => {
    const workspace = record("workspace", {
      label: "Sixteen panel acceptance",
      hostIds: Array(16).fill(host.id),
      layout: "split",
    });
    await invoke("records_save", { records: [workspace] });
    await browser.executeAsync((done) => {
      window.__TAURI__.event.emit("vault-changed", {}).then(() => done(true));
    });
    await $("button=Workspaces").click();
    await $("h3=Sixteen panel acceptance").waitForDisplayed();
    await $("button=Open").click();
    await browser.waitUntil(
      async () =>
        await browser.execute(() => {
          const text = Array.from(
            document.querySelectorAll(".terminal-status"),
            (e) => e.textContent,
          );
          return text.length === 16 && text.every((t) => t === "Connected");
        }),
      { timeout: 60000 },
    );
    await expect(
      invoke("session_start", { hostId: host.id, shell: null }),
    ).rejects.toThrow(/16/);
    await $(
      'button[title="Broadcast input to all connected terminals"]',
    ).click();
    await browser.execute(() => {
      const textarea = document.querySelectorAll(".xterm-helper-textarea")[15];
      textarea.focus();
      const data = new DataTransfer();
      data.setData("text/plain", "printf 'PANELS_%s\\n' 'BROADCAST_OK'");
      textarea.dispatchEvent(
        new ClipboardEvent("paste", {
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    await browser.keys("Enter");
    await browser.waitUntil(
      async () =>
        (await browser.execute(
          () =>
            Object.values(window.__nativeEvents).filter((e) =>
              e.output.includes("PANELS_BROADCAST_OK"),
            ).length,
        )) === 16,
      { timeout: 20000 },
    );
    await browser.saveScreenshot(
      path.join(root, "artifacts/native-sixteen-panels.png"),
    );
    await $(
      'button[title="Broadcast input to all connected terminals"]',
    ).click();
    const buttons = await $$('button[title="Close terminal"]');
    for (const button of buttons) await button.click();
  });
  it("previews every documented plain-text import adapter and CSV mappings", async () => {
    for (const [filename, format] of [
      ["hosts.csv", "csv"],
      ["ssh-config", "openssh"],
      ["putty.reg", "putty"],
      ["mobaxterm.ini", "mobaxterm"],
      ["securecrt.xml", "securecrt"],
      ["inventory.ini", "ansible"],
      ["inventory.yml", "ansible"],
    ]) {
      const source = path.join(root, "templates", filename);
      const before = await fs.readFile(source);
      const preview = await invoke("import_preview", {
        path: source,
        format,
        password: null,
        mapping: null,
      });
      expect(preview.records.some((r) => r.kind === "host")).toBe(true);
      expect(await fs.readFile(source)).toEqual(before);
    }
    const custom = path.join(work, "mapped.csv");
    await fs.writeFile(
      custom,
      "Server title,IP number,Login\nTürkçe,192.0.2.10,ubuntu\n",
    );
    const preview = await invoke("import_preview", {
      path: custom,
      format: "csv",
      password: null,
      mapping: {
        "Server title": "label",
        "IP number": "address",
        Login: "username",
      },
    });
    expect(preview.records[0].data.label).toBe("Türkçe");
  });
  it("automatically sends offline edits after the isolated PostgreSQL cluster restarts", async () => {
    const lab = JSON.parse(
      await fs.readFile(path.join(root, ".lab", "connection.json"), "utf8"),
    );
    const profile = {
      ...lab,
      database: "termterm_e2e",
      caPath: path.join(root, ".lab", "ca.crt"),
    };
    delete profile.accounts;
    await invoke("sync_run", { profile });
    await invoke("sync_watch", { profile, enabled: true });
    const offline = record("snippet", {
      label: "Offline restart acceptance",
      command: "printf offline",
    });
    const cluster = (action) =>
      execFileSync(
        "wsl.exe",
        [
          "-d",
          "Ubuntu-24.04",
          "-u",
          "root",
          "--",
          "pg_ctlcluster",
          "18",
          "termterm",
          action,
        ],
        { timeout: 30000 },
      );
    try {
      cluster("stop");
      const saved = await invoke("records_save", { records: [offline] });
      expect(saved.records.some((r) => r.id === offline.id)).toBe(true);
      await expect(invoke("sync_run", { profile })).rejects.toThrow();
    } finally {
      cluster("start");
    }
    try {
      await browser.waitUntil(
        async () => {
          const count = execFileSync(
            "wsl.exe",
            [
              "-d",
              "Ubuntu-24.04",
              "-u",
              "postgres",
              "--",
              "psql",
              "-X",
              "-p",
              "55432",
              "-d",
              "termterm_e2e",
              "-At",
              "-c",
              `SELECT count(*) FROM termterm.records WHERE id='${offline.id}' AND NOT deleted`,
            ],
            { encoding: "utf8", timeout: 10000 },
          );
          return count.trim() === "1";
        },
        {
          timeout: 45000,
          interval: 1000,
          timeoutMsg: "Background sync did not resume after PostgreSQL restart",
        },
      );
    } finally {
      await invoke("sync_watch", { profile, enabled: false });
    }
  });
  it("commits an active session log before locking its vault", async () => {
    const before = await invoke("vault_info");
    const id = await invoke("session_start", { hostId: host.id, shell: null });
    await browser.waitUntil(
      async () =>
        String(
          await browser.execute((id) => window.__nativeEvents[id]?.status, id),
        ).startsWith("connected"),
      { timeout: 30000 },
    );
    await invoke("session_input", {
      id,
      data: "printf 'LOCK_%s\\n' 'LOG_SAVED'\r",
    });
    await browser.waitUntil(
      async () =>
        String(
          await browser.execute((id) => window.__nativeEvents[id]?.output, id),
        ).includes("LOCK_LOG_SAVED"),
      { timeout: 10000 },
    );
    await invoke("vault_lock");
    const reopened = await invoke("vault_open", {
      path: before.path,
      password,
    });
    expect(
      reopened.records.some(
        (r) =>
          r.kind === "log" &&
          r.data.sessionId === id &&
          r.data.content.includes("LOCK_LOG_SAVED"),
      ),
    ).toBe(true);
  });
  it("roundtrips portable and backup files and rejects the wrong password without changing the source", async () => {
    const before = await invoke("vault_info");
    const portable = path.join(work, "portable.ttvault"),
      backup = path.join(work, "backup.ttbackup");
    await invoke("vault_copy", { path: portable });
    await invoke("backup_create", {
      path: backup,
      password,
      includeProfiles: true,
      ids: [],
    });
    const preview = await invoke("backup_preview", { path: backup, password });
    expect(preview.vaults[0].records).toEqual(before.records);
    await invoke("vault_lock");
    const original = await fs.readFile(portable);
    await expect(
      invoke("vault_open", { path: portable, password: "incorrect-password" }),
    ).rejects.toThrow();
    expect(await fs.readFile(portable)).toEqual(original);
    const opened = await invoke("vault_open", { path: portable, password });
    expect(opened.records).toEqual(before.records);
    expect(opened.deviceId).not.toBe(before.deviceId);
    const restored = await invoke("backup_restore", {
      source: backup,
      password,
      path: path.join(work, "restored.ttvault"),
      newPassword: password,
      index: 0,
    });
    expect(restored.records.length).toBe(before.records.length);
    expect(restored.id).not.toBe(before.id);
    await invoke("vault_lock");
  });
});
