import { expect, test } from "@playwright/test";
import { mkdir } from "node:fs/promises";
const output = "artifacts/vpn-ui";
test.beforeEach(async ({ page }) => {
  await page.goto("/?preview&previewVpn");
  await expect(
    page.getByRole("heading", { name: "VPN", exact: true }),
  ).toBeVisible();
});
test("VPN overview, folders and simultaneous connection preparation are truthful", async ({
  page,
}) => {
  await mkdir(output, { recursive: true });
  await expect(page.locator(".vpn-card")).toHaveCount(3);
  await expect(page.locator(".vpn-disconnected")).toHaveText([
    "Bağlı değil",
    "Bağlı değil",
    "Bağlı değil",
  ]);
  await page.screenshot({ path: `${output}/01-vpn-overview.png` });
  await page.getByRole("button", { name: "3 profili kontrol et" }).click();
  const dialog = page.getByRole("dialog", { name: "Bağlantı hazırlığı" });
  await expect(dialog.locator(".vpn-preflight-row")).toHaveCount(3);
  await expect(
    dialog.getByText("Bağlantı başlamadı", { exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: `${output}/04-vpn-parallel-check.png` });
  await dialog.getByRole("button", { name: "Anladım" }).click();
  await page.getByRole("button", { name: /^Müşteriler 1$/ }).click();
  await expect(page.locator(".vpn-card")).toHaveCount(1);
  await page.getByRole("button", { name: /^Tüm profiller 3$/ }).click();
  await page.getByRole("textbox", { name: "VPN ara" }).fill("GlobalProtect");
  await expect(page.locator(".vpn-card")).toHaveCount(1);
});
test("OpenVPN file selection is transient and profile types can be switched", async ({
  page,
}) => {
  await page
    .getByRole("button", { name: "Dosyadan ekle", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "VPN profili ekle / düzenle",
  });
  await dialog
    .getByRole("textbox", { name: "Profil adı" })
    .fill("İzole test bağlantısı");
  await dialog
    .getByLabel("VPN dosyası", { exact: true })
    .setInputFiles({
      name: "istanbul-test.ovpn",
      mimeType: "text/plain",
      buffer: Buffer.from("client\nremote vpn.example 1194\n"),
    });
  await expect(
    dialog.getByText("istanbul-test.ovpn", { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByText(/içerik doğrulaması ve şifreli import henüz bağlı değil/),
  ).toBeVisible();
  await page.screenshot({ path: `${output}/02-vpn-file-import.png` });
  await dialog.getByRole("button", { name: "Taslağı ekle" }).click();
  await expect(page.locator(".vpn-card")).toHaveCount(4);
  await page.reload();
  await expect(page.locator(".vpn-card")).toHaveCount(3);
});
test("Fortinet and GlobalProtect form validates HTTPS and never asks to store OTP", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Yeni VPN", exact: true }).click();
  const dialog = page.getByRole("dialog", {
    name: "VPN profili ekle / düzenle",
  });
  await dialog
    .getByRole("textbox", { name: "Profil adı" })
    .fill("Firma · Fortinet");
  await dialog
    .getByRole("textbox", { name: "VPN sunucusu" })
    .fill("http://vpn.example");
  await dialog.getByRole("button", { name: "Taslağı ekle" }).click();
  await expect(dialog.getByRole("alert")).toContainText("HTTPS");
  await dialog
    .getByRole("textbox", { name: "VPN sunucusu" })
    .fill("https://vpn.firma.example:443");
  await page.screenshot({ path: `${output}/03-vpn-fortinet-profile.png` });
  await dialog.getByRole("button", { name: /GlobalProtect Portal/ }).click();
  await expect(
    dialog.getByRole("textbox", { name: "Portal / gateway adresi" }),
  ).toBeVisible();
  await expect(dialog.locator('input[type="password"]')).toHaveCount(0);
  await page.screenshot({ path: `${output}/05-vpn-globalprotect-profile.png` });
  await dialog.getByRole("button", { name: "Taslağı ekle" }).click();
  await expect(page.locator(".vpn-card")).toHaveCount(4);
});
test("narrow layout and unsupported file selection", async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 768 });
  await expect(page.locator(".vpn-heading")).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBeTruthy();
  await page
    .getByRole("button", { name: "Dosyadan ekle", exact: true })
    .click();
  await page
    .getByLabel("VPN dosyası", { exact: true })
    .setInputFiles({
      name: "script.sh",
      mimeType: "text/plain",
      buffer: Buffer.from("echo test"),
    });
  await expect(page.getByRole("alert")).toContainText(".ovpn, .conf veya .xml");
});
