import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: ".",
  testMatch: ["vpn.spec.ts", "vpnDesktop.spec.ts", "team-vpn.spec.ts"],
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:1427",
    channel:
      process.env.CI || process.platform !== "win32" ? undefined : "msedge",
    launchOptions: { args: ["--no-proxy-server"] },
    viewport: { width: 1600, height: 1000 },
  },
  webServer: {
    command: "pnpm dev --port 1427",
    url: "http://127.0.0.1:1427",
    reuseExistingServer: false,
  },
});
