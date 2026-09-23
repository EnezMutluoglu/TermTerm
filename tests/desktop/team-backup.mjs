// Focused native regression; run after team.mjs with its ignored demo.json path.
import { browser, $, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
const invoke = (command, args = {}) =>
  browser
    .executeAsync(
      (command, args, done) =>
        window.__TAURI__.core.invoke(command, args).then(
          (value) => done({ value }),
          (error) => done({ error: String(error) }),
        ),
      command,
      args,
    )
    .then((result) => {
      if (result.error) throw Error(result.error);
      return result.value;
    });
describe("Team portable key files", () => {
  it("includes selected key files and reports missing files, then shows history", async () => {
    await browser.setTimeout({ script: 120000 });
    expect((await invoke("app_info")).defaultVaultPath).toContain(
      "teamdev.e2e",
    );
    const demoPath = process.env.TERMTERM_TEAM_DEMO;
    if (!demoPath)
      throw Error(
        "Set TERMTERM_TEAM_DEMO to the ignored demo.json created by team.mjs",
      );
    const demo = JSON.parse(await fs.readFile(demoPath, "utf8"));
    const profile = JSON.parse(
      await fs.readFile(".lab/team-windows.json", "utf8"),
    );
    profile.database = "termterm_team_e2e";
    await invoke("team_auth", {
      profile,
      login: demo.owner.username,
      password: demo.password,
      email: "",
      register: false,
    });
    await invoke("team_open_vault", {
      vaultId: demo.vaultId,
      teamId: demo.teamId,
      name: "İstanbul Altyapısı",
    });
    const work = path.dirname(demoPath);
    const keyPath = path.join(work, "portable-disposable-key");
    // Disposable bytes only; this test validates the archive, not SSH key parsing.
    const contents = "Disposable portable file fixture\nUnicode: İstanbul\n";
    await fs.writeFile(keyPath, contents);
    const records = [keyPath, path.join(work, "missing-disposable-key")].map(
      (keyPath, i) => ({
        id: crypto.randomUUID(),
        kind: "host",
        data: {
          label: `Portable key fixture ${i}`,
          address: "127.0.0.1",
          keyPath,
        },
        updatedAt: Date.now(),
      }),
    );
    await invoke("records_save", { records });
    await invoke("team_request", { action: "sync", body: {} });
    for (const includeFiles of [false, true]) {
      const destination = path.join(
        work,
        `portable-key-${includeFiles}-${crypto.randomUUID()}.ttbackup`,
      );
      const report = await invoke("backup_bundle", {
        path: destination,
        password: demo.password,
        ids: records.map((r) => r.id),
        sources: [],
        includeFiles,
        includeProfiles: false,
      });
      expect(report.records).toBe(2);
      const saved = (
        await invoke("backup_preview", {
          path: destination,
          password: demo.password,
        })
      ).vaults[0].records;
      const key = saved.find((r) => r.id === records[0].id);
      if (includeFiles) {
        expect(key.data.privateKey).toBe(contents);
        expect(key.data.keyPath).toBeUndefined();
        expect(
          report.warnings.some(
            (w) =>
              w.includes("cannot include") &&
              w.includes("missing-disposable-key"),
          ),
        ).toBe(true);
      } else {
        expect(key.data.keyPath).toBe(keyPath);
        expect(key.data.privateKey).toBeUndefined();
        expect(report.warnings.some((w) => w.includes("not embedded"))).toBe(
          true,
        );
      }
    }
    await invoke("records_delete", { ids: records.map((r) => r.id) });
    await invoke("team_request", { action: "sync", body: {} });
    const visible = (await invoke("vault_info")).records;
    const host = visible
      .filter((r) => r.kind === "host" && r.data.chain?.length)
      .sort((a, b) => b.data._teamRevision - a.data._teamRevision)[0];
    await browser.refresh();
    await $(".team-workspace").waitForDisplayed({ timeout: 30000 });
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
    await browser.execute(() =>
      document
        .querySelector(".team-version")
        .scrollIntoView({ block: "start" }),
    );
    await browser.saveScreenshot(path.resolve("artifacts/team-history.png"));
  });
});
