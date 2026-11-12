import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const read = (path) => JSON.parse(fs.readFileSync(new URL(`../${path}`, import.meta.url)));
test("VPN development cannot use the installed product identity or stable feed", () => {
  const stable = read("src-tauri/tauri.conf.json");
  const dev = read("src-tauri/tauri.vpn-dev.conf.json");
  assert.notEqual(dev.identifier, stable.identifier);
  assert.notEqual(dev.productName, stable.productName);
  assert.equal(dev.version, "0.4.3-dev.4");
  assert.equal(dev.bundle.active, false);
  assert.deepEqual(dev.bundle.resources, []);
  assert.equal(dev.bundle.createUpdaterArtifacts, false);
  assert.deepEqual(dev.plugins.updater.endpoints, []);
  const scripts = read("package.json").scripts;
  assert.match(scripts["vpn:dev"], /--config src-tauri\/tauri\.vpn-dev\.conf\.json$/);
  assert.match(scripts["vpn:build"], /--no-bundle/);
  assert.match(scripts["vpn:build"], /--config src-tauri\/tauri\.vpn-dev\.conf\.json$/);
});
test("VPN core source and license pins are complete", () => {
  const spec = read("vpn-engine/dependencies.json");
  assert.equal(spec.distributionReady, false);
  assert.equal(spec.openvpnCore.license, "MPL-2.0");
  for (const source of [spec.openvpnCore, spec.openconnectReferenceSource]) {
    assert.match(source.commit, /^[0-9a-f]{40}$/);
    assert.match(source.gitArchiveSha256, /^[0-9a-f]{64}$/);
    assert.match(source.licenseSha256, /^[0-9a-f]{64}$/);
  }
});

test("Linux VPN packages have a separate binary, identity and disabled updater", () => {
  const dev = read("src-tauri/tauri.vpn-dev.conf.json");
  const overlay = read("src-tauri/tauri.vpn-linux-package.conf.json");
  assert.equal(overlay.identifier, dev.identifier);
  assert.equal(overlay.productName, dev.productName);
  assert.equal(overlay.mainBinaryName, "termterm-vpn-dev");
  assert.deepEqual(overlay.bundle.targets, ["deb", "appimage"]);
  assert.equal(overlay.bundle.createUpdaterArtifacts, false);
  assert.deepEqual(overlay.plugins.updater.endpoints, []);
  assert.deepEqual(overlay.bundle.linux.deb.conflicts, []);
  assert.deepEqual(overlay.bundle.linux.deb.replaces, []);
});

test("Windows VPN installer never checks or kills the stable executable basename", () => {
  const config = read("src-tauri/tauri.vpn-windows-package.conf.json");
  assert.equal(config.mainBinaryName, "termterm-vpn-dev");
  assert.equal(config.identifier, read("src-tauri/tauri.vpn-dev.conf.json").identifier);
  assert.equal(config.productName, "TermTerm VPN Dev");
  assert.deepEqual(config.plugins.updater.endpoints, []);
  assert.equal(config.bundle.createUpdaterArtifacts, false);
});


test("VPN UI is enabled only by development or the explicit isolated build mode", () => {
  const text = (path) => fs.readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
  const dev = read("src-tauri/tauri.vpn-dev.conf.json");
  const stable = read("src-tauri/tauri.conf.json");
  assert.equal(dev.build.beforeDevCommand, "pnpm dev --mode vpn-dev");
  assert.equal(dev.build.beforeBuildCommand, "pnpm build --mode vpn-dev");
  assert.equal(dev.build.frontendDist, "../dist-vpn-dev");
  assert.equal(stable.build.beforeBuildCommand, "pnpm build");
  assert.equal(stable.build.frontendDist, "../dist");
  assert.equal(text(".env.vpn-dev").trim(), "VITE_VPN_DEV=1");
  assert.match(text("src/App.tsx"), /import\.meta\.env\.MODE === "vpn-dev" && import\.meta\.env\.VITE_VPN_DEV === "1"/);
  assert.match(text("vite.config.ts"), /outDir: mode === "vpn-dev" \? "dist-vpn-dev" : mode === "vpn-candidate" \? "dist-vpn-candidate" : "dist"/);
});

test("production candidate is an explicit versioned feature with no updater or test capability", () => {
  const candidate = read("src-tauri/tauri.vpn-candidate.conf.json");
  const text = (path) => fs.readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
  assert.equal(candidate.version, "0.4.5");
  assert.equal(candidate.identifier, "local.termterm.desktop");
  assert.equal(candidate.mainBinaryName, "termterm");
  assert.deepEqual(candidate.build.features, ["vpn-candidate"]);
  assert.equal(candidate.build.beforeBuildCommand, "pnpm build --mode vpn-candidate");
  assert.equal(candidate.build.frontendDist, "../dist-vpn-candidate");
  assert.deepEqual(candidate.plugins.updater.endpoints, []);
  assert.equal(candidate.bundle.active, false);
  assert.equal(candidate.bundle.createUpdaterArtifacts, false);
  assert.equal(candidate.app.withGlobalTauri, undefined);
  assert.equal(text(".env.vpn-candidate").trim(), "VITE_VPN_CANDIDATE=0.4.5");
  assert.match(text("src/App.tsx"), /import\.meta\.env\.MODE === "vpn-candidate" && import\.meta\.env\.VITE_VPN_CANDIDATE === "0.4.5"/);
  for (const capability of candidate.app.security.capabilities) {
    assert.deepEqual(capability.webviews, ["main"]);
    assert.equal(capability.remote, undefined);
    assert.equal(capability.windows, undefined);
  }
});

test("VPN native UI driver stays in an explicit debug test overlay", () => {
  const dev = read("src-tauri/tauri.vpn-dev.conf.json");
  const e2e = read("src-tauri/tauri.vpn-e2e.conf.json");
  assert.equal(e2e.identifier, dev.identifier);
  assert.equal(e2e.app.withGlobalTauri, true);
  assert.equal(dev.app.withGlobalTauri, undefined);
  assert.deepEqual(e2e.app.security.capabilities[1].permissions, ["wdio:default", "wdio-webdriver:default"]);
  assert.doesNotMatch(read("package.json").scripts["vpn:build"], /--features.*e2e/);
  const native = fs.readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  assert.match(native, /#\[cfg\(all\(feature = "e2e", debug_assertions\)\)\]/);
});

test("remote SSO child webviews inherit no application or test-driver capability", () => {
  for (const config of [read("src-tauri/tauri.vpn-dev.conf.json"),read("src-tauri/tauri.vpn-e2e.conf.json")]) {
    for (const capability of config.app.security.capabilities) {
      assert.deepEqual(capability.webviews,["main"]);
      assert.equal(capability.windows,undefined);
      assert.equal(capability.remote,undefined);
    }
  }
});
