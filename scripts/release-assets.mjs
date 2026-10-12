// Explicit release allowlist: generated files and signatures, never private keys.
export function releaseAssets(version, {includeMacos = false} = {}) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw Error('Stable version required');
  const platforms = {
    'windows-x86_64': `TermTerm_${version}_x64-setup.exe`,
    'linux-x86_64': `TermTerm_${version}_amd64.AppImage`,
  };
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
