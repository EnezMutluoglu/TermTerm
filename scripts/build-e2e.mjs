import { execFileSync } from "node:child_process";
const env = { ...process.env, VITE_E2E: "true" };
execFileSync(process.execPath, ["node_modules/typescript/bin/tsc", "-b"], {
  stdio: "inherit",
  env,
});
execFileSync(process.execPath, ["node_modules/vite/bin/vite.js", "build"], {
  stdio: "inherit",
  env,
});
