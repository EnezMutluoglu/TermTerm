import path from "node:path";
export const config = {
  runner: "local",
  specs: [process.env.TERMTERM_SPEC ?? "./tests/desktop/workspace.mjs"],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: path.resolve(
          process.env.TERMTERM_BINARY ??
            `src-tauri/target/debug/termterm${process.platform === "win32" ? ".exe" : ""}`,
        ),
      },
    },
  ],
  services: [
    [
      "tauri",
      {
        appBinaryPath: path.resolve(
          process.env.TERMTERM_BINARY ??
            `src-tauri/target/debug/termterm${process.platform === "win32" ? ".exe" : ""}`,
        ),
        driverProvider: "embedded",
        embeddedPort: Number(process.env.TERMTERM_DRIVER_PORT ?? 4445),
      },
    ],
  ],
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    timeout: Math.max(
      120000,
      (Number(process.env.TERMTERM_SOAK_SECONDS ?? 0) + 90) * 1000,
    ),
  },
  waitforTimeout: 15000,
  logLevel: "warn",
  before: async function () {
    // Existing native interaction suites use English selectors; the dedicated
    // remember/restart suite verifies the actual default Turkish application.
    const locale = process.env.TERMTERM_LOCALE ??
      (process.env.TERMTERM_SPEC?.includes("remember-vault") ? "tr" : "en");
    await browser.execute((value) => {
      if (value === "tr") localStorage.removeItem("termterm.locale");
      else localStorage.setItem("termterm.locale", value);
      if (document.documentElement.lang !== value) {
        window.__localeReloadPending = true;
        setTimeout(() => location.reload(), 100);
      }
    }, locale);
    await browser.waitUntil(async () =>
      (await browser.execute(() => window.__localeReloadPending ? "pending" : document.documentElement.lang)) === locale,
    );
  },
  outputDir: process.env.TERMTERM_LOG_DIR ?? ".tools/wdio",
  afterTest: async function (test, context, { passed }) {
    if (!passed) {
      console.log(
        "Native events:",
        await browser.execute(() =>
          Object.fromEntries(
            Object.entries(window.__nativeEvents ?? {}).map(([id, e]) => [
              id,
              { status: e.status, output: e.output.slice(-1200) },
            ]),
          ),
        ),
      );
      await browser.saveScreenshot(
        path.resolve("artifacts/native-failure.png"),
      );
      console.log(
        "Failure screen:",
        (await browser.$("body").getText()).slice(-6000),
      );
    }
  },
};
