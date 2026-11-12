import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {releaseAssets} from './release-assets.mjs';

// The reviewed 0.4.3 overlay has separate desktop/source receipts and a 0.4.2 service.
export function verifyReleaseManifest(dir, version) {
  const need = (ok, message) => { if (!ok) throw Error(message); };
  need(['0.4.3', '0.4.4'].includes(version), 'Candidate manifest is only defined for 0.4.3/0.4.4');
  const {required} = releaseAssets(version);
  const files = required.filter(name => name !== 'RELEASE-MANIFEST.json');
  const allowed = new Set([...required, 'latest.json', 'SHA256SUMS.txt']);
  for (const name of fs.readdirSync(dir)) {
    need(allowed.has(name), `Unknown release asset: ${name}`);
    need(fs.lstatSync(path.join(dir, name)).isFile(), `Unsafe release asset: ${name}`);
  }
  for (const name of required) need(fs.lstatSync(path.join(dir, name)).isFile(), `Unsafe release asset: ${name}`);
  const read = name => JSON.parse(fs.readFileSync(path.join(dir, name), 'utf8'));
  const manifest = read('RELEASE-MANIFEST.json');
  need(manifest.formatVersion === 1 && manifest.version === version, 'Candidate version mismatch');
  const contract = {serviceVersion:'0.4.2', apiVersion:1, schemaVersion:66, minimumClientVersion:'0.4.1', clientVersions:['0.4.1','0.4.2','0.4.3']};
  need(manifest.teamCompatibility && Object.keys(manifest.teamCompatibility).length === Object.keys(contract).length && Object.keys(contract).every(key => JSON.stringify(manifest.teamCompatibility[key]) === JSON.stringify(contract[key])), 'Team component compatibility mismatch');
  need(manifest.assets && JSON.stringify(Object.keys(manifest.assets).sort()) === JSON.stringify(files.slice().sort()), 'Manifest asset allowlist mismatch');
  for (const name of files) {
    const bytes = fs.readFileSync(path.join(dir, name)), item = manifest.assets[name];
    need(item.bytes === bytes.length && item.sha256 === crypto.createHash('sha256').update(bytes).digest('hex'), `Release asset hash mismatch: ${name}`);
  }
  const hash = name => manifest.assets[name].sha256;
  if (version === '0.4.4') {
    need(manifest.teamRelease?.version === '0.4.3' && manifest.teamRelease.unchanged === true, 'Patch must reference the unchanged 0.4.3 Team release');
    const windows = read('WINDOWS-PACKAGE-RECEIPT.json'), linux = read('LINUX-PACKAGE-RECEIPT.json');
    for (const receipt of [windows, linux]) need(receipt.version === version && receipt.identity === 'local.termterm.desktop', 'Patch desktop version/identity mismatch');
    need(windows.binaryMetadata?.productVersion === version && windows.binaryMetadata?.fileVersion === version, 'Windows metadata version mismatch');
    need(windows.exactNsisDesktopAndManifestVerified === true && ['brokerSha256','portableClientSha256','nsisClientSha256','portableBundleManifestSha256','nsisBundleManifestSha256'].every(key => /^[a-f0-9]{64}$/.test(windows[key])), 'Windows installer/broker binding mismatch');
    need(windows.sourceZipSha256 === hash('TermTerm-0.4.4-windows-source.zip') && linux.sourceSha256 === hash('TermTerm-0.4.4-linux-source.zip'), 'Desktop source receipt mismatch');
    for (const [receipt, original, released] of [
      [windows, 'TermTerm_0.4.4_x64-setup.exe', 'TermTerm_0.4.4_x64-setup.exe'],
      [windows, 'TermTerm-Release-Candidate-0.4.4-windows-x64-portable.zip', 'TermTerm-0.4.4-windows-x64-portable.zip'],
      [windows, 'TermTerm-Release-Candidate-0.4.4-dependency-source.zip', 'TermTerm-0.4.4-windows-dependency-source.zip'],
      [linux, 'TermTerm_0.4.4_amd64.AppImage', 'TermTerm_0.4.4_amd64.AppImage'],
      [linux, 'TermTerm_0.4.4_amd64.deb', 'TermTerm_0.4.4_amd64.deb'],
      [linux, 'termterm-vpn-broker_0.4.4_amd64.deb', 'termterm-vpn-broker_0.4.4_amd64.deb'],
      [linux, 'termterm-vpn-linux-dependency-source.tar.gz', 'TermTerm-0.4.4-linux-dependency-source.tar.gz'],
    ]) need(receipt.files?.[original]?.sha256 === hash(released), `Package receipt mismatch: ${released}`);
    const installer = read('TermTerm-0.4.4-APPIMAGE-INSTALL.json');
    need(installer.candidateVersion === version && installer.imageSha256 === hash('TermTerm_0.4.4_amd64.AppImage') && installer.applicationIdentifier === linux.identity && installer.clientSha256 === linux.appImageClientSha256 && /^[a-f0-9]{64}$/.test(installer.clientSha256), 'AppImage installer binding mismatch');
    need(linux.appImageManifestSha256 === hash('TermTerm-0.4.4-APPIMAGE-INSTALL.json'), 'AppImage manifest receipt mismatch');
    const acceptance = read('PATCH-ACCEPTANCE.json');
    need(acceptance.version === version && acceptance.windows?.ok === true && acceptance.windows.profileImport === true && acceptance.windows.nativeDataPath === true && acceptance.linux?.ok === true && acceptance.linux.profileImport === true && acceptance.linux.engineValidation === true, 'Actual Windows/Linux patch acceptance required');
    need(acceptance.windows.installerSha256 === hash('TermTerm_0.4.4_x64-setup.exe') && acceptance.windows.packageReceiptSha256 === hash('WINDOWS-PACKAGE-RECEIPT.json') && acceptance.linux.appImageSha256 === hash('TermTerm_0.4.4_amd64.AppImage') && acceptance.linux.brokerSha256 === hash('termterm-vpn-broker_0.4.4_amd64.deb') && acceptance.linux.packageReceiptSha256 === hash('LINUX-PACKAGE-RECEIPT.json'), 'Patch acceptance artifact binding mismatch');
    return manifest;
  }
  const windows = read('WINDOWS-PACKAGE-RECEIPT.json'), linux = read('LINUX-PACKAGE-RECEIPT.json'), mac = read('MACOS-SOURCE-RECEIPT.json');
  for (const receipt of [windows, linux, mac]) need(receipt.version === version, 'Desktop receipt version mismatch');
  need(windows.binaryMetadata?.productVersion === version && windows.binaryMetadata?.fileVersion === version, 'Windows metadata version mismatch');
  need(windows.identity === 'local.termterm.desktop' && linux.identity === windows.identity, 'Desktop identity mismatch');
  need(windows.sourceZipSha256 === hash('TermTerm-0.4.3-windows-source.zip') && linux.sourceSha256 === hash('TermTerm-0.4.3-linux-source.zip') && mac.files?.['termterm-vpn-source.zip']?.sha256 === hash('TermTerm-0.4.3-macos-source.zip'), 'Desktop source receipt mismatch');
  for (const [receipt, names] of [[windows, ['TermTerm_0.4.3_x64-setup.exe']], [linux, ['TermTerm_0.4.3_amd64.AppImage','TermTerm_0.4.3_amd64.deb','termterm-vpn-broker_0.4.3_amd64.deb']]])
    for (const name of names) need(receipt.files?.[name]?.sha256 === hash(name), `Package receipt mismatch: ${name}`);
  for (const [receipt, original, released] of [
    [windows,'TermTerm-Release-Candidate-0.4.3-windows-x64-portable.zip','TermTerm-0.4.3-windows-x64-portable.zip'],
    [windows,'TermTerm-Release-Candidate-0.4.3-dependency-source.zip','TermTerm-0.4.3-windows-dependency-source.zip'],
    [linux,'termterm-vpn-linux-dependency-source.tar.gz','TermTerm-0.4.3-linux-dependency-source.tar.gz'],
    [mac,'TermTerm-0.4.3-macos-source-preparation.zip','TermTerm-0.4.3-macos-source-preparation.zip'],
  ]) need(receipt.files?.[original]?.sha256 === hash(released), `Archive rename receipt mismatch: ${released}`);
  const installer = read('TermTerm-0.4.3-APPIMAGE-INSTALL.json');
  need(installer.candidateVersion === version && installer.imageSha256 === hash('TermTerm_0.4.3_amd64.AppImage') && installer.applicationIdentifier === linux.identity, 'AppImage installer binding mismatch');
  need(linux.appImageManifestSha256 === hash('TermTerm-0.4.3-APPIMAGE-INSTALL.json'), 'AppImage manifest receipt mismatch');
  const team = read('TEAM-DELIVERY-RECEIPT.json'), delta = read('TEAM-CATALOGUE-DELTA.json'), image = read('TermTerm-Team-Image-0.4.2-vpn-0.4.3-linux-x64.json');
  need(team.serviceVersion === contract.serviceVersion && team.desktopCandidateContext === version && team.apiVersion === 1 && team.schemaVersion === 66 && team.buildProfile === 'release', 'Team receipt version mismatch');
  need(team.publicPromoted === true && team.compiledSourceEqualsPublicSource === true && team.catalogueDeltaSha256 === hash('TEAM-CATALOGUE-DELTA.json'), 'Team compiled source promotion mismatch');
  for (const key of Object.keys(contract)) need(JSON.stringify(delta.after?.[key]) === JSON.stringify(contract[key]), `Team catalogue mismatch: ${key}`);
  need(team.archive?.sha256 === hash('TermTerm-Team-Service-0.4.2-vpn-0.4.3-linux-x64.tar.gz') && team.imageArchive?.sha256 === hash('TermTerm-Team-Image-0.4.2-vpn-0.4.3-linux-x64.tar.gz'), 'Team archive receipt mismatch');
  need(image.version === contract.serviceVersion && image.archive?.sha256 === team.imageArchive.sha256 && image.binarySha256 === team.binarySha256 && image.imageId === team.imageId, 'Team image receipt mismatch');
  const acceptance = read('RELEASE-ACCEPTANCE.json');
  need(acceptance.version === version && acceptance.windowsUpgrade === true && acceptance.linuxRuntime === true && acceptance.teamPolicy === true, 'Actual upgrade/runtime/Team acceptance required');
  need(acceptance.windowsInstallerSha256 === hash('TermTerm_0.4.3_x64-setup.exe') && acceptance.linuxAppImageSha256 === hash('TermTerm_0.4.3_amd64.AppImage') && acceptance.teamBinarySha256 === team.binarySha256, 'Acceptance artifact binding mismatch');
  const windowsUpgrade = read('WINDOWS-UPGRADE-ACCEPTANCE.json'), linuxRuntime = read('LINUX-RUNTIME-ACCEPTANCE.json'), teamPolicy = read('TEAM-CATALOGUE-ACCEPTANCE.json');
  need(windowsUpgrade.ok === true && windowsUpgrade.candidateVersion === version && windowsUpgrade.installerSha256 === acceptance.windowsInstallerSha256, 'Windows upgrade acceptance mismatch');
  need(linuxRuntime.ok === true && linuxRuntime.version === version && linuxRuntime.packageReceiptSha256 === hash('LINUX-PACKAGE-RECEIPT.json') && linuxRuntime.appImageSha256 === acceptance.linuxAppImageSha256 && linuxRuntime.brokerSha256 === hash('termterm-vpn-broker_0.4.3_amd64.deb'), 'Linux runtime acceptance mismatch');
  need(teamPolicy.ok === true && teamPolicy.candidateVersion === version && teamPolicy.binarySha256 === team.binarySha256 && teamPolicy.imageId === image.imageId, 'Team runtime acceptance mismatch');
  for (const transport of ['verified-tls-postgres','unix-socket-postgres']) need(teamPolicy.checks?.[transport]?.ownerSelection043 === true && teamPolicy.checks[transport].oldClientsReceive426 === true && teamPolicy.checks[transport]['043LoginAndRuntime'] === true, 'Team policy transport acceptance mismatch');
  const upgrade = read('TEAM-UPGRADE-64-66-EVIDENCE.json');
  need(upgrade.ok === true && upgrade.previousReleaseSchema === 64 && upgrade.targetSchema === 66 && JSON.stringify(upgrade.steps?.map(step => [step.sourceSchema,step.targetSchema])) === '[[64,65],[65,66]]' && upgrade.steps.every(step => step.phase === 'ready' && step.backupSha256Verified === true && step.realRestoreVerified === true && step.retainedSyntheticRowVerifiedNow === true && step.allAppliedMigrationHashesMatchCurrentPackage === true), 'Team retained-data upgrade evidence mismatch');
  return manifest;
}
