import { test, expect } from "@playwright/test";
const size = (page: any, id = "terminal-1") =>
  page.evaluate(
    (id: string) =>
      (window as any).inputs
        .filter((r: any) => r.id === id && r.cols && r.rows)
        .at(-1),
    id,
  );
async function fills(page: any) {
  await expect
    .poll(async () =>
      page
        .locator(".terminal-pane.focused .terminal-surface")
        .evaluate((el) => {
          const screen = el
            .querySelector(".xterm-screen")!
            .getBoundingClientRect();
          const box = el.getBoundingClientRect();
          const css = getComputedStyle(el);
          return (
            box.height -
            parseFloat(css.paddingTop) -
            parseFloat(css.paddingBottom) -
            screen.height
          );
        }),
    )
    .toBeLessThan(26);
}
test("PTY size is resent at connection and follows grow/shrink, hidden tabs, Stats and split layouts", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1100, height: 720 });
  await page.goto("/tests/ui/terminal-harness.html");
  await page.getByTitle("New local terminal", { exact: true }).click();
  await expect(page.locator(".terminal-status")).toHaveText("Connected");
  await expect.poll(() => size(page)).toMatchObject({ id: "terminal-1" });
  await fills(page);
  const small = await size(page);
  await page.evaluate(() => {
    (window as any).inputs = [];
    (window as any).emitEvent("session-event", {
      id: "terminal-1",
      kind: "connected",
    });
  });
  await expect
    .poll(() => size(page))
    .toMatchObject({ cols: small.cols, rows: small.rows });
  await page.setViewportSize({ width: 1920, height: 1080 });
  await expect
    .poll(async () => (await size(page)).rows)
    .toBeGreaterThan(small.rows);
  await fills(page);
  const big = await size(page);
  expect(big.cols).toBeGreaterThan(small.cols);
  await page.getByRole("button", { name: "Vault", exact: true }).click();
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.locator(".session-tab-label").first().click();
  await expect.poll(async () => (await size(page)).cols).toBeLessThan(big.cols);
  await fills(page);
  const stats = await size(page);
  await page.getByRole("checkbox", { name: "Show resource monitor" }).uncheck();
  await expect
    .poll(async () => (await size(page)).rows)
    .toBeGreaterThanOrEqual(stats.rows);
  await fills(page);
  await page.getByTitle("New local terminal", { exact: true }).click();
  await page.getByTitle("Split terminals", { exact: true }).click();
  await expect
    .poll(async () => (await size(page, "terminal-2")).cols)
    .toBeLessThan(stats.cols);
  await fills(page);
  await page.getByTitle("Focus terminal", { exact: true }).click();
  await fills(page);
  await page.setViewportSize({ width: 5200, height: 1200 });
  await expect
    .poll(async () => (await size(page, "terminal-2")).cols)
    .toBeGreaterThan(500);
});
