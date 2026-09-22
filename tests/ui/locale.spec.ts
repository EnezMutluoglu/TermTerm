import { test, expect } from "@playwright/test";

test.use({ storageState: { cookies: [], origins: [] } });

test("Turkish is the default on welcome, unlock and remembered startup", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("lang", "tr");
  await page.getByRole("button", { name: /Yerel kasa oluştur/ }).click();
  await expect(
    page.getByLabel("Kasa parolası", { exact: false }),
  ).toBeVisible();
  await expect(
    page.getByRole("checkbox", { name: /Parolayı .* hatırla/ }),
  ).toBeVisible();
  await page.screenshot({ path: "artifacts/review-0.3.4/turkish-welcome.png" });
  await page.goto("/tests/ui/remember-harness.html");
  await page
    .getByLabel("Kasa parolası", { exact: false })
    .fill("Disposable-test-password");
  await page.getByRole("checkbox", { name: /Parolayı .* hatırla/ }).check();
  await page.getByRole("button", { name: "Kasanın kilidini aç", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Sunucular", exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Sunucular", exact: true }),
  ).toBeVisible();
});

test("translated menus and settings preserve user labels, addresses and command bytes", async ({
  page,
}) => {
  const fixture = [
    {
      id: "folder",
      kind: "group",
      data: { label: "Production" },
      updatedAt: 1,
    },
    {
      id: "server",
      kind: "host",
      data: {
        label: "Connect",
        address: "api.example.test",
        groupId: "folder",
      },
      updatedAt: 1,
    },
    {
      id: "snippet",
      kind: "snippet",
      data: { label: "Run", command: "echo 'Connection'" },
      updatedAt: 1,
    },
  ];
  await page.addInitScript((records) => {
    (window as any).recordsFixture = records;
  }, fixture);
  await page.goto("/tests/ui/terminal-harness.html");
  const folder = page.locator(".group-main").filter({ hasText: "Production" });
  await folder.dblclick();
  const host = page.locator('.record-card[aria-label="Connect"]');
  await host.click({ button: "right" });
  await expect(
    page.getByRole("menuitem", { name: "Bağlan", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("menuitem", { name: /Düzenle/ })).toBeVisible();
  await page.keyboard.press("Escape");
  await host.click();
  await expect(page.locator(".record-details")).toContainText(
    "api.example.test",
  );
  await page.getByTitle("Yeni yerel terminal", { exact: true }).click();
  await expect(page.locator(".terminal-status")).toHaveText("Bağlı");
  await page.locator(".terminal-surface").click();
  await page.keyboard.type("echo Connection");
  expect(
    await page.evaluate(() =>
      (window as any).inputs
        .filter((v: any) => v.data)
        .map((v: any) => v.data)
        .join(""),
    ),
  ).toBe("echo Connection");
  await page
    .getByRole("button", { name: /Güncellemeler/ })
    .first()
    .click();
  await expect(
    page.getByRole("button", { name: "Güncellemeleri denetle" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Uygulama güncellemeleri" }),
  ).toBeVisible();
  expect(await page.evaluate(() => (window as any).testVault.records)).toEqual(
    fixture,
  );
  await page.screenshot({ path: "artifacts/review-0.3.4/turkish-updates.png" });
});
