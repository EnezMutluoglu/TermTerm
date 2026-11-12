// Explicit release allowlist: generated files and signatures, never private keys.
export function releaseAssets(version, {includeMacos = false} = {}) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw Error('Stable version required');
  const platforms = {
    'windows-x86_64': `TermTerm_${version}_x64-setup.exe`,
    'linux-x86_64': `TermTerm_${version}_amd64.AppImage`,
  };
  if (['0.4.4', '0.4.5'].includes(version)) {
    if (includeMacos) throw Error(`${version} provides Windows and Linux packages only`);
    return {platforms, required: [
      ...Object.values(platforms).flatMap(name => [name, `${name}.sig`]),
      `TermTerm-${version}-windows-x64-portable.zip`, `TermTerm_${version}_amd64.deb`,
      `termterm-vpn-broker_${version}_amd64.deb`, `TermTerm-${version}-APPIMAGE-INSTALL.json`, 'install-vpn-appimage.py',
      ...['windows', 'linux'].map(target => `TermTerm-${version}-${target}-source.zip`),
      `TermTerm-${version}-windows-dependency-source.zip`, `TermTerm-${version}-linux-dependency-source.tar.gz`,
      'THIRD_PARTY.md', 'THIRD_PARTY_LICENSES.txt', 'DEPENDENCIES.json',
      'WINDOWS-PACKAGE-RECEIPT.json', 'LINUX-PACKAGE-RECEIPT.json', 'PATCH-ACCEPTANCE.json',
      'RELEASE-OKU.md', `VALIDATION_${version}.md`, 'RELEASE-MANIFEST.json',
    ]};
  }
  if (version === '0.4.3') {
    if (includeMacos) throw Error('0.4.3 provides macOS source only');
    return {platforms, required: [
      ...Object.values(platforms).flatMap(name => [name, `${name}.sig`]),
      'TermTerm-0.4.3-windows-x64-portable.zip', 'TermTerm_0.4.3_amd64.deb',
      'termterm-vpn-broker_0.4.3_amd64.deb', 'TermTerm-0.4.3-APPIMAGE-INSTALL.json', 'install-vpn-appimage.py',
      ...['windows', 'linux', 'macos'].map(target => `TermTerm-0.4.3-${target}-source.zip`),
      'TermTerm-0.4.3-windows-dependency-source.zip', 'TermTerm-0.4.3-linux-dependency-source.tar.gz',
      'TermTerm-0.4.3-macos-source-preparation.zip', 'THIRD_PARTY.md', 'THIRD_PARTY_LICENSES.txt', 'DEPENDENCIES.json',
      'TermTerm-Team-Service-0.4.2-vpn-0.4.3-linux-x64.tar.gz',
      'TermTerm-Team-Image-0.4.2-vpn-0.4.3-linux-x64.tar.gz', 'TermTerm-Team-Image-0.4.2-vpn-0.4.3-linux-x64.json',
      'TEAM-RUNTIME-LICENSE-NOTICES.tar.gz', 'TEAM-RUNTIME-PACKAGES-SOURCES.txt',
      'TEAM_SERVICE_INSTALL_TR.md', 'TEAM_041_GUIDE_TR.md', 'TEAM_UPGRADES_TR.md', 'TEAM_043_VPN_DELIVERY_TR.md',
      'TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md',
      'TEAM_SOCKET_KURULUM_TR.md', 'TEAM_DB_OWNER_TEMPLATE_TR.md',
      'WINDOWS-PACKAGE-RECEIPT.json', 'LINUX-PACKAGE-RECEIPT.json', 'MACOS-SOURCE-RECEIPT.json',
      'TEAM-DELIVERY-RECEIPT.json', 'TEAM-CATALOGUE-DELTA.json', 'TEAM-CATALOGUE-ACCEPTANCE.json', 'TEAM-UPGRADE-64-66-EVIDENCE.json',
      'WINDOWS-UPGRADE-ACCEPTANCE.json', 'LINUX-RUNTIME-ACCEPTANCE.json', 'RELEASE-ACCEPTANCE.json',
      'RELEASE-OKU.md', 'VALIDATION_0.4.3.md', 'RELEASE-MANIFEST.json',
    ]};
  }
  if (includeMacos) Object.assign(platforms, {
    'darwin-x86_64': `TermTerm-${version}-macos-x64.app.tar.gz`,
    'darwin-aarch64': `TermTerm-${version}-macos-arm64.app.tar.gz`,
  });
  const required = [
    ...Object.values(platforms).flatMap(name => [name, `${name}.sig`]),
    `TermTerm-${version}-windows-x64-portable.zip`, `TermTerm-${version}-source.zip`,
    `TermTerm_${version}_amd64.deb`, `TermTerm-${version}-1.x86_64.rpm`,
    ...(includeMacos ? [
      `TermTerm_${version}_x64.dmg`, `TermTerm_${version}_aarch64.dmg`,
      `TermTerm-${version}-macos-x64.app.zip`, `TermTerm-${version}-macos-arm64.app.zip`,
    ] : []),
    'RELEASE-OKU.md', `VALIDATION_${version}.md`,
  ];
  const [major, minor, patch] = version.split('.').map(Number);
  if (major > 0 || minor > 4 || (minor === 4 && patch >= 1)) {
    required.push(`TermTerm-Team-Service-${version}-linux-x64.tar.gz`,
      'TEAM_SERVICE_INSTALL_TR.md', 'TEAM_041_GUIDE_TR.md', 'TEAM_UPGRADES_TR.md', 'SOURCE_FINGERPRINT.json');
  }
  // The already-published 0.4.1 assets remain immutable; future releases include the image.
  if (major > 0 || minor > 4 || (minor === 4 && patch >= 2)) {
    required.push(`TermTerm-Team-Image-${version}-linux-x64.tar.gz`, `TermTerm-Team-Image-${version}-linux-x64.json`,
      'TEAM_SOCKET_KURULUM_TR.md', 'TEAM_DB_OWNER_TEMPLATE_TR.md');
  }
  return {platforms, required};
}
