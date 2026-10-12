import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const read = (path) => JSON.parse(fs.readFileSync(new URL(`../${path}`, import.meta.url)));
test("VPN development cannot use the installed product identity or stable feed", () => {
  const stable = read("src-tauri/tauri.conf.json");
  const dev = read("src-tauri/tauri.vpn-dev.conf.json");
  assert.notEqual(dev.identifier, stable.identifier);
  assert.notEqual(dev.productName, stable.productName);
  assert.equal(dev.version, "0.4.3-dev.1");
  assert.equal(dev.bundle.active, false);
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
