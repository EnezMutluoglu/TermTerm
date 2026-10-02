// Real isolated Team PostgreSQL + Windows native IPC, SSH chain and SFTP.
// Never opens a personal/production vault. Credentials remain in ignored .lab.
import { browser, $, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
const execLab = promisify(execFile);
const root = process.cwd(),
  password = "Disposable-Team-native-1234";
const suffix = Date.now().toString(36),
  work = path.join(root, ".lab", "team-" + suffix);
let profile, ssh, teamId, vaultId, owner, operator, editor, group, host, jump;
const rec = (kind, data) => ({
  id: crypto.randomUUID(),
  kind,
  data,
  updatedAt: Date.now(),
});
async function invoke(command, args = {}) {
  const result = await browser.executeAsync(
    (command, args, done) =>
      window.__TAURI__.core.invoke(command, args).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) }),
      ),
    command,
    args,
  );
  if (result.error) throw Error(result.error);
  return result.value;
}
const request = (action, body = {}) => invoke("team_request", { action, body });
async function login(account, register = false) {
  return invoke("team_auth", {
    profile,
    login: account.username,
    email: account.username + "@example.invalid",
    password,
    register,
  });
}
async function openVault() {
  return invoke("team_open_vault", {
    vaultId,
    name: "İstanbul Altyapısı",
    teamId,
  });
}
async function permission(userId, recordId, permission, effect = "allow") {
  const overview = await request("overview");
  return request("acl_set", {
    teamId,
    vaultId,
    userId,
    recordId,
    permission,
    effect,
    aclRevision: overview.teams.find((t) => t.id === teamId).aclRevision,
  });
}
async function second(action, body = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      "wsl.exe",
      [
        "-d",
        "Ubuntu-24.04",
        "-u",
        "root",
        "--",
        "python3",
        "/mnt/c/Users/sonx/Desktop/termterm/tests/team_rpc.py",
      ],
      { windowsHide: true },
    );
    let out = "",
      error = "";
    child.stdout.on("data", (b) => (out += b));
    child.stderr.on("data", (b) => (error += b));
    child.on("error", reject);
    child.on("close", (code) =>
      code ? reject(Error(error)) : resolve(JSON.parse(out)),
    );
    child.stdin.end(
      JSON.stringify({ login: owner.username, password, action, body }),
    );
  });
}
async function events() {
  await browser.executeAsync((done) => {
    window.__teamEvents = {};
    Promise.all([
      window.__TAURI__.event.listen("session-event", ({ payload: e }) => {
        let state = (window.__teamEvents[e.id] ??= { text: "", status: "" });
        if (e.kind === "data") state.text += atob(e.detail);
        else state.status = e.kind;
      }),
      window.__TAURI__.event.listen("session-prompt", ({ payload: p }) => {
        if (p.kind === "hostKey" && p.detail.address.includes("127.0.0.1"))
          void window.__TAURI__.core.invoke("prompt_answer", {
            id: p.id,
            answers: ["trust"],
          });
      }),
    ]).then(() => done());
  });
}
describe("TermTerm Team native acceptance", () => {
  before(async () => {
    await fs.mkdir(work, { recursive: true });
    await browser.setTimeout({ script: 120000 });
    const info = await invoke("app_info");
    if (!info.defaultVaultPath.includes("teamdev.e2e"))
      throw Error("Refusing non-isolated application identity");
    profile = JSON.parse(
      await fs.readFile((process.env.TERMTERM_TEAM_PROFILE ?? path.join(root, ".lab/team-windows.json")), "utf8"),
    );
    profile.database = "termterm_team_e2e";
    ssh = JSON.parse(
      await fs.readFile(path.join(root, ".lab/ssh.json"), "utf8"),
    );
    owner = { username: "owner_" + suffix };
    operator = { username: "operator_" + suffix };
    editor = { username: "editor_" + suffix };
    editor.id = (await login(editor, true)).user.id;
    await invoke("team_logout");
    const initial = await login(operator, true);
    operator.id = initial.user.id;
    await invoke("team_logout");
    const status = await login(owner, true);
    owner.id = status.user.id;
  });
  it("creates a real team, encrypted shared vault and host chain", async () => {
    teamId = (await request("create_team", { name: "İstanbul Platform Ekibi" }))
      .id;
    vaultId = (
      await request("create_vault", { teamId, name: "İstanbul Altyapısı" })
    ).id;
    await openVault();
    group = rec("group", { label: "Üretim", groupId: "" });
    jump = rec("host", {
      label: "Lab Jump",
      address: ssh.address,
      port: ssh.port,
      username: ssh.username,
      privateKey: ssh.privateKey,
      protocol: "ssh",
      groupId: "",
    });
    host = rec("host", {
      label: "Linux Lab · Chain",
      address: ssh.address,
      port: ssh.port,
      username: ssh.username,
      privateKey: ssh.privateKey,
      protocol: "ssh",
      groupId: group.id,
      chain: [jump.id],
    });
    await invoke("records_save", { records: [group, jump, host] });
    await request("sync");
    const vault = await invoke("vault_info");
    expect(vault.records.filter((r) => r.kind === "host")).toHaveLength(2);
    await request("member_set", {
      teamId,
      userId: operator.id,
      role: "operator",
      active: true,
    });
    for (const p of ["read", "connect"])
      await permission(operator.id, host.id, p);
    await browser.refresh();
    await $(".team-workspace").waitForDisplayed({ timeout: 30000 });
    await browser.saveScreenshot(
      path.join(root, "artifacts", "team-vaults.png"),
    );
    await $("button=Üyeler ve yetkiler").click();
    await $(".team-matrix").waitForDisplayed();
    await browser.waitUntil(
      async () =>
        await $("body")
          .getText()
          .then((t) => t.includes(operator.username)),
    );
    await browser.saveScreenshot(
      path.join(root, "artifacts", "team-permissions.png"),
    );
  });
  it("operator sees only allowed host, cannot export/edit/jump independently, can use SSH chain and SFTP", async () => {
    await invoke("team_logout");
    await login(operator);
    await openVault();
    let vault = await invoke("vault_info");
    const hosts = vault.records.filter((r) => r.kind === "host");
    expect(hosts.map((h) => h.id)).toEqual([host.id]);
    expect(hosts[0].data.privateKey).toBeUndefined();
    expect(
      vault.records.find((r) => r.id === group.id).data._teamPathOnly,
    ).toBe(true);
    await expect(
      invoke("export_preview", { format: "csv", secrets: false }),
    ).rejects.toThrow();
    await expect(
      invoke("records_save", {
        records: [{ ...host, data: { ...host.data, label: "Forbidden" } }],
      }),
    ).rejects.toThrow();
    await events();
    const denied = await invoke("session_start", {
      hostId: jump.id,
      shell: null,
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) => window.__teamEvents[id]?.status === "closed",
          denied,
        ),
    );
    const session = await invoke("session_start", {
      hostId: host.id,
      shell: null,
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) => window.__teamEvents[id]?.status === "connected",
          session,
        ),
      { timeout: 45000 },
    );
    await invoke("session_input", {
      id: session,
      data: "printf '__TEAM_CHAIN_OK__\\n'\r",
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) => window.__teamEvents[id]?.text.includes("__TEAM_CHAIN_OK__"),
          session,
        ),
    );
    const sftpId = crypto.randomUUID();
    await invoke("sftp_connect", { id: sftpId, hostId: host.id });
    const entries = await invoke("file_list", {
      endpoint: { connection: sftpId, path: ssh.home },
    });
    expect(entries.some((e) => e.name === "welcome.txt")).toBe(true);
    const output = path.join(work, "from-lab.txt");
    await invoke("file_transfer", {
      id: crypto.randomUUID(),
      source: { connection: sftpId, path: ssh.home + "/welcome.txt" },
      dest: { connection: "local", path: output },
      overwrite: false,
    });
    expect(await fs.readFile(output, "utf8")).toContain(
      "TermTerm SFTP integration fixture",
    );
    await invoke("sftp_disconnect", { id: sftpId });
    const shared = await invoke("share_start", {
      profile,
      localId: session,
      remoteId: null,
    });
    expect(
      (await invoke("share_list", { profile })).some((s) => s.id === shared.id),
    ).toBe(true);
    await invoke("share_control", {
      profile,
      id: shared.id,
      writer: owner.username,
      finish: false,
    });
    await invoke("session_input", {
      id: session,
      data: "printf '__FORBIDDEN_SHARE_INPUT__'\r",
    });
    await browser.waitUntil(
      async () =>
        (await invoke("share_list", { profile })).find(
          (s) => s.id === shared.id,
        )?.writer === owner.username,
    );
    await invoke("share_control", {
      profile,
      id: shared.id,
      writer: operator.username,
      finish: false,
    });
    // Let the native poll observe the new lease before submitting fresh input.
    await browser.executeAsync((done) => {
      const un = window.__TAURI__.event.listen(
        "share-event",
        ({ payload: p }) => {
          if (p.writer?.startsWith("operator_")) {
            void un.then((fn) => fn());
            done();
          }
        },
      );
    });
    await invoke("session_input", {
      id: session,
      data: "printf '\\137\\137TEAM_SHARE_OK\\137\\137\\n'\r",
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) => window.__teamEvents[id]?.text.includes("__TEAM_SHARE_OK__"),
          session,
        ),
      { timeout: 15000 },
    );
    expect(
      await browser.execute(
        (id) =>
          window.__teamEvents[id]?.text.includes("__FORBIDDEN_SHARE_INPUT__"),
        session,
      ),
    ).toBe(false);
    await invoke("share_control", {
      profile,
      id: shared.id,
      writer: operator.username,
      finish: true,
    });
    const bridge = await invoke("api_bridge", { enabled: true });
    const bridgeReply = await fetch(bridge.url + "/v1/hosts", {
      headers: { Authorization: "Bearer " + bridge.token },
    });
    expect(bridgeReply.status).toBe(400);
    await invoke("api_bridge", { enabled: false });
    await second("member_set", {
      teamId,
      userId: operator.id,
      role: "operator",
      active: false,
    });
    await expect(request("sync")).rejects.toThrow();
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) => window.__teamEvents[id]?.status === "closed",
          session,
        ),
      { timeout: 10000 },
    );
    expect(
      (await invoke("vault_info")).records.filter((r) =>
        ["host", "group", "credential"].includes(r.kind),
      ),
    ).toHaveLength(0);
    await expect(invoke("records_save", { records: [host] })).rejects.toThrow();
  });
  it("routes revoked editor drafts to a real decision and merges without exposing secrets", async () => {
    await invoke("team_logout");
    await login(owner);
    await openVault();
    await request("member_set", {
      teamId,
      userId: editor.id,
      role: "editor",
      active: true,
    });
    for (const p of ["read", "edit"]) await permission(editor.id, host.id, p);
    await invoke("team_logout");
    await login(editor);
    await openVault();
    const original = (await invoke("vault_info")).records.find(
      (r) => r.id === host.id,
    );
    await invoke("records_save", {
      records: [
        {
          ...original,
          data: { ...original.data, label: "Editor offline candidate" },
        },
      ],
    });
    const overview = await second("overview");
    await second("acl_set", {
      vaultId,
      userId: editor.id,
      recordId: host.id,
      permission: "edit",
      effect: "deny",
      aclRevision: overview.teams.find((t) => t.id === teamId).aclRevision,
    });
    await request("sync");
    expect((await invoke("team_status")).pending).toBe(0);
    await invoke("team_logout");
    await login(owner);
    await openVault();
    let conflicts = await request("conflicts", { vaultId });
    expect(conflicts.length).toBeGreaterThan(0);
    const candidate = conflicts.find((c) => c.recordId === host.id);
    expect(candidate.candidateRecord.data.privateKey).toBeUndefined();
    await browser.refresh();
    await $(".team-workspace").waitForDisplayed();
    await $("button=Bekleyen kararlar").click();
    await $(".team-conflict").waitForDisplayed();
    await browser.saveScreenshot(
      path.join(root, "artifacts", "team-decisions.png"),
    );
    const current = (await invoke("vault_info")).records.find(
      (r) => r.id === host.id,
    );
    await invoke("team_decide", {
      conflictId: candidate.id,
      currentRevision: current.data._teamRevision,
      choice: "merge",
      merged: {
        ...candidate.candidateRecord,
        data: { ...candidate.candidateRecord.data, label: "Reviewed merge" },
      },
    });
    expect(
      (await invoke("vault_info")).records.find((r) => r.id === host.id).data
        .label,
    ).toBe("Reviewed merge");
    expect(
      (await request("conflicts", { vaultId })).some(
        (c) => c.id === candidate.id,
      ),
    ).toBe(false);
  });
  it("keeps edits and SSH usable while the real lab PostgreSQL is stopped, then uploads", async () => {
    const control = async (action) =>
      execLab(
        "wsl.exe",
        [
          "-d",
          "Ubuntu-24.04",
          "-u",
          "root",
          "--",
          "pg_ctlcluster",
          "--skip-systemctl-redirect",
          "18",
          "termterm",
          action,
        ],
        { windowsHide: true },
      );
    const cfg = await execLab(
      "wsl.exe",
      [
        "-d",
        "Ubuntu-24.04",
        "-u",
        "root",
        "--",
        "pg_conftool",
        "18",
        "termterm",
        "show",
        "port",
      ],
      { windowsHide: true },
    );
    if (!cfg.stdout.includes("55432"))
      throw Error("Refusing to stop a non-lab PostgreSQL cluster");
    let session;
    try {
      await control("stop");
      await expect(request("sync")).rejects.toThrow();
      expect((await invoke("team_status")).online).toBe(false);
      const original = (await invoke("vault_info")).records.find(
        (r) => r.id === host.id,
      );
      await invoke("records_save", {
        records: [
          {
            ...original,
            data: { ...original.data, label: "Offline retained edit" },
          },
        ],
      });
      expect((await invoke("team_status")).pending).toBe(1);
      await events();
      session = await invoke("session_start", { hostId: host.id, shell: null });
      await browser.waitUntil(
        async () =>
          await browser.execute(
            (id) => window.__teamEvents[id]?.status === "connected",
            session,
          ),
        { timeout: 45000 },
      );
      await invoke("session_input", {
        id: session,
        data: "printf '\\137\\137OFFLINE_SSH_OK\\137\\137\\n'\r",
      });
      await browser.waitUntil(
        async () =>
          await browser.execute(
            (id) =>
              window.__teamEvents[id]?.text.includes("__OFFLINE_SSH_OK__"),
            session,
          ),
        { timeout: 10000 },
      );
      await invoke("session_input", { id: session, close: true });
      session = undefined;
    } finally {
      await control("start");
      if (session)
        await invoke("session_input", { id: session, close: true }).catch(
          () => {},
        );
    }
    await request("sync");
    expect((await invoke("team_status")).pending).toBe(0);
    await invoke("team_logout");
    await login(owner);
    await openVault();
    expect(
      (await invoke("vault_info")).records.find((r) => r.id === host.id).data
        .label,
    ).toBe("Offline retained edit");
  });
  it("keeps versions and demonstrates temporary use / current view", async () => {
    await invoke("team_logout");
    await login(owner);
    await openVault();
    for (let n = 0; n < 12; n++) {
      let r = (await invoke("vault_info")).records.find(
        (r) => r.id === host.id,
      );
      r.data.label = "Linux Lab · r" + (n + 2);
      await invoke("records_save", { records: [r] });
      await request("sync");
    }
    const history = await invoke("team_history", { recordId: host.id });
    expect(history).toHaveLength(11);
    await invoke("team_checkout", {
      recordId: host.id,
      revision: history.at(-1).revision,
      permanent: false,
    });
    expect(
      (await invoke("vault_info")).records.find((r) => r.id === host.id).data
        ._teamTemporaryRevision,
    ).toBe(history.at(-1).revision);
    await invoke("team_checkout", {
      recordId: host.id,
      revision: null,
      permanent: false,
    });
    const before = (await invoke("vault_info")).records.find(
      (r) => r.id === host.id,
    );
    await invoke("team_checkout", {
      recordId: host.id,
      revision: history.at(-1).revision,
      permanent: true,
    });
    const restored = (await invoke("vault_info")).records.find(
      (r) => r.id === host.id,
    );
    expect(restored.data.label).toBe(history.at(-1).record.data.label);
    const versions = await invoke("team_history", { recordId: host.id });
    expect(versions.find((v) => v.returnPoint).revision).toBe(
      before.data._teamRevision,
    );
    await invoke("team_checkout", {
      recordId: host.id,
      revision: before.data._teamRevision,
      permanent: true,
    });
    expect(
      (await invoke("vault_info")).records.find((r) => r.id === host.id).data
        .label,
    ).toBe(before.data.label);
    const moved = rec("group", { label: "Staging", groupId: "" });
    await invoke("records_save", { records: [moved] });
    await request("sync");
    const preview = await request("move_preview", {
      vaultId,
      recordId: host.id,
      parentId: moved.id,
    });
    await invoke("team_move", {
      recordId: host.id,
      parentId: moved.id,
      aclRevision: preview.aclRevision,
      recordRevision: preview.recordRevision,
    });
    expect(
      (await invoke("vault_info")).records.find((r) => r.id === host.id).data
        .groupId,
    ).toBe(moved.id);
    const backup = path.join(work, "team.ttbackup");
    await invoke("backup_create", {
      path: backup,
      password,
      includeProfiles: false,
      ids: [host.id],
    });
    const hash = crypto
      .createHash("sha256")
      .update(await fs.readFile(backup))
      .digest("hex");
    const imported = await invoke("import_preview", {
      path: backup,
      format: "auto",
      password,
    });
    const backedUp = (
      await invoke("backup_preview", { path: backup, password })
    ).vaults[0].records;
    expect(backedUp.some((r) => r.id === jump.id)).toBe(true);
    expect(backedUp.some((r) => r.id === moved.id)).toBe(true);
    const importedHost = imported.records.find(
      (r) => r.kind === "host" && r.data.chain?.length,
    );
    expect(importedHost).toBeDefined();
    expect(
      imported.records.find((r) => r.id === importedHost.data.chain[0]).data
        .label,
    ).toBe("Lab Jump");
    expect(
      imported.records.find((r) => r.id === importedHost.data.groupId).data
        .label,
    ).toBe("Staging");
    const applied = await invoke("import_apply", {
      records: imported.records,
      policy: "copy",
      vaultId,
    });
    expect(applied.added).toBe(imported.records.length);
    await request("sync");
    expect(
      crypto
        .createHash("sha256")
        .update(await fs.readFile(backup))
        .digest("hex"),
    ).toBe(hash);
    const disposable = rec("host", {
      label: "History deletion fixture",
      address: "127.0.0.1",
      groupId: moved.id,
    });
    await invoke("records_save", { records: [disposable] });
    await request("sync");
    await invoke("records_delete", { ids: [disposable.id] });
    await request("sync");
    expect(
      (await invoke("vault_info")).records.some((r) => r.id === disposable.id),
    ).toBe(false);
    await invoke("team_checkout", {
      recordId: disposable.id,
      revision: 1,
      permanent: true,
    });
    expect(
      (await invoke("vault_info")).records.some((r) => r.id === disposable.id),
    ).toBe(true);
    await browser.refresh();
    await $(".team-workspace").waitForDisplayed({ timeout: 30000 });
    await $(".team-status.online").waitForDisplayed({ timeout: 20000 });
    // The embedded driver's synthetic option click does not perform a native
    // select default action. Dispatch the select change; the real UI button
    // below must load and render the actual PostgreSQL history.
    await browser.waitUntil(
      async () =>
        await $(`.team-history option[value="${host.id}"]`).isExisting(),
    );
    await browser.execute((id) => {
      const select = document.querySelector(".team-history select");
      select.value = id;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    }, host.id);
    await $("button=Geçmişi getir").waitForEnabled();
    await $("button=Geçmişi getir").click();
    await $(".team-version").waitForDisplayed();
    await $("button=Geçmişi getir").waitForEnabled();
    await $(".team-version").scrollIntoView({ block: "center" });
    await browser.saveScreenshot(
      path.join(root, "artifacts", "team-history.png"),
    );
    await $("button=İşlem geçmişi").click();
    await $(".team-audit").waitForDisplayed();
    await browser.saveScreenshot(
      path.join(root, "artifacts", "team-audit.png"),
    );
    await fs.writeFile(
      path.join(work, "demo.json"),
      JSON.stringify(
        { owner, operator, editor, teamId, vaultId, password },
        null,
        2,
      ),
    );
    console.log(
      "Team native acceptance evidence saved to ignored lab directory.",
    );
  });
});
