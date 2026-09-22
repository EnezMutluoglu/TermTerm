// Explicit release allowlist: generated files and signatures, never private keys.
export function releaseAssets(version) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw Error('Stable version required');
  const platforms = {
    'windows-x86_64': `TermTerm_${version}_x64-setup.exe`,
    'linux-x86_64': `TermTerm_${version}_amd64.AppImage`,
    'darwin-x86_64': `TermTerm-${version}-macos-x64.app.tar.gz`,
    'darwin-aarch64': `TermTerm-${version}-macos-arm64.app.tar.gz`,
  };
  const required = [
    ...Object.values(platforms).flatMap(name => [name, `${name}.sig`]),
    `TermTerm-${version}-windows-x64-portable.zip`, `TermTerm-${version}-source.zip`,
    `TermTerm_${version}_amd64.deb`, `TermTerm-${version}-1.x86_64.rpm`,
    `TermTerm_${version}_x64.dmg`, `TermTerm_${version}_aarch64.dmg`,
    `TermTerm-${version}-macos-x64.app.zip`, `TermTerm-${version}-macos-arm64.app.zip`,
    'RELEASE-OKU.md', `VALIDATION_${version}.md`,
  ];
  return {platforms, required};
}
