import { expect, test } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import path from "node:path";

const fixture = "/tests/ui/authenticator-harness.html";
const publicSeed = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const code = (page: import("@playwright/test").Page) => page.getByRole("button", { name: "alice@example.test kodunu kopyala" });

test("personal codes keep leading zeroes; copy revalidates permission; no seeds in storage", async ({ page }) => {
  await page.goto(fixture);
  await expect(code(page)).toHaveText("0708 1804");
  await code(page).click();
  await expect(page.getByRole("status")).toHaveText("Kod panoya kopyalandı.");
  expect(await page.evaluate(() => (window as any).fixture.writes)).toEqual(["07081804"]);
  await page.evaluate(() => { (window as any).fixture.codeAllowed = false; });
  await code(page).click();
  await expect(code(page)).toBeDisabled();
  expect(await page.evaluate(() => (window as any).fixture.writes)).toEqual(["07081804"]);
  expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0]);
});

test("Team wrapper, search, access buttons, no-code rows and mocked screenshot", async ({ page }) => {
  await page.goto(`${fixture}?mode=team`);
  await expect(page.locator(".authenticator-card")).toHaveCount(3);
  await expect(page.getByRole("button", { name: "limited@example.test kodunu kopyala" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "limited@example.test hesabını sil" })).toHaveCount(0);
  await page.getByRole("button", { name: "operations@example.test paylaşımını yönet" }).click();
  await page.getByRole("button", { name: "operations@example.test dışa aktar" }).click();
  await page.getByRole("button", { name: "operations@example.test kurtarma erişimini yönet" }).click();
  expect(await page.evaluate(() => { const f = (window as any).fixture; return [f.shared, f.exported, f.recovered, f.calls[0].command]; })).toEqual(["team", "team", "team", "team_v2_action"]);
  await mkdir("artifacts/authenticator-ui", { recursive: true });
  await page.screenshot({ path: "artifacts/authenticator-ui/mock-overview.png" });
  await page.getByLabel("Hesap görünürlüğü").selectOption("shared");
  await expect(page.locator(".authenticator-card")).toHaveCount(2);
  await page.getByRole("textbox", { name: "Authenticator hesaplarında ara" }).fill("Team Lab");
  await expect(page.locator(".authenticator-card")).toHaveCount(1);
  await page.setViewportSize({ width: 600, height: 800 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("polling is bounded and non-overlapping, hidden views do not refresh", async ({ page }) => {
  await page.goto(`${fixture}?mode=expiry`);
  await expect(code(page)).toBeEnabled();
  await page.evaluate(() => { (window as any).fixture.hold = true; });
  await expect.poll(() => page.evaluate(() => (window as any).fixture.active)).toBe(1);
  await page.waitForTimeout(1300);
  expect(await page.evaluate(() => (window as any).fixture.maximumActive)).toBe(1);
  await expect(code(page)).toBeDisabled();
  await page.getByRole("button", { name: "Toggle visibility" }).click();
  await page.evaluate(() => { const f = (window as any).fixture; f.hold = false; f.release(); });
  const calls = await page.evaluate(() => (window as any).fixture.calls.length);
  await page.waitForTimeout(1300);
  expect(await page.evaluate(() => (window as any).fixture.calls.length)).toBe(calls);
  await expect(page.locator(".authenticator-card")).toHaveCount(0);
});

test("unmount and account switch discard delayed copy and old list responses", async ({ page }) => {
  await page.goto(fixture);
  await expect(code(page)).toBeEnabled();
  await page.evaluate(() => { (window as any).fixture.hold = true; });
  await code(page).click();
  await expect.poll(() => page.evaluate(() => (window as any).fixture.active)).toBe(1);
  await page.evaluate(() => { (window as any).fixture.hold = false; });
  await page.getByRole("button", { name: "Switch account" }).click();
  await page.evaluate(() => { const f = (window as any).fixture; f.hold = false; f.release(); });
  await expect(page.getByRole("button", { name: "new-account@example.test kodunu kopyala" })).toBeEnabled();
  expect(await page.evaluate(() => (window as any).fixture.writes)).toEqual([]);
  await page.evaluate(() => { (window as any).fixture.hold = true; });
  await page.getByRole("button", { name: "new-account@example.test kodunu kopyala" }).click();
  await page.getByRole("button", { name: "Unmount fixture" }).click();
  await page.evaluate(() => { const f = (window as any).fixture; f.hold = false; f.release(); });
  expect(await page.evaluate(() => (window as any).fixture.writes)).toEqual([]);
});

test("lock errors hide codes and setup; stale contexts never silently bootstrap", async ({ page }) => {
  await page.goto(fixture);
  await expect(code(page)).toBeEnabled();
  await page.getByRole("button", { name: "Hesap ekle", exact: true }).click();
  await page.getByLabel("Kurulum anahtarı veya otpauth adresi").fill(publicSeed);
  await page.evaluate(() => { (window as any).fixture.fail = true; });
  // A native lock can arrive while a form is open; trigger its next refresh directly.
  await page.getByRole("button", { name: "Kodları yenile" }).evaluate((button) => button.click());
  await expect(page.getByRole("alert")).toHaveText("Unlock a vault first");
  await expect(code(page)).toBeDisabled();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.evaluate(() => { const f = (window as any).fixture; f.fail = false; f.scope = "different"; });
  await page.getByRole("button", { name: "Kodları yenile" }).click();
  await expect(page.getByRole("alert")).toHaveText("Vault changed; reopen Authenticator");
  expect(await page.evaluate(() => (window as any).fixture.calls.slice(1).every((entry: any) => entry.context === "personal-a"))).toBe(true);
});

test("manual provisioning, URI import and deletion carry correct native contracts", async ({ page }) => {
  await page.goto(fixture);
  await expect(code(page)).toBeEnabled();
  await page.getByRole("button", { name: "Hesap ekle", exact: true }).click();
  await page.getByLabel("Kurulum anahtarı veya otpauth adresi").fill(publicSeed);
  await page.getByLabel(/^Hesap adı/).fill("disposable@example.test");
  await page.getByRole("combobox", { name: "Algoritma", exact: true }).selectOption("SHA512");
  await page.getByRole("combobox", { name: "Hane", exact: true }).selectOption("8");
  await page.getByLabel("Süre (saniye)", { exact: true }).fill("60");
  await page.getByRole("button", { name: "Hesabı kaydet" }).click();
  expect(await page.evaluate(() => { const input = (window as any).fixture.lastAdd; return [input.provisioning === "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", input.algorithm, input.digits, input.period]; })).toEqual([true, "SHA512", 8, 60]);
  await page.getByRole("button", { name: "disposable@example.test hesabını sil" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Hesabı sil", exact: true }).click();
  expect(await page.evaluate(() => (window as any).fixture.lastDelete)).toEqual({ id: "added", revision: 1 });
  await page.getByRole("button", { name: "Hesap ekle", exact: true }).click();
  await page.getByLabel("Kurulum anahtarı veya otpauth adresi").fill(`otpauth://totp/Example:qr%40example.test?secret=${publicSeed}`);
  await expect(page.getByLabel(/^Hesap adı/)).toHaveCount(0);
  await page.getByRole("button", { name: "Hesabı kaydet" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("refresh removes a revealed setup URI when its shared record revision changes", async ({ page }) => {
  await page.goto(`${fixture}?mode=team`);
  await page.getByRole("button", { name: "operations@example.test kurulum sırrını göster" }).click();
  await expect(page.getByRole("textbox", { name: "TOTP kurulum adresi" })).toHaveValue(/^otpauth:\/\/totp\//);
  await page.evaluate(() => { (window as any).fixture.revisions.team = 3; });
  await page.getByRole("button", { name: "Kodları yenile" }).evaluate((button) => button.click());
  await expect(page.getByRole("dialog", { name: "TOTP kurulum sırrı" })).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "TOTP kurulum adresi" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "operations@example.test kurulum sırrını göster" })).toBeEnabled();
});

test("local QR reader accepts public TOTP fixture and rejects migration, blank, damaged and oversize images", async ({ page }) => {
  await page.goto(fixture);
  await expect(code(page)).toBeEnabled();
  await page.getByRole("button", { name: "Hesap ekle", exact: true }).click();
  const file = page.getByLabel("TOTP QR görseli");
  await file.setInputFiles(path.resolve("tests/ui/authenticator-fixtures/rfc6238.png"));
  await expect(page.getByRole("status")).toContainText("QR kodu cihazda okundu");
  await expect(page.getByRole("button", { name: "Hesabı kaydet" })).toBeEnabled();
  await file.setInputFiles(path.resolve("tests/ui/authenticator-fixtures/google-migration.png"));
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText("Google Authenticator taşıma QR");
  await expect(page.getByRole("button", { name: "Hesabı kaydet" })).toBeDisabled();
  await file.setInputFiles({ name: "broken.png", mimeType: "image/png", buffer: Buffer.from("invalid PNG") });
  await expect(page.getByRole("dialog").getByRole("alert")).toHaveText("QR görseli açılamadı.");
  await file.setInputFiles({ name: "large.png", mimeType: "image/png", buffer: Buffer.alloc(4 * 1024 * 1024 + 1) });
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText("En fazla 4 MB");
  const blank = await page.evaluate(() => { const canvas = document.createElement("canvas"); canvas.width = 100; canvas.height = 100; return canvas.toDataURL("image/png").split(",")[1]; });
  await file.setInputFiles({ name: "blank.png", mimeType: "image/png", buffer: Buffer.from(blank, "base64") });
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText("okunabilir bir QR kodu bulunamadı");
  const tooWide = await page.evaluate(() => { const canvas = document.createElement("canvas"); canvas.width = 4097; canvas.height = 1; return canvas.toDataURL("image/png").split(",")[1]; });
  await file.setInputFiles({ name: "wide.png", mimeType: "image/png", buffer: Buffer.from(tooWide, "base64") });
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText("4096 × 4096");
  await file.setInputFiles(path.resolve("tests/ui/authenticator-fixtures/rfc6238.png"));
  await page.getByRole("button", { name: "Hesabı kaydet" }).click();
  expect(await page.evaluate(() => (window as any).fixture.lastAdd.provisioning.startsWith("otpauth://totp/"))).toBe(true);
});
