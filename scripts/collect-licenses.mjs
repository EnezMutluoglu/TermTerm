import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
const root = path.resolve(import.meta.dirname, "..");
const cargo = process.platform === "win32" ? path.join(process.env.USERPROFILE, ".cargo", "bin", "cargo.exe") : "cargo";
const metadata = JSON.parse(
  execFileSync(
    cargo,
    [
      "metadata",
      "--manifest-path",
      path.join(root, "src-tauri/Cargo.toml"),
      "--format-version",
      "1",
      "--locked",
      "--offline",
    ],
    { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
  ),
);
const npm = JSON.parse(
  await fs.readFile(path.join(root, ".tools/npm-licenses.json"), "utf8"),
);
const packages = [];
for (const pkg of metadata.packages) {
  if (pkg.name === "termterm") continue;
  packages.push({
    name: pkg.name,
    version: pkg.version,
    license: pkg.license,
    repository: pkg.repository,
    folder: path.dirname(pkg.manifest_path),
  });
}
for (const [license, items] of Object.entries(npm)) {
  for (const item of items) {
    for (let i = 0; i < item.paths.length; i++)
      packages.push({
        name: item.name,
        version: item.versions[i] ?? item.versions[0],
        license,
        repository: item.repository,
        folder: item.paths[i],
      });
  }
}
const notices = [
  "TermTerm dependency notices",
  "This inventory includes build/test and other-platform dependencies, some of which are not linked into the Windows release. Original component licenses govern their use.",
  "Mosh and Cygwin notices are also distributed beside their separate helper executable.",
];
const inventory = [];
for (const pkg of packages) {
  const files = (await fs.readdir(pkg.folder).catch(() => [])).filter((n) =>
    /^(LICENSE|LICENCE|COPYING|NOTICE)([._-].*)?$/i.test(n),
  );
  const texts = [];
  for (const name of files) {
    try {
      const stat = await fs.stat(path.join(pkg.folder, name));
      if (stat.isFile() && stat.size < 1024 * 1024)
        texts.push(
          name +
            "\n" +
            (await fs.readFile(path.join(pkg.folder, name), "utf8")),
        );
    } catch {}
  }
  inventory.push({
    name: pkg.name,
    version: pkg.version,
    license: pkg.license,
    repository: pkg.repository,
    licenseFiles: files,
  });
  notices.push(
    "\n" +
      "=".repeat(72) +
      "\n" +
      pkg.name +
      " " +
      pkg.version +
      "\nSPDX: " +
      (pkg.license ?? "See component source") +
      "\n" +
      (pkg.repository ?? "") +
      "\n" +
      texts.join("\n\n"),
  );
}
const dest = path.join(root, "docs/licenses");
await fs.mkdir(dest, { recursive: true });
await fs.writeFile(
  path.join(dest, "DEPENDENCIES.json"),
  JSON.stringify(inventory, null, 2),
);
await fs.writeFile(
  path.join(dest, "THIRD_PARTY_LICENSES.txt"),
  notices.join("\n"),
);
console.log(`Collected notices for ${inventory.length} dependencies.`);
