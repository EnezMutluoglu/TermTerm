import { test, expect } from "@playwright/test";
test("onboarding, host search, group navigation and editor layout", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Your workspace starts here" }),
  ).toBeVisible();
  await page.screenshot({ path: "artifacts/onboarding.png" });
  await page.goto("/?preview");
  await expect(
    page.getByRole("heading", { name: "Hosts", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".record-card")).toHaveCount(0);
  await expect(page.locator(".group-main")).toHaveCount(2);
  await expect(
    page.locator(".record-card h3").filter({ hasText: "web-01" }),
  ).toHaveCount(0);
  await page.screenshot({ path: "artifacts/hosts.png" });
  await page.locator(".group-main").filter({ hasText: "Production" }).dblclick();
  await expect(page.locator(".record-card")).toHaveCount(3);
  await expect(page.locator(".breadcrumb")).toContainText("Production");
  await page.getByRole("textbox", { name: "Search records" }).fill("database");
  await expect(page.locator(".record-card")).toHaveCount(1);
  await page.getByRole("textbox", { name: "Search records" }).fill("");
  await page.locator(".record-card").filter({hasText:"web-01"}).click({button:"right"});
  await page.getByRole("menuitem", {name:"Edit", exact:true}).click();
  await expect(
    page
      .locator(".editor")
      .getByRole("heading", { name: "web-01", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Advanced", exact: true }).click();
  await expect(page.getByText("Host chain", { exact: true })).toBeVisible();
  await page.screenshot({ path: "artifacts/host-chain.png" });
  await page.getByRole("button", { name: "Close editor" }).click();
  await page.getByRole("button", { name: "List view" }).click();
  await expect(page.locator(".records-grid")).toHaveClass(/list/);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "PostgreSQL sync" }),
  ).toBeVisible();
  await page.screenshot({ path: "artifacts/postgresql-settings.png" });
  expect(errors).toEqual([]);
});
test("1366px layout has no horizontal page overflow", async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 768 });
  await page.goto("/?preview");
  await expect(page.locator(".group-main")).toHaveCount(2);
  await expect(page.locator(".record-card")).toHaveCount(0);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBeTruthy();
  await page.screenshot({ path: "artifacts/hosts-1366.png" });
});
test("folder navigation hides nested hosts and terminal themes preview immediately", async ({
  page,
}) => {
  await page.goto("/?preview");
  await expect(page.locator(".record-card")).toHaveCount(0);
  await page.locator(".group-main").filter({ hasText: "Production" }).dblclick();
  await expect(
    page.getByRole("heading", { name: "Production", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".record-card")).toHaveCount(3);
  await expect(
    page.locator(".record-card h3").filter({ hasText: "Development" }),
  ).toHaveCount(0);
  await page
    .locator(".breadcrumb")
    .getByRole("button", { name: "Hosts", exact: true })
    .click();
  await expect(page.locator(".record-card")).toHaveCount(0);
  await page.locator(".group-main").filter({ hasText: "Ungrouped" }).dblclick();
  await expect(
    page.locator(".record-card h3").filter({ hasText: "Development" }),
  ).toHaveCount(1);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "General", exact: true }).click();
  await page.getByLabel("Terminal color theme").selectOption("forest");
  await expect(page.locator(".terminal-sample")).toHaveCSS(
    "background-color",
    "rgb(16, 32, 28)",
  );
  await page.getByLabel("Terminal color theme").selectOption("paper");
  await expect(page.locator(".terminal-sample")).toHaveCSS(
    "background-color",
    "rgb(241, 238, 230)",
  );
  await page.screenshot({ path: "artifacts/terminal-themes.png" });
});
test("10,000-host search renders a matching result promptly", async ({
  page,
}) => {
  await page.goto("/?preview&previewCount=10000");
  await page.locator(".group-main").filter({ hasText: "Ungrouped" }).dblclick();
  await expect(page.locator(".records-toolbar")).toContainText(
    "9997 hosts in this folder",
  );
  await expect(page.locator(".record-card")).toHaveCount(120);
  const elapsed = await page.evaluate(async () => {
    const input = document.querySelector<HTMLInputElement>(
      'input[aria-label="Search records"]',
    )!;
    const start = performance.now();
    return await new Promise<number>((resolve) => {
      const observer = new MutationObserver(() => {
        const cards = document.querySelectorAll(".record-card");
        if (
          cards.length === 1 &&
          cards[0].textContent?.includes("Benchmark host 9999")
        ) {
          observer.disconnect();
          requestAnimationFrame(() => resolve(performance.now() - start));
        }
      });
      observer.observe(document.querySelector(".records-grid")!, {
        childList: true,
        subtree: true,
      });
      Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )!.set!.call(input, "Benchmark host 9999");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  });
  console.log("10,000-host search latency (ms):", elapsed.toFixed(1));
  expect(elapsed).toBeLessThan(150);
  await page.screenshot({ path: "artifacts/search-10000.png" });
});

test("parent folder totals include descendants while each folder lists only its own hosts", async ({
  page,
}) => {
  await page.goto("/?preview&previewNested");
  await expect(page.locator(".records-toolbar")).toContainText(
    "2 folders · 6 hosts total",
  );
  const parent = page
    .locator(".group-main")
    .filter({ hasText: "Environments" });
  await expect(parent).toContainText("3 hosts");
  await expect(
    page
      .locator(".sidebar-group")
      .filter({ hasText: "Environments" })
      .locator(".sidebar-group-count"),
  ).toHaveText("3");
  await parent.dblclick();
  await expect(page.locator(".record-card")).toHaveCount(0);
  await expect(page.locator(".records-toolbar")).toContainText(
    "0 hosts in this folder · 1 folders",
  );
  await expect(
    page.locator(".group-main").filter({ hasText: "Production" }),
  ).toContainText("3 hosts");
  await page.locator(".group-main").filter({ hasText: "Production" }).dblclick();
  await expect(page.locator(".record-card")).toHaveCount(3);
  await expect(page.locator(".records-toolbar")).toContainText(
    "3 hosts in this folder",
  );
  await page
    .locator(".breadcrumb")
    .getByRole("button", { name: "Hosts", exact: true })
    .click();
  await expect(page.locator(".record-card")).toHaveCount(0);
  await expect(
    page.locator(".record-card h3").filter({ hasText: "web-01" }),
  ).toHaveCount(0);
});
