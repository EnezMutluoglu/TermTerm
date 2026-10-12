import { test, expect, type Page } from "@playwright/test";
async function open(page: Page, count = 1) {
  await page.goto("/tests/ui/team-terminal-harness.html");
  for (let i = 0; i < count; i++)
    await page
      .getByRole("button", { name: "Test bağlantısı", exact: true })
      .click();
  await expect(page.locator(".terminal-status").last()).toHaveText("Connected");
  await page.locator(".terminal-pane.focused .terminal-surface").click();
}
test("Team top tabs retain output and terminal size, context menu returns focus, lock closes sessions", async ({
  page,
}) => {
  await open(page, 2);
  await expect(page.locator(".top-session-tab")).toHaveCount(2);
  await expect(
    page.getByRole("toolbar", { name: "Terminal araçları" }),
  ).toBeVisible();
  const before = await page
    .locator(".terminal-pane.focused")
    .getAttribute("data-session-id");
  await page.getByRole("button", { name: "SFTP gezinme" }).click();
  await expect(page.getByText("SFTP görünümü")).toBeVisible();
  await page.locator(".session-tab-label").last().click();
  await expect(page.locator(".terminal-pane.focused")).toHaveAttribute(
    "data-session-id",
    before!,
  );
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((x: any) => x.command === "team_v2_start")
          .length,
    ),
  ).toBe(2);
  await page.locator(".session-tab-label").first().click({ button: "right" });
  await page
    .getByRole("menuitem", { name: "Bölünmüş görünüm", exact: true })
    .click();
  await expect(page.locator(".terminal-pane:visible")).toHaveCount(2);
  await expect
    .poll(
      async () =>
        await page.evaluate(
          () =>
            (window as any).inputs.filter((x: any) => x.cols && x.rows > 8)
              .length,
        ),
    )
    .toBeGreaterThan(1);
  await page.locator(".terminal-pane.focused .terminal-surface").click();
  await page.keyboard.press("Control+Shift+L");
  await expect(page.getByText("Team kilitlendi")).toBeVisible();
  await expect
    .poll(
      async () =>
        await page.evaluate(
          () => (window as any).inputs.filter((x: any) => x.close).length,
        ),
    )
    .toBe(2);
});
test("Team broadcast uses ordered writes, keeps shell shortcuts, and stops at closed terminals", async ({
  page,
}) => {
  await open(page, 2);
  await page.keyboard.press("Control+Shift+B");
  await expect(
    page.getByRole("button", { name: "Toplu giriş", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("status")).toContainText("2 terminale");
  await page.evaluate(() => {
    (window as any).inputs = [];
  });
  await page.keyboard.type("abc");
  await page.keyboard.press("Control+k");
  await expect
    .poll(
      async () =>
        await page.evaluate(() =>
          [1, 2].map((i) =>
            (window as any).inputs
              .filter((x: any) => x.id === `terminal-${i}`)
              .map((x: any) => x.data ?? "")
              .join(""),
          ),
        ),
    )
    .toEqual(["abc\u000b", "abc\u000b"]);
  await page.evaluate(() => {
    (window as any).emitEvent("session-event", {
      id: "terminal-1",
      kind: "closed",
    });
  });
  await expect(
    page.getByRole("button", { name: "Toplu giriş", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Toplu giriş", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  await page.evaluate(() => {
    (window as any).inputs = [];
    (window as any).holdInput = true;
  });
  await page.keyboard.type("123");
  await expect
    .poll(
      async () =>
        await page.evaluate(
          () => (window as any).inputs.filter((x: any) => x.data).length,
        ),
    )
    .toBe(1);
  await page.evaluate(() => {
    (window as any).holdInput = false;
    (window as any).releaseInput();
  });
  await expect
    .poll(
      async () =>
        await page.evaluate(() =>
          (window as any).inputs
            .filter((x: any) => x.data)
            .map((x: any) => x.data)
            .join(""),
        ),
    )
    .toBe("123");
  expect(
    await page.evaluate(() =>
      (window as any).inputs
        .filter((x: any) => x.data)
        .every((x: any) => x.id === "terminal-2"),
    ),
  ).toBe(true);
});
test("Team clipboard paste preserves bracketed mode and drops queued input after close", async ({
  page,
}) => {
  await open(page);
  await page.evaluate(() => {
    (window as any).emitTerminal("terminal-1", "\u001b[?2004h");
    (window as any).clipboardText = "Türkçe\nçok satır";
  });
  // xterm parses remote control bytes asynchronously; wait for the rendering frames before simulating input.
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  await page.keyboard.press("Shift+Insert");
  await expect
    .poll(
      async () =>
        await page.evaluate(() =>
          (window as any).inputs
            .filter((x: any) => x.data)
            .map((x: any) => x.data)
            .join(""),
        ),
    )
    .toBe("\u001b[200~Türkçe\rçok satır\u001b[201~");
  await page.evaluate(() => {
    (window as any).inputs = [];
    (window as any).holdInput = true;
  });
  await page.keyboard.type("xyz");
  await page.keyboard.press("Control+Shift+W");
  await expect(page.getByText("Hosts görünümü")).toBeVisible();
  await page.evaluate(() => {
    (window as any).holdInput = false;
    (window as any).releaseInput();
  });
  expect(
    await page.evaluate(() =>
      (window as any).inputs
        .filter((x: any) => x.data)
        .map((x: any) => x.data)
        .join(""),
    ),
  ).toBe("x");
});
