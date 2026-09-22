// Run three separate application processes, with the same disposable work directory.
// TERMTERM_REMEMBER_PHASE=create|restart|forgotten; never targets a production profile.
import { browser, $, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";

const phase = process.env.TERMTERM_REMEMBER_PHASE;
const work = path.resolve(process.env.TERMTERM_REMEMBER_WORK ?? ".lab/remember-vault");
const vaultPath = path.join(work, "remember-test.ttvault");
const password = "Disposable-native-remember-password";
const field = (label) => $(`//label[contains(@class,'field')][span[contains(.,'${label}')]]//input`);
const remember = () => $(".onboard-form input[type=checkbox]");
const unlock = () => $("button=Kasanın kilidini aç");
async function invoke(command, args = {}) {
  const result = await browser.executeAsync((command, args, done) =>
    window.__TAURI__.core.invoke(command, args).then(
      (value) => done({ value }), (error) => done({ error: String(error) }),
    ), command, args);
  if (result.error) throw Error(result.error);
  return result.value;
}
async function hosts() { await $("h1=Sunucular").waitForDisplayed({ timeout: 30000 }); }
async function lock() {
  await $('[title="Kasayı kilitle"]').click();
  await $("h2=Tekrar hoş geldiniz").waitForDisplayed();
  await field("Kasa parolası").waitForEnabled();
}
describe(`Native remembered vault: ${phase}`, () => {
  before(async () => {
    if (!["create", "restart", "forgotten"].includes(phase)) throw Error("Select a remember test phase");
    await fs.mkdir(work, { recursive: true });
    await browser.setTimeout({ script: 120000 });
    const info = await invoke("app_info");
    if (!info.defaultVaultPath.includes("local.termterm.desktop.e2e")) throw Error("Refusing a production application profile");
    if (phase === "create") {
      const recentPath = path.join(path.dirname(info.defaultVaultPath), "recent.json");
      const previous = await fs.readFile(recentPath).catch((e) => { if (e.code !== "ENOENT") throw e; return null; });
      await fs.writeFile(path.join(work, "restore.json"), JSON.stringify({ recentPath, previous: previous?.toString("base64") ?? null }));
      await browser.waitUntil(async () => await browser.execute(() =>
        !!document.querySelector(".onboard-form form, .welcome-option, .sidebar")),
      );
      if (await $('[title="Kasayı kilitle"]').isExisting()) await lock();
      // Hide only the E2E profile's recent pointer until this disposable vault is created.
      await fs.unlink(recentPath).catch((e) => { if (e.code !== "ENOENT") throw e; });
    }
  });

  it("persists only the requested credential and preserves vault contents across real restarts", async () => {
    if (phase === "create") {
      if (await $("button=Geri").isExisting()) await $("button=Geri").click();
      await $("button*=Yerel kasa oluştur").click();
      await field("Kasa adı").setValue("Türkçe test kasası");
      await field("Kasa dosyası").setValue(vaultPath);
      await field("Kasa parolası").setValue(password);
      await field("Parolayı doğrulayın").setValue(password);
      await remember().click();
      await browser.saveScreenshot(path.join(work, "turkish-login.png"));
      await $("button=Kasa oluştur").click();
      await hosts();
      expect(await invoke("vault_remember_status", { path: vaultPath })).toBe(true);
      await invoke("records_save", { records: [{ id: "41332140-9ea0-4616-aba4-50edc3372463", kind: "host", updatedAt: Date.now(), data: { label: "Production", address: "test.example.invalid", password: "disposable-record-secret" } }] });
      return;
    }
    if (phase === "restart") {
      await hosts(); // No password has been sent to this process.
      expect((await invoke("vault_info")).path).toBe(vaultPath);
      expect((await invoke("vault_info")).records[0].data.password).toBe("disposable-record-secret");
      await lock();
      await expect(remember()).toBeSelected();
      await expect(field("Kasa parolası")).toHaveValue("");
      await unlock().click();
      await hosts();
      await lock();
      await field("Kasa parolası").setValue(password);
      await unlock().click();
      await hosts();
      expect(await invoke("vault_remember_status", { path: vaultPath })).toBe(true);
      await lock();
      await remember().click();
      await unlock().click();
      await hosts();
      expect(await invoke("vault_remember_status", { path: vaultPath })).toBe(false);
      return;
    }
    await $("h2=Tekrar hoş geldiniz").waitForDisplayed();
    await field("Kasa parolası").waitForEnabled();
    await expect(remember()).not.toBeSelected();
    await field("Kasa parolası").setValue("wrong-password");
    await unlock().click();
    await expect($(".onboard-form .notice.error")).toHaveText("Parola yanlış veya şifreli veri hasarlı");
    await field("Kasa parolası").setValue(password);
    await unlock().click();
    await hosts();
    expect((await invoke("vault_info")).records[0].data.label).toBe("Production");
    expect(await invoke("vault_remember_status", { path: vaultPath })).toBe(false);
    await browser.saveScreenshot(path.join(work, "turkish-hosts.png"));
    await $("button*=Güncellemeler").click();
    await $("h2=Uygulama güncellemeleri").waitForDisplayed();
    await browser.saveScreenshot(path.join(work, "turkish-updates.png"));
    await invoke("vault_lock");
  });
});
