// Run only with an explicitly supplied personal file. Never copy it into fixtures.
import { browser, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
const source = process.env.TERMTERM_IMPORT_FILE;
const hash = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
async function invoke(command, args = {}) {
  const result = JSON.parse(
    await browser.executeAsync(
      (command, args, done) => {
        window.__TAURI__.core.invoke(command, args).then(
          (value) => done(JSON.stringify({ value })),
          (error) => done(JSON.stringify({ error: String(error) })),
        );
      },
      command,
      args,
    ),
  );
  if (result.error) throw Error(result.error);
  return result.value;
}
describe("User-provided MobaXterm sessions", () => {
  it("imports every connection and folder, preserves Turkish labels, and reopens the vault", async function () {
    if (!source) return this.skip();
    await browser.setTimeout({ script: 120000 });
    const bytes = await fs.readFile(source),
      originalHash = hash(bytes);
    const text = new TextDecoder("windows-1254", { fatal: true }).decode(bytes);
    const expected = [],
      folders = new Set();
    let folder = "";
    for (const line of text.split(/\r?\n/)) {
      const split = line.indexOf("=");
      if (split < 0) continue;
      const label = line.slice(0, split),
        value = line.slice(split + 1);
      if (label === "SubRep") {
        folder = value.replaceAll("\\", "/");
        if (folder) folders.add(folder);
        continue;
      }
      if (!value.startsWith("#")) continue;
      const fields = value.split("#")[2]?.split("%");
      if (fields?.[0] === "0")
        expected.push([label, fields[1], Number(fields[2]), fields[3], folder]);
    }
    let unicodeRejected = false;
    try {
      await invoke("import_preview", { path: source, format: "auto" });
    } catch {
      unicodeRejected = true;
    }
    expect(unicodeRejected).toBe(true);
    const preview = await invoke("import_preview", {
      path: source,
      format: "auto",
      encoding: "windows-1254",
    });
    expect(preview.format).toBe("mobaxterm");
    const directory = path.resolve(".lab/vendor-import");
    await fs.mkdir(directory, { recursive: true });
    const vaultPath = path.join(directory, crypto.randomUUID() + ".ttvault");
    const password = crypto.randomBytes(32).toString("base64url");
    let vault = await invoke("vault_create", {
      path: vaultPath,
      name: "MobaXterm Import Test",
      password,
    });
    const applied = await invoke("import_apply", {
      records: preview.records,
      policy: "copy",
      vaultId: vault.id,
    });
    expect(applied.added).toBe(preview.records.length);
    await invoke("vault_lock");
    vault = await invoke("vault_open", { path: vaultPath, password });
    const records = new Map(vault.records.map((r) => [r.id, r]));
    function groupPath(id) {
      const r = records.get(id);
      return r
        ? [groupPath(r.data.groupId), r.data.label].filter(Boolean).join("/")
        : "";
    }
    const actual = vault.records
      .filter((r) => r.kind === "host")
      .map((r) => [
        r.data.label,
        r.data.address,
        r.data.port,
        r.data.username,
        groupPath(r.data.groupId),
      ]);
    expect(hash(JSON.stringify(actual.sort()))).toBe(
      hash(JSON.stringify(expected.sort())),
    );
    expect(
      hash(
        JSON.stringify(
          vault.records
            .filter((r) => r.kind === "group")
            .map((r) => groupPath(r.id))
            .sort(),
        ),
      ),
    ).toBe(hash(JSON.stringify([...folders].sort())));
    expect(hash(await fs.readFile(source))).toBe(originalHash);
    await browser.executeAsync((done) =>
      window.__TAURI__.event.emit("vault-changed").then(() => done(true)),
    );
    await browser
      .$('button[title="Hosts"]')
      .isExisting()
      .then(async (exists) => {
        if (exists) await browser.$('button[title="Hosts"]').click();
      });
    await browser.waitUntil(
      async () => (await browser.$$(".group-card")).length > 0,
    );
    await browser.saveScreenshot(
      path.resolve("artifacts/mobaxterm-import-folders.png"),
    );
    const firstFolder = actual.find((row) => row[4])?.[4];
    if (firstFolder)
      for (const name of firstFolder.split("/")) {
        const buttons = await browser.$$(".group-main");
        for (const button of buttons)
          if ((await button.$("strong").getText()) === name) {
            await button.click();
            break;
          }
      }
    await browser.waitUntil(
      async () => (await browser.$$(".record-card")).length > 0,
    );
    await browser.saveScreenshot(
      path.resolve("artifacts/mobaxterm-import-sessions.png"),
    );
    await fs.writeFile(
      path.resolve("artifacts/mobaxterm-import-result.json"),
      JSON.stringify(
        {
          sourceHash: originalHash,
          encoding: "windows-1254",
          hosts: actual.length,
          groups: folders.size,
          added: applied.added,
          updated: applied.updated,
          skipped: applied.skipped,
          failed: applied.failed,
          sourceUnchanged: true,
          reopened: true,
          fieldsAndRelationshipsEqual: true,
          warningCount: preview.warnings.length,
          connectionTest: "No connections to personal hosts were initiated",
          provenance:
            "User-provided actual export; producing application version not supplied",
        },
        null,
        2,
      ),
    );
    console.log(
      `MobaXterm: ${actual.length} hosts, ${folders.size} folders; encrypted reopen and source hash verified.`,
    );
    await invoke("vault_lock");
  });
});
