// Package a minimal Cygwin runtime from the official HTTPS repository.
// Archives are checked against the SHA-512 values in setup.ini before extraction.
import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";
const root = path.resolve(import.meta.dirname, "..");
const cache = path.join(root, ".tools", "cygwin");
const dest = path.join(root, "src-tauri", "resources", "mosh");
await fs.mkdir(cache, { recursive: true });
await fs.mkdir(dest, { recursive: true });
const manifest = await (
  await fetch("https://cygwin.com/pub/cygwin/x86_64/setup.ini")
).text();
const packages = [
  "mosh",
  "cygwin",
  "libgcc1",
  "libstdc++6",
  "libncursesw10",
  "libprotobuf32",
  "libssl3",
  "zlib0",
  "libiconv2",
  "libintl8",
  "libzstd1",
  "terminfo",
];
const inventory = [];
const approvedFiles = new Set(["packages.json", "NOTICE.txt"]);
for (const name of packages) {
  const section = manifest
    .split(`\n@ ${name}\n`)[1]
    ?.split(/\n@ /)[0]
    ?.split(/\n\[(?:prev|test)\]/)[0];
  const entry = section?.match(/^install: (\S+) (\d+) ([a-f0-9]{128})$/m);
  if (!entry) throw new Error(`Missing current package: ${name}`);
  const [, file, size, sha] = entry;
  const archive = path.join(cache, path.basename(file));
  let bytes;
  try {
    bytes = await fs.readFile(archive);
  } catch {
    const response = await fetch("https://cygwin.com/pub/cygwin/" + file);
    if (!response.ok) throw new Error(`Download failed: ${name}`);
    bytes = Buffer.from(await response.arrayBuffer());
    await fs.writeFile(archive, bytes);
  }
  if (
    bytes.length !== Number(size) ||
    crypto.createHash("sha512").update(bytes).digest("hex") !== sha
  )
    throw new Error(`Archive integrity check failed: ${name}`);
  const listing = execFileSync("tar", ["-tf", archive], { encoding: "utf8" })
    .trim()
    .split(/\r?\n/);
  const files = listing.filter(
    (f) =>
      !f.endsWith("/") &&
      !f.includes("..") &&
      (/(?:^|\/)bin\/[^/]+\.dll$/.test(f) ||
        /(?:^|\/)bin\/mosh-client\.exe$/.test(f) ||
        /share\/terminfo\/.*\/(xterm|xterm-256color|screen|screen-256color)$/.test(
          f,
        ) ||
        /share\/doc\/.*(COPYING|LICENSE|copyright)/i.test(f)),
  );
  if (files.length)
    execFileSync("tar", ["-xf", archive, "-C", dest, ...files], {
      stdio: "pipe",
    });
  for (const file of files) approvedFiles.add(file);
  const sourceEntry = (() => {
    const direct = section.match(/^source: (\S+) (\d+) ([a-f0-9]{128})$/m);
    if (direct) return direct;
    const external = section.match(/^external-source: (\S+)/m)?.[1];
    return external && manifest.split(`\n@ ${external}\n`)[1]?.split(/\n@ /)[0]?.split(/\n\[(?:prev|test)\]/)[0]?.match(/^source: (\S+) (\d+) ([a-f0-9]{128})$/m);
  })();
  if (!sourceEntry) throw new Error(`Missing matching source: ${name}`);
  inventory.push({
    name,
    version: section.match(/^version: (.+)$/m)?.[1],
    url: "https://cygwin.com/pub/cygwin/" + file,
    sha512: sha,
    source: "https://cygwin.com/pub/cygwin/" + sourceEntry[1],
    sourceBytes: Number(sourceEntry[2]),
    sourceSha512: sourceEntry[3],
  });
  console.log(`Packaged ${name}`);
}
const sourceCache = path.join(cache, "sources");
await fs.mkdir(sourceCache, {recursive: true});
const sources = {};
for (const pkg of inventory) {
  const file = path.posix.basename(new URL(pkg.source).pathname);
  const pin = {url: pkg.source, bytes: pkg.sourceBytes, sha512: pkg.sourceSha512};
  if (sources[file]) {
    if (JSON.stringify(sources[file]) !== JSON.stringify(pin)) throw new Error("Conflicting Mosh source pins");
    continue;
  }
  const archive = path.join(sourceCache, file);
  let bytes;
  try { bytes = await fs.readFile(archive); }
  catch (error) {
    if (error.code !== "ENOENT") throw error;
    const response = await fetch(pkg.source);
    if (!response.ok) throw new Error(`Source download failed: ${pkg.name}`);
    bytes = Buffer.from(await response.arrayBuffer());
    await fs.writeFile(archive, bytes);
  }
  if (bytes.length !== pin.bytes || crypto.createHash("sha512").update(bytes).digest("hex") !== pin.sha512)
    throw new Error(`Source archive integrity check failed: ${pkg.name}`);
  sources[file] = pin;
}
await fs.writeFile(path.join(sourceCache, "SOURCES.json"), JSON.stringify({version: 1, sources}, null, 2));
// Cygwin locates its DLLs alongside the executable. Archives use both bin and usr/bin.
await fs.mkdir(path.join(dest, "bin"), { recursive: true });
for (const file of [...approvedFiles]) {
  const name = path.posix.basename(file);
  if (file.startsWith("usr/bin/") && (name.endsWith(".dll") || name === "mosh-client.exe")) {
    await fs.copyFile(
      path.join(dest, file),
      path.join(dest, "bin", name),
    );
    approvedFiles.add("bin/" + name);
  }
}
await fs.writeFile(
  path.join(dest, "packages.json"),
  JSON.stringify(inventory, null, 2),
);
await fs.writeFile(
  path.join(dest, "NOTICE.txt"),
  "Mosh (GPL-3.0): https://mosh.org/ and https://github.com/mobile-shell/mosh\nCygwin runtime and libraries: https://cygwin.com/licensing.html\nPackage versions, source locations and checksums are recorded in packages.json.\nOriginal license files are in share/doc or usr/share/doc.\n",
);
const approvedHashes = {};
for (const file of [...approvedFiles].sort()) {
  approvedHashes[file] = crypto.createHash("sha256").update(await fs.readFile(path.join(dest, file))).digest("hex");
}
await fs.writeFile(path.join(dest, "runtime-files.json"), JSON.stringify({version: 1, files: approvedHashes}, null, 2));
console.log("Mosh runtime ready.");
