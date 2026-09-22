import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";
const root = path.resolve(import.meta.dirname, ".."),
  bundle = path.join(root, "src-tauri/target/release/bundle");
async function walk(dir) {
  const out = [];
  for (const e of await fs.readdir(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) out.push(...(await walk(p)));
    else out.push(p);
  }
  return out;
}
const version=JSON.parse(await fs.readFile(path.join(root,'package.json'),'utf8')).version;
const files = (await walk(bundle)).filter(file=>path.basename(file).includes(`_${version}_`)),
  suffixes =
    process.platform === "darwin"
      ? [".dmg"]
      : process.platform === "linux"
        ? [".AppImage", ".deb"]
        : [".exe"];
for (const ext of suffixes)
  if (!files.some((f) => f.endsWith(ext)))
    throw Error("Missing package " + ext);
const tree = execFileSync(
  "cargo",
  [
    "tree",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--edges",
    "normal",
    "--prefix",
    "none",
  ],
  { cwd: root, encoding: "utf8" },
);
if (/tauri-plugin-wdio/.test(tree))
  throw Error("Production dependency graph contains test driver");
if (process.platform === "darwin") {
  const app = path.join(bundle, "macos/TermTerm.app");
  execFileSync("codesign", ["--verify", "--deep", "--strict", app]);
}
const output = path.join(root, "artifacts", `release-${version}`);
await fs.mkdir(output, { recursive: true });
const hashes = [];
for (const file of files.filter((f) =>
  suffixes.some((ext) => f.endsWith(ext)),
)) {
  const bytes = await fs.readFile(file);
  if (bytes.length < 10000) throw Error("Truncated package " + file);
  const name = path.basename(file);
  await fs.copyFile(file, path.join(output, name));
  hashes.push(
    crypto.createHash("sha256").update(bytes).digest("hex") + "  " + name,
  );
}
await fs.writeFile(
  path.join(output, `SHA256SUMS-${process.platform}-${process.arch}.txt`),
  hashes.join("\n") + "\n",
);
console.log(
  "Verified native packages, normal dependency graph and hashes:",
  hashes.length,
);
