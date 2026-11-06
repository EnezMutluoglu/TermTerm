import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: ".", testMatch: "authenticator.spec.ts", workers: 1,
  use: {
    baseURL: "http://127.0.0.1:1428", viewport: { width: 1440, height: 900 },
    channel: process.env.CI || process.platform !== "win32" ? undefined : "msedge",
    launchOptions: { args: ["--no-proxy-server"] },
  },
  webServer: { command: "pnpm dev --port 1428", url: "http://127.0.0.1:1428", reuseExistingServer: true },
});
