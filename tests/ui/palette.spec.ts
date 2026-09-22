import { test, expect, type Locator } from "@playwright/test";

async function background(element: Locator) {
  return element.evaluate((el) => getComputedStyle(el).backgroundColor);
}

// Measure rendered foreground against its actual opaque ancestor surface.
async function contrast(element: Locator) {
  return element.evaluate((el) => {
    const rgb = (value: string) => value.match(/[\d.]+/g)!.map(Number);
    const luminance = (values: number[]) =>
      values.slice(0, 3).reduce((sum, value, i) => {
        const n = value / 255;
        return (
          sum +
          (n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4) *
            [0.2126, 0.7152, 0.0722][i]
        );
      }, 0);
    let parent: Element | null = el;
    let bg: number[] = [];
    while (parent) {
      bg = rgb(getComputedStyle(parent).backgroundColor);
      if (bg.length === 3 || bg[3] === 1) break;
      parent = parent.parentElement;
    }
    if (!parent) throw new Error("No opaque surface for contrast measurement");
    const a = luminance(rgb(getComputedStyle(el).color)),
      b = luminance(bg);
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  });
}

test("host, navigation and SFTP selections share a readable visual state", async ({
  page,
}) => {
  await page.goto("/");
  await page.screenshot({ path: "artifacts/palette-review/onboarding.png" });
  await page.goto("/?preview");
  await page.locator(".group-main").filter({ hasText: "Production" }).dblclick();
  await page
    .locator(".record-card")
    .first()
    .click({ modifiers: ["Control"] });
  await page.mouse.move(5, 5);
  const nav = page.locator(".main-nav button.active");
  const selected = page.locator(".record-card.selected");
  const color = await background(nav);
  await expect.poll(() => background(selected)).toBe(color);
  for (const locator of [
    nav,
    selected.locator("h3"),
    selected.locator(".card-title > span"),
    page.locator(".page-heading .primary"),
  ]) {
    expect(await contrast(locator)).toBeGreaterThanOrEqual(4.5);
  }
  await page.screenshot({ path: "artifacts/palette-review/hosts.png" });
  await page.locator(".record-card").filter({hasText:"web-01"}).click({button:"right"});
  await page.getByRole("menuitem", {name:"Edit", exact:true}).click();
  await page.getByRole("button", { name: "Advanced", exact: true }).click();
  const groupChain = page.getByRole("button", {
    name: "Use group chain",
    exact: true,
  });
  await expect(groupChain).toBeVisible();
  expect(await contrast(groupChain)).toBeGreaterThanOrEqual(4.5);
  await page.screenshot({ path: "artifacts/palette-review/host-chain.png" });

  await page.goto("/tests/ui/terminal-harness.html");
  await expect(
    page.getByRole("heading", { name: "Hosts", exact: true }),
  ).toBeVisible();
  await page.evaluate(() => {
    const w = window as any,
      original = w.__TAURI_INTERNALS__.invoke;
    w.__TAURI_INTERNALS__.invoke = async (command: string, args: any) =>
      command === "file_list"
        ? [
            {
              name: "Projects",
              path: "/home/demo/Projects",
              directory: true,
              symlink: false,
              size: 0,
              modified: 0,
            },
            {
              name: "README.md",
              path: "/home/demo/README.md",
              directory: false,
              symlink: false,
              size: 2048,
              modified: 0,
            },
          ]
        : original(command, args);
  });
  await page
    .locator(".top-tabs")
    .getByRole("button", { name: "SFTP", exact: true })
    .click();
  await page.locator(".file-row").filter({ hasText: "README.md" }).click();
  await page.mouse.move(5, 5);
  const file = page.locator(".file-row.selected");
  await expect.poll(() => background(file)).toBe(color);
  expect(await contrast(file.locator(".file-name"))).toBeGreaterThanOrEqual(
    4.5,
  );
  await page.screenshot({ path: "artifacts/palette-review/sftp.png" });
});

test("resource graphs use related blues and a full disk keeps its error indication", async ({
  page,
}) => {
  await page.goto("/tests/ui/terminal-harness.html");
  await page.getByTitle("New local terminal", { exact: true }).click();
  await expect(page.locator(".terminal-status")).toHaveText("Connected");
  for (let n = 0; n < 12; n++) {
    await page.evaluate((n) => {
      const w = window as any,
        time = Date.now() - (11 - n) * 2000,
        gib = 1024 ** 3;
      w.emitEvent("session-metrics", {
        sessionId: "terminal-1",
        source: "UI fixture",
        os: "linux",
        timestamp: time,
        sampledAt: time,
        disksAt: time,
        cpuPercent: 24 + (n % 4) * 7,
        memory: {
          used: (5 + n / 20) * gib,
          available: 10 * gib,
          total: 16 * gib,
        },
        disks: [
          {
            mountPoint: "/",
            device: "/dev/sda1",
            filesystem: "ext4",
            total: 100 * gib,
            used: 94 * gib,
            available: 6 * gib,
            root: true,
            virtual: false,
          },
        ],
        error: null,
        diskError: null,
        supported: true,
      });
    }, n);
    await expect(page.locator(".resource-cpu b")).toHaveText(
      `${24 + (n % 4) * 7}%`,
    );
  }
  for (const locator of [
    page.locator(".resource-cpu .resource-spark"),
    page.locator(".resource-memory .resource-spark"),
  ]) {
    const rgb = await locator.evaluate((el) =>
      getComputedStyle(el).color.match(/\d+/g)!.map(Number),
    );
    expect(rgb[2]).toBeGreaterThan(rgb[1]);
    expect(rgb[1]).toBeGreaterThan(rgb[0]);
    expect(await contrast(locator)).toBeGreaterThanOrEqual(4.5);
  }
  const summary = page.locator(".resource-disk .capacity-track i");
  await expect(summary).toHaveClass("critical");
  await page.locator(".resource-disk").click();
  await expect(page.locator(".resource-mount .capacity-track i")).toHaveCSS(
    "background-color",
    await background(summary),
  );
  await page.screenshot({
    path: "artifacts/palette-review/terminal-stats.png",
  });
  await page.getByRole("button", { name: /^Updates v/ }).click();
  await page.screenshot({ path: "artifacts/palette-review/updates.png" });
  await page.getByRole("button", { name: "General", exact: true }).click();
  const navigationColor = await page
    .locator(".main-nav button")
    .first()
    .evaluate((el) => getComputedStyle(el).color);
  await page.getByLabel("Terminal color theme").selectOption("forest");
  // Choosing a green ANSI theme must not recolor the application chrome.
  await expect(page.locator(".main-nav button").first()).toHaveCSS(
    "color",
    navigationColor,
  );
  await page.getByLabel("Terminal color theme").selectOption("graphite");
  await page.screenshot({ path: "artifacts/palette-review/appearance.png" });
});
