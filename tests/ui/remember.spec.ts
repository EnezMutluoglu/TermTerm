import { test, expect, type Page } from "@playwright/test";
const password = "Disposable-test-password";
const field = (page: Page) =>
  page.getByLabel("Vault password", { exact: false });
const remember = (page: Page) =>
  page.getByRole("checkbox", { name: /Remember password using/ });
const hosts = (page: Page) =>
  page.getByRole("heading", { name: "Hosts", exact: true });
const form = (page: Page) =>
  page.getByRole("heading", { name: "Welcome back" });
async function open(page: Page, query = "") {
  await page.goto("/tests/ui/remember-harness.html" + query);
  await expect(form(page)).toBeVisible();
  await expect(field(page)).toBeEnabled();
}
async function save(page: Page) {
  await open(page);
  await field(page).fill(password);
  await remember(page).check();
  await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  await expect(hosts(page)).toBeVisible();
}
test("remember survives a fresh UI startup and StrictMode opens it only once", async ({
  page,
}) => {
  await save(page);
  await page.reload();
  await expect(hosts(page)).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter(
          (c: any) => c.command === "vault_try_open_remembered",
        ).length,
    ),
  ).toBe(1);
  expect(
    await page.evaluate(() => localStorage.getItem("fixture-remembered")),
  ).toBe("true");
});
test("explicit lock stays locked; saved state is checked and primary unlock needs no password", async ({
  page,
}) => {
  await save(page);
  await page.getByTitle("Lock vault", { exact: true }).click();
  await expect(form(page)).toBeVisible();
  await expect(remember(page)).toBeChecked();
  await expect(field(page)).toHaveValue("");
  await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  await expect(hosts(page)).toBeVisible();
  expect(
    await page.evaluate(() =>
      (window as any).calls.some(
        (c: any) => c.command === "vault_remember" && c.enabled === false,
      ),
    ),
  ).toBe(false);
});
test("manually entering a remembered password does not erase it", async ({
  page,
}) => {
  await save(page);
  await page.getByTitle("Lock vault", { exact: true }).click();
  await expect(remember(page)).toBeChecked();
  await field(page).fill(password);
  await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  await expect(hosts(page)).toBeVisible();
  await page.reload();
  await expect(hosts(page)).toBeVisible();
});
test("explicitly unchecking forgets the password and next startup requires it", async ({
  page,
}) => {
  await save(page);
  await page.getByTitle("Lock vault", { exact: true }).click();
  await expect(remember(page)).toBeChecked();
  await remember(page).uncheck();
  await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  await expect(hosts(page)).toBeVisible();
  await page.reload();
  await expect(form(page)).toBeVisible();
  await expect(remember(page)).not.toBeChecked();
  await field(page).fill("wrong");
  await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  await expect(
    page.getByText("Incorrect vault password", { exact: true }),
  ).toBeVisible();
  await field(page).fill(password);
  await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  await expect(hosts(page)).toBeVisible();
  expect(
    await page.evaluate(() => localStorage.getItem("fixture-remembered")),
  ).toBe("false");
});
test("damaged saved credentials fall back to manual unlocking", async ({
  page,
}) => {
  await open(page, "?broken");
  await expect(
    page.getByText(/Could not use the remembered password/),
  ).toBeVisible();
  await field(page).fill(password);
  await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  await expect(hosts(page)).toBeVisible();
  expect(
    await page.evaluate(() =>
      (window as any).calls.some((c: any) => c.command === "vault_remember"),
    ),
  ).toBe(false);
});
test("secure-store failure does not ask for the password a second time", async ({
  page,
}) => {
  await open(page, "?save-failure");
  await field(page).fill(password);
  await remember(page).check();
  await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  await page
    .getByRole("button", { name: "Continue without remembering password" })
    .click();
  await expect(hosts(page)).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.command === "vault_open")
          .length,
    ),
  ).toBe(1);
});
