import { browser, $, $$, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { selectValue } from "./interaction.mjs";

const root = process.cwd(),
  work = path.join(root, ".lab", `context-menu-${Date.now()}`);
const vaultPath = path.join(work, "test.ttvault"),
  password = "Disposable-context-menu-test";
const entity = (kind, data) => ({
  id: crypto.randomUUID(),
  kind,
  data,
  updatedAt: Date.now(),
});
const group = entity("group", { label: "Production", username: "deploy" });
const destination = entity("group", { label: "Archive" });
const child = entity("group", { label: "Nested", groupId: group.id });
const key = entity("credential", {
  label: "Test identity",
  publicKey: "ssh-ed25519 fixture",
  password: "private-fixture-sentinel",
});
const host = entity("host", {
  label: "API",
  address: "api.example.test",
  groupId: group.id,
  credentialId: key.id,
});
const nested = entity("host", {
  label: "Database",
  address: "db.example.test",
  groupId: child.id,
  chain: [host.id],
});
let recentPath, recentBytes;
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
async function refresh() {
  await browser.executeAsync((done) =>
    window.__TAURI__.event.emit("vault-changed").then(() => done(true)),
  );
}
const records = async () => (await invoke("vault_info")).records;
const card = (name) => $(`.record-card[aria-label="${name}"]`);
// Embedded WDIO omits the contextmenu event. Dispatch it in the real WebView;
// the Playwright suite separately covers trusted right-button/keyboard input.
async function menu(element, action) {
  await element.waitForDisplayed();
  await browser.execute((el) => {
    const r = el.getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        button: 2,
        clientX: r.x + 40,
        clientY: r.y + 25,
      }),
    );
  }, element);
  await $('[role="menu"]').waitForDisplayed();
  if (action) await $(`button[role="menuitem"]=${action}`).click();
}
async function rootFolder() {
  await $(".breadcrumb").$("button=Hosts").click();
}
async function waitRecord(label) {
  await browser.waitUntil(async () =>
    (await records()).some((r) => r.data.label === label),
  );
}
async function folder(name) {
  for (const el of await $$(".group-main"))
    if ((await el.$("strong").getText()) === name) return el;
  throw Error(`Folder missing: ${name}`);
}
async function pathInput(side, value) {
  const input = await $(`input[aria-label="${side} path"]`);
  await input.setValue(value);
  await browser.execute(
    (el) =>
      el.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
      ),
    input,
  );
}
describe("Native context actions persist to the vault and filesystem", () => {
  before(async () => {
    await fs.mkdir(work, { recursive: true });
    await fs.mkdir(path.join(root, "artifacts", "context-review"), {
      recursive: true,
    });
    await browser.setTimeout({ script: 120000 });
    const info = await invoke("app_info");
    recentPath = path.join(path.dirname(info.defaultVaultPath), "recent.json");
    try {
      recentBytes = await fs.readFile(recentPath);
      await fs.writeFile(path.join(work, "recent-before.bin"), recentBytes);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    await invoke("vault_create", {
      path: vaultPath,
      name: "Context actions lab",
      password,
    });
    await invoke("records_save", {
      records: [group, destination, child, key, host, nested],
    });
    await refresh();
    await $("h1=Hosts").waitForDisplayed();
  });
  after(async () => {
    await invoke("vault_lock").catch(() => {});
    if (recentPath) {
      const current = JSON.parse(
        await fs.readFile(recentPath, "utf8").catch(() => "{}"),
      );
      if (current.path === vaultPath) {
        if (recentBytes) await fs.writeFile(recentPath, recentBytes);
        else await fs.unlink(recentPath);
      }
    }
  });
  it("shows read-only details, duplicates a complete group, moves and deletes copies", async () => {
    await (await folder("Production")).click();
    await expect($(".record-details")).toHaveText(
      expect.stringContaining("2 hosts"),
    );
    expect(await $$(".record-details input")).toHaveLength(0);
    expect(
      await $$(".record-card input[type=checkbox], .selection-bar"),
    ).toHaveLength(0);
    await menu(await folder("Production"), "Duplicate");
    await waitRecord("Production copy");
    let saved = await records();
    const copiedGroup = saved.find((r) => r.data.label === "Production copy"),
      copiedChild = saved.find(
        (r) => r.data.groupId === copiedGroup.id && r.kind === "group",
      );
    const copiedApi = saved.find(
      (r) => r.data.groupId === copiedGroup.id && r.kind === "host",
    );
    const copiedDb = saved.find((r) => r.data.groupId === copiedChild.id);
    expect(copiedApi.data.credentialId).toBe(key.id);
    expect(copiedDb.data.chain).toEqual([copiedApi.id]);
    await (await folder("Production")).click();
    await card("API").click();
    await expect($(".record-details")).toHaveText(
      expect.stringContaining("deploy"),
    );
    expect(await $(".record-details").getText()).not.toContain(
      "private-fixture-sentinel",
    );
    await menu(await card("API"));
    await browser.saveScreenshot(
      path.join(root, "artifacts/context-review/native-host-menu.png"),
    );
    await $('button[role="menuitem"]=Duplicate').click();
    await waitRecord("API copy");
    await menu(await card("API copy"), "Move to group…");
    await selectValue($(".modal select"), destination.id);
    await $("button=Move records").click();
    await browser.waitUntil(
      async () =>
        (await records()).find((r) => r.data.label === "API copy")?.data
          .groupId === destination.id,
    );
    await rootFolder();
    await (await folder("Archive")).click();
    await menu(await card("API copy"), "Delete…");
    await $("button=Delete").click();
    await browser.waitUntil(
      async () => !(await records()).some((r) => r.data.label === "API copy"),
    );
    const before = await records();
    await invoke("vault_lock");
    await invoke("vault_open", { path: vaultPath, password });
    await refresh();
    expect(await records()).toEqual(before);
    const bytes = await fs.readFile(vaultPath);
    expect(bytes.includes(Buffer.from("api.example.test"))).toBe(false);
    expect(bytes.includes(Buffer.from("private-fixture-sentinel"))).toBe(false);
  });
  it("keeps a referenced identity when deletion is rejected by the real vault", async () => {
    await $("button=Keychain").click();
    await menu(await card("Test identity"), "Delete…");
    await $("button=Delete").click();
    await $(".toast.error").waitForDisplayed();
    expect((await records()).some((r) => r.id === key.id)).toBe(true);
    await $("button=Cancel").click();
    await $('.toast.error button[aria-label="Dismiss message"]').click();
  });
  it("previews selected export scope and round-trips the scoped encrypted backup through the real backend", async () => {
    await $(".main-nav button:first-child").click();
    await rootFolder();
    await (await folder("Production")).click();
    const backupPath = path.join(work, "selected.ttbackup");
    await menu(await card("API"), "Export encrypted backup…");
    await expect($(".modal")).toHaveText(
      expect.stringContaining("Selected export: 1 records"),
    );
    await $('.modal button[aria-label="Close dialog"]').click();
    // Embedded WebDriver cannot control the Windows save dialog, and WDIO's
    // global invoke mock does not intercept the app's bundled ES-module API.
    // Playwright verifies the UI's exact ids/arguments. Here the real backend
    // writes and decrypts the selected scope; the OS picker is not tested.
    const report = await invoke("backup_bundle", {
      path: backupPath,
      password,
      sources: [],
      ids: [host.id],
      includeProfiles: false,
      includeFiles: false,
    });
    expect(report.records).toBe(3);
    const backup = await invoke("backup_preview", {
      path: backupPath,
      password,
    });
    expect(backup.vaults[0].records.map((r) => r.id).sort()).toEqual(
      [host.id, group.id, key.id].sort(),
    );
    const bytes = await fs.readFile(backupPath);
    expect(bytes.includes(Buffer.from("api.example.test"))).toBe(false);
  });
  it("renames, copies, creates and deletes real local files from SFTP menus", async () => {
    const source = path.join(work, "source"),
      target = path.join(work, "target");
    await fs.mkdir(source);
    await fs.mkdir(target);
    const content = "Türkçe dosya: ğüşöçıİ\nSecond line\n";
    await fs.writeFile(path.join(source, "Notes.txt"), content);
    await $(".top-tabs").$("button=SFTP").click();
    await pathInput("Left", source);
    await $('.file-row[aria-label="Notes.txt"]').waitForDisplayed();
    await menu(await $('.file-row[aria-label="Notes.txt"]'), "Rename…");
    await $(".modal input").setValue("Notlar.txt");
    await $("button=Apply").click();
    await $('.file-row[aria-label="Notlar.txt"]').waitForDisplayed();
    expect(await fs.readFile(path.join(source, "Notlar.txt"), "utf8")).toBe(
      content,
    );
    await selectValue($('select[aria-label="Right connection"]'), "local");
    await pathInput("Right", target);
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () => !document.querySelector('[data-file-pane="1"] .busy'),
        ),
    );
    await menu(
      await $('.file-row[aria-label="Notlar.txt"]'),
      "Copy to target directory",
    );
    await browser.waitUntil(
      async () =>
        await fs.readFile(path.join(target, "Notlar.txt"), "utf8").then(
          (v) => v === content,
          () => false,
        ),
    );
    expect(await fs.readFile(path.join(source, "Notlar.txt"), "utf8")).toBe(
      content,
    );
    await menu(await $('[data-file-pane="0"] .file-list'), "New folder…");
    await $(".modal input").setValue("Empty folder");
    await $("button=Apply").click();
    await $('.file-row[aria-label="Empty folder"]').waitForDisplayed();
    expect(
      (await fs.stat(path.join(source, "Empty folder"))).isDirectory(),
    ).toBe(true);
    await menu(
      await $('[data-file-pane="0"] .file-row[aria-label="Notlar.txt"]'),
      "Delete…",
    );
    await $("button=Delete").click();
    await browser.waitUntil(
      async () =>
        await fs.stat(path.join(source, "Notlar.txt")).then(
          () => false,
          () => true,
        ),
    );
    expect(await fs.readFile(path.join(target, "Notlar.txt"), "utf8")).toBe(
      content,
    );
    await menu(
      await $('[data-file-pane="1"] .file-row[aria-label="Notlar.txt"]'),
    );
    await browser.saveScreenshot(
      path.join(root, "artifacts/context-review/native-sftp-menu.png"),
    );
  });
});
