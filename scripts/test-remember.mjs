// Requires the isolated E2E build; exercises three real application processes.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
fs.mkdirSync(path.join(root, ".lab"), { recursive: true });
const work = fs.mkdtempSync(path.join(root, ".lab", "remember-"));
try {
  for (const phase of ["create", "restart", "forgotten"]) {
    execFileSync(process.execPath, [
      path.join(root, "node_modules/@wdio/cli/bin/wdio.js"), "run", "wdio.conf.mjs",
    ], {
      cwd: root, stdio: "inherit",
      env: { ...process.env, TERMTERM_SPEC: "./tests/desktop/remember-vault.mjs",
        TERMTERM_REMEMBER_WORK: work, TERMTERM_REMEMBER_PHASE: phase, TERMTERM_LOCALE: "tr" },
    });
  }
  console.log(`Remembered vault restart tests passed. Test screenshots: ${work}`);
} finally {
  const metadata = path.join(work, "restore.json");
  if (fs.existsSync(metadata)) {
    const { recentPath, previous } = JSON.parse(fs.readFileSync(metadata, "utf8"));
    const target = path.resolve(recentPath);
    if (path.basename(target) !== "recent.json" || path.basename(path.dirname(target)) !== "local.termterm.desktop.e2e")
      throw Error("Unexpected test profile; original pointer retained in restore.json");
    if (previous !== null) fs.writeFileSync(target, Buffer.from(previous, "base64"));
    else if (fs.existsSync(target)) fs.unlinkSync(target);
  }
}
