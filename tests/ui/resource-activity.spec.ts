import { test, expect } from "@playwright/test";
async function emit(page: any, overrides: any = {}) {
  await page.evaluate((overrides: any) => {
    const time = Date.now(),
      gib = 1024 ** 3;
    (window as any).emitEvent("session-metrics", {
      sessionId: "terminal-1",
      source: "Linux lab",
      os: "linux",
      timestamp: time,
      sampledAt: time,
      disksAt: time,
      activityAt: time,
      cpuPercent: 25,
      memory: { total: 16 * gib, used: 4 * gib, available: 12 * gib },
      supported: true,
      error: null,
      diskError: null,
      activityError: null,
      disks: [
        {
          mountPoint: "/",
          device: "/dev/sda1",
          filesystem: "ext4",
          total: 100 * gib,
          used: 30 * gib,
          available: 70 * gib,
          root: true,
          virtual: false,
        },
        {
          mountPoint: "/mnt/Veri",
          device: "/dev/sdb1",
          filesystem: "ext4",
          total: 100 * gib,
          used: 90 * gib,
          available: 10 * gib,
          root: false,
          virtual: false,
        },
        {
          mountPoint: "/mnt/Arşiv",
          device: "/dev/sdc1",
          filesystem: "ext4",
          total: 100 * gib,
          used: 95 * gib,
          available: 0,
          root: false,
          virtual: false,
        },
        {
          mountPoint: "/run",
          device: "tmpfs",
          filesystem: "tmpfs",
          total: gib,
          used: gib,
          available: 0,
          root: false,
          virtual: true,
        },
      ],
      activity: {
        network: [
          {
            name: "eth0",
            readBytesPerSecond: 1_000_000,
            writeBytesPerSecond: 2_000_000,
          },
          { name: "eth1", readBytesPerSecond: 500_000, writeBytesPerSecond: 0 },
        ],
        disks: [
          {
            name: "sda",
            readBytesPerSecond: 1048576,
            writeBytesPerSecond: 2097152,
          },
          { name: "sdb", readBytesPerSecond: 0, writeBytesPerSecond: 0 },
        ],
        networkError: null,
        diskError: null,
        defaultInterface: "eth0",
      },
      ...overrides,
    });
  }, overrides);
}
test.beforeEach(async ({ page }) => {
  await page.goto("/tests/ui/terminal-harness.html");
  await page.getByTitle("New local terminal", { exact: true }).click();
  await expect(page.locator(".terminal-status")).toHaveText("Connected");
  await emit(page);
});
test("throughput units, per-device selection, unavailable and hidden stats", async ({
  page,
}) => {
  const n = page.getByTestId("network-throughput"),
    d = page.getByTestId("disk-throughput");
  await expect(n).toContainText(/8[.,]00 Mbit\/s/);
  await expect(n).toContainText(/16[.,]00 Mbit\/s/);
  await expect(d).toContainText(/1[.,]00 MiB\/s/);
  await expect(d).toContainText(/2[.,]00 MiB\/s/);
  await n.click();
  await page
    .getByLabel("Network interface", { exact: true })
    .selectOption("eth1");
  await expect(n).toContainText(/4[.,]00 Mbit\/s/);
  await expect(n).toContainText(/0[.,]00 Mbit\/s/);
  await page.getByLabel("Disk device", { exact: true }).selectOption("sdb");
  await expect(d).toContainText(/0[.,]00 MiB\/s/);
  await page.getByRole("button", { name: "Close activity details" }).click();
  await emit(page, { activityAt: Date.now() - 12000 });
  await expect(n).not.toContainText("Mbit/s");
  await expect(n).toContainText("—");
  await emit(page, { activityError: "Probe unavailable" });
  await expect(d).toHaveAttribute("title", "Probe unavailable");
  await page.getByRole("checkbox", { name: "Show resource monitor" }).uncheck();
  await expect(page.getByTestId("resource-monitor")).toHaveCount(0);
});
test("non-root critical/full mounts alert in summary and sort first", async ({
  page,
}) => {
  await expect(page.locator(".resource-disk-alert")).toHaveText(
    "1 full · 1 critical",
  );
  await page.locator(".resource-disk").click();
  const mounts = page.locator(".resource-mount");
  await expect(mounts).toHaveCount(3);
  await expect(mounts.nth(0)).toContainText("/mnt/Arşiv");
  await expect(mounts.nth(0)).toContainText("Full");
  await expect(mounts.nth(1)).toContainText("Critical");
  await expect(mounts.nth(2)).toContainText("/dev/sda1");
  await page.getByLabel("Show virtual mounts").check();
  await expect(mounts).toHaveCount(4);
  await expect(
    page.locator(".resource-disk > .resource-disk-alert"),
  ).toContainText("2 full");
  await page.screenshot({ path: "artifacts/activity-disk-alerts.png" });
});
test("compact panels keep throughput within the terminal and refit xterm", async ({
  page,
}) => {
  await page.setViewportSize({ width: 900, height: 700 });
  await page.getByTitle("New local terminal", { exact: true }).click();
  await page.getByTitle("Split terminals", { exact: true }).click();
  await expect
    .poll(() =>
      page
        .locator(".terminal-pane")
        .first()
        .evaluate((el) => el.scrollWidth <= el.clientWidth + 1),
    )
    .toBe(true);
  await expect
    .poll(() =>
      page
        .locator(".terminal-pane")
        .first()
        .evaluate((el) => {
          const s = el.querySelector(".terminal-surface")!,
            x = el.querySelector(".xterm-screen")!;
          return s.clientHeight - x.clientHeight;
        }),
    )
    .toBeLessThan(40);
});

test("Turkish preview shows full and critical disks with readable throughput", async ({
  page,
}) => {
  await page.evaluate(() => localStorage.setItem("termterm.locale", "tr"));
  await page.reload();
  await page.getByTitle("Yeni yerel terminal", { exact: true }).click();
  await expect(page.locator(".terminal-status")).toHaveText("Bağlı");
  await emit(page, { source: "Örnek ölçümler" });
  await expect(page.getByTestId("network-throughput")).toContainText("Gelen");
  await expect(page.getByTestId("disk-throughput")).toContainText("Yazma");
  await page.locator(".resource-disk").click();
  await expect(page.locator(".resource-mount").first()).toContainText("Dolu");
  await page.screenshot({ path: "artifacts/activity-preview-tr.png" });
});
