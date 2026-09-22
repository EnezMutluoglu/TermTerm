import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "tests/ui",
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:1420",
    channel:
      process.env.PLAYWRIGHT_CHANNEL ||
      (process.env.CI || process.platform !== "win32" ? undefined : "msedge"),
    headless: true,
    viewport: { width: 1440, height: 900 },
  },
  reporter: "list",
  webServer: {
    command: "pnpm dev",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: true,
  },
});
