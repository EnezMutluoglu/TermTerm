import { browser, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
const root = process.cwd(),
  work = path.join(root, ".lab", "v03-" + Date.now()),
  password = "Disposable-v03-password";
const record = (kind, data) => ({
  id: crypto.randomUUID(),
  kind,
  data,
  updatedAt: Date.now(),
});
const hash = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
async function invoke(command, args = {}) {
  const r = await browser.executeAsync(
    (command, args, done) =>
      window.__TAURI__.core.invoke(command, args).then(
        (value) => done({ value }),
        (e) => done({ error: String(e) }),
      ),
    command,
    args,
  );
  if (r.error) throw Error(r.error);
  return r.value;
}
async function launch(command, args) {
  await browser.execute(
    (command, args) => {
      window.__jobs[args.id] = { pending: true };
      window.__TAURI__.core.invoke(command, args).then(
        (value) => (window.__jobs[args.id] = { value }),
        (e) => (window.__jobs[args.id] = { error: String(e) }),
      );
    },
    command,
    args,
  );
}
async function job(id) {
  await browser.waitUntil(
    async () =>
      !JSON.parse(
        await browser.execute((id) => JSON.stringify(window.__jobs[id]), id),
      )?.pending,
    { timeout: 90000 },
  );
  return JSON.parse(
    await browser.execute((id) => JSON.stringify(window.__jobs[id]), id),
  );
}
let ssh, host, connection, remoteHome;
const remoteFiles = [];
describe("0.3 native import and transfer acceptance", () => {
  before(async () => {
    await fs.mkdir(work, { recursive: true });
    ssh = JSON.parse(
      await fs.readFile(path.join(root, ".lab/ssh.json"), "utf8"),
    );
    await fs.writeFile(path.join(work, "id_ed25519"), ssh.privateKey);
    await browser.setTimeout({ script: 120000 });
    // Refuse a stale test binary before generating any credential-bearing data.
    await invoke("operation_cancel", { id: crypto.randomUUID() });
    await browser.executeAsync((done) => {
      window.__jobs = {};
      window.__events = {};
      window.__progress = {};
      window.__transferStates = {};
      Promise.all([
        window.__TAURI__.event.listen("session-event", ({ payload: e }) => {
          const s = window.__events[e.id] ?? { output: "", status: "" };
          if (e.kind === "data") s.output += atob(e.detail);
          else s.status = e.kind;
          window.__events[e.id] = s;
        }),
        window.__TAURI__.event.listen(
          "transfer-progress",
          ({ payload: e }) => (window.__progress[e.id] = e),
        ),
        window.__TAURI__.event.listen(
          "transfer-state",
          ({ payload: e }) => (window.__transferStates[e.id] = e),
        ),
      ]).then(() => done(true));
    });
  });
  it("applies every adapter, resolves duplicates, reopens encrypted files and connects imported hosts", async () => {
    const key = path.join(work, "id_ed25519").replaceAll("\\", "/");
    const files = [
      [
        "csv",
        "hosts.csv",
        `label,address,port,username,password,group,tags\n"İstanbul, 東京",127.0.0.1,22222,${ssh.username},${ssh.password},Lab/Linux,test;unicode\n`,
      ],
      [
        "openssh",
        "config",
        `Host lab\n HostName 127.0.0.1\n Port 22222\n User ${ssh.username}\n IdentityFile "${key}"\n`,
      ],
      [
        "putty",
        "sessions.reg",
        `Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\Lab]\n"HostName"="127.0.0.1"\n"PortNumber"=dword:000056ce\n"UserName"="${ssh.username}"\n"PublicKeyFile"="${key}"\n`,
      ],
      [
        "mobaxterm",
        "sessions.ini",
        `[Bookmarks]\nSubRep=Lab/Linux\nLab=#109#0%127.0.0.1%22222%${ssh.username}%${key}#\n`,
      ],
      [
        "securecrt",
        "sessions.xml",
        `<?xml version="1.0"?><key name="Sessions"><key name="Lab"><string name="Hostname">127.0.0.1</string><dword name="[SSH2] Port">22222</dword><string name="Username">${ssh.username}</string><string name="Identity Filename">${key}</string></key></key>`,
      ],
      [
        "ansible",
        "inventory.ini",
        `[linux]\nlab ansible_host=127.0.0.1 ansible_port=22222 ansible_user=${ssh.username} ansible_ssh_private_key_file="${key}"\n`,
      ],
      [
        "ansible",
        "inventory.yml",
        `all:\n  children:\n    linux:\n      hosts:\n        lab:\n          ansible_host: 127.0.0.1\n          ansible_port: 22222\n          ansible_user: ${ssh.username}\n          ansible_ssh_private_key_file: '${key}'\n`,
      ],
    ];
    await invoke("vault_create", {
      path: path.join(work, "seed.ttvault"),
      name: "Seed",
      password,
    });
    await invoke("records_save", {
      records: [
        record("host", {
          label: "Archive lab",
          address: "127.0.0.1",
          port: 22222,
          username: ssh.username,
          keyPath: key,
        }),
      ],
    });
    for (const ext of ["ttvault", "ttbackup"]) {
      const file = path.join(work, "source." + ext);
      if (ext === "ttvault") await invoke("vault_copy", { path: file });
      else
        await invoke("backup_bundle", {
          path: file,
          password,
          includeProfiles: false,
          includeFiles: true,
          sources: [],
          ids: [],
        });
      files.push([ext, "source." + ext, null]);
    }
    for (let i = 0; i < files.length; i++) {
      const [format, name, text] = files[i],
        source = path.join(work, name),
        vaultPath = path.join(work, `import-${i}.ttvault`);
      if (text !== null) await fs.writeFile(source, text);
      const before = hash(await fs.readFile(source));
      let v = await invoke("vault_create", {
        path: vaultPath,
        name: format,
        password,
      });
      const preview = await invoke("import_preview", {
        path: source,
        format: "auto",
        password,
      });
      expect(preview.format).toBe(format);
      expect(preview.records.filter((r) => r.kind === "host")).toHaveLength(1);
      const result = await invoke("import_apply", {
        records: preview.records,
        policy: "copy",
        vaultId: v.id,
      });
      expect(result.added).toBe(preview.records.length);
      expect(result.failed).toBe(0);
      v = result.vault;
      const imported = v.records;
      const skipped = await invoke("import_apply", {
        records: preview.records,
        policy: "skip",
        vaultId: v.id,
      });
      expect(skipped.skipped).toBe(preview.records.length);
      expect(skipped.added).toBe(0);
      const updated = await invoke("import_apply", {
        records: preview.records,
        policy: "update",
        vaultId: v.id,
      });
      expect(updated.updated).toBe(preview.records.length);
      await invoke("vault_lock");
      v = await invoke("vault_open", { path: vaultPath, password });
      expect(
        hash(
          JSON.stringify(v.records.map((r) => [r.id, r.kind, r.data]).sort()),
        ),
      ).toBe(
        hash(
          JSON.stringify(imported.map((r) => [r.id, r.kind, r.data]).sort()),
        ),
      );
      host = v.records.find((r) => r.kind === "host");
      expect(host.data.port).toBe(22222);
      await invoke("records_save", {
        records: [
          record("knownHost", {
            address: "[127.0.0.1]:22222",
            publicKey: ssh.publicKey,
          }),
        ],
      });
      const id = await invoke("session_start", {
        hostId: host.id,
        shell: null,
      });
      await browser.waitUntil(
        async () =>
          await browser.execute(
            (id) => window.__events[id]?.status === "connected",
            id,
          ),
        { timeout: 30000 },
      );
      await invoke("session_input", {
        id,
        data: "printf 'IMPORT_%s\\n' 'CONNECTED'\r",
      });
      await browser.waitUntil(
        async () =>
          await browser.execute(
            (id) => window.__events[id]?.output.includes("IMPORT_CONNECTED"),
            id,
          ),
        { timeout: 10000 },
      );
      await invoke("session_input", { id, close: true });
      expect(hash(await fs.readFile(source))).toBe(before);
    }
  });
  it("rejects stale vault targets and invalid records without partial writes", async () => {
    const v = await invoke("vault_info"),
      before = v.records;
    for (const args of [
      {
        records: [record("host", { label: "Invalid", port: 70000 })],
        policy: "copy",
        vaultId: v.id,
      },
      { records: [], policy: "copy", vaultId: crypto.randomUUID() },
    ]) {
      let rejected = false;
      try {
        await invoke("import_apply", args);
      } catch {
        rejected = true;
      }
      expect(rejected).toBe(true);
    }
    expect(hash(JSON.stringify((await invoke("vault_info")).records))).toBe(
      hash(JSON.stringify(before)),
    );
  });
  it("pauses and resumes the same staging file, restarts changed sources and cancels without replacing the target", async () => {
    connection = crypto.randomUUID();
    remoteHome = await invoke("sftp_connect", {
      id: connection,
      hostId: host.id,
    });
    const source = path.join(work, "large.bin");
    const file = await fs.open(source, "w");
    await file.truncate(256 * 1024 * 1024);
    await file.close();
    const remote = remoteHome + "/v03-" + crypto.randomUUID() + ".bin";
    remoteFiles.push(remote);
    const dest = { connection, path: remote },
      args = {
        id: crypto.randomUUID(),
        source: { connection: "local", path: source },
        dest,
        overwrite: false,
      };
    await launch("file_transfer", args);
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) => (window.__progress[id]?.transferred ?? 0) > 0,
          args.id,
        ),
      { timeout: 20000 },
    );
    expect(
      await invoke("transfer_control", { id: args.id, action: "pause" }),
    ).toBe(true);
    await browser.pause(300);
    const files = await invoke("file_list", {
      endpoint: { connection, path: remoteHome },
    });
    const staging = files.find(
      (f) =>
        f.path.startsWith(remote + ".termterm-") && f.path.endsWith(".part"),
    );
    expect(!!staging).toBe(true);
    const replacement = crypto.randomBytes(2 * 1024 * 1024 + 39);
    await fs.writeFile(source, replacement);
    expect(
      await invoke("transfer_control", { id: args.id, action: "resume" }),
    ).toBe(true);
    expect((await job(args.id)).value).toBe(replacement.length);
    const downloaded = path.join(work, "changed-download.bin");
    await invoke("file_transfer", {
      id: crypto.randomUUID(),
      source: dest,
      dest: { connection: "local", path: downloaded },
      overwrite: false,
    });
    expect(hash(await fs.readFile(downloaded))).toBe(hash(replacement));
    const big = await fs.open(source, "w");
    await big.truncate(256 * 1024 * 1024);
    await big.close();
    const cancel = { ...args, id: crypto.randomUUID(), overwrite: true };
    await launch("file_transfer", cancel);
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) => (window.__progress[id]?.transferred ?? 0) > 0,
          cancel.id,
        ),
      { timeout: 20000 },
    );
    const start = Date.now();
    expect(
      await invoke("transfer_control", { id: cancel.id, action: "cancel" }),
    ).toBe(true);
    expect(Date.now() - start).toBeLessThan(2000);
    expect((await job(cancel.id)).error).toContain("Transfer cancelled");
    const after = path.join(work, "cancelled-download.bin");
    await invoke("file_transfer", {
      id: crypto.randomUUID(),
      source: dest,
      dest: { connection: "local", path: after },
      overwrite: false,
    });
    expect(hash(await fs.readFile(after))).toBe(hash(replacement));
    expect(
      (
        await invoke("file_list", {
          endpoint: { connection, path: remoteHome },
        })
      ).some((f) => f.path.startsWith(remote + ".termterm-")),
    ).toBe(false);
  });
  it("preserves workspace references and performs remote folder, rename, chmod and remote-to-remote copy actions", async () => {
    const group=record("group",{label:"Action test folder"});
    const item=record("host",{label:"Action test host",address:"127.0.0.1",groupId:group.id});
    const workspace=record("workspace",{label:"Action test workspace",hostIds:[item.id]});
    await invoke("records_save",{records:[group,item,workspace]});
    let rejected=false;try{await invoke("records_delete",{ids:[item.id]});}catch{rejected=true;}
    expect(rejected).toBe(true);
    await invoke("records_delete",{ids:[workspace.id,item.id,group.id]});
    expect((await invoke("vault_info")).records.some(r=>[group.id,item.id,workspace.id].includes(r.id))).toBe(false);
    const second=crypto.randomUUID();await invoke("sftp_connect",{id:second,hostId:host.id});
    const directory=remoteHome+"/actions-"+crypto.randomUUID(), target=directory+"/Türkçe 空白.bin", renamed=directory+"/renamed.bin";
    const endpoint=p=>({connection:second,path:p});
    try {
      await invoke("file_action",{endpoint:endpoint(directory),action:"mkdir"});
      remoteFiles.push(renamed,directory);
      await invoke("file_transfer",{id:crypto.randomUUID(),source:{connection,path:remoteFiles[0]},dest:endpoint(target),overwrite:false});
      await invoke("file_action",{endpoint:endpoint(target),action:"chmod",permissions:0o640});
      await invoke("file_action",{endpoint:endpoint(target),action:"rename",target:renamed});
      const listing=await invoke("file_list",{endpoint:endpoint(directory)});
      expect(listing.map(f=>f.name)).toEqual(["renamed.bin"]);
      expect(listing[0].permissions & 0o777).toBe(0o640);
      const downloaded=path.join(work,"remote-copy.bin");
      await invoke("file_transfer",{id:crypto.randomUUID(),source:endpoint(renamed),dest:{connection:"local",path:downloaded},overwrite:false});
      expect(hash(await fs.readFile(downloaded))).toBe(hash(await fs.readFile(path.join(work,"changed-download.bin"))));
      await invoke("file_action",{endpoint:endpoint(renamed),action:"remove"});
      expect(await invoke("file_list",{endpoint:endpoint(directory)})).toHaveLength(0);
      await invoke("file_action",{endpoint:endpoint(directory),action:"remove"});
    } finally {await invoke("sftp_disconnect",{id:second});}
  });
  after(async () => {
    if (connection) {
      for (const p of remoteFiles)
        await invoke("file_action", {
          endpoint: { connection, path: p },
          action: "remove",
          target: null,
          permissions: null,
        }).catch(() => {});
      await invoke("sftp_disconnect", { id: connection }).catch(() => {});
    }
    await invoke("vault_lock").catch(() => {});
  });
});
