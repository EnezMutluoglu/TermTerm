import {it, expect} from 'vitest';
// @ts-expect-error Plain Node release script, intentionally independent of frontend build.
import {releaseAssets} from '../scripts/release-assets.mjs';
it('requires signed updater assets for all four targets and download packages for Linux/macOS',()=>{
  const plan=releaseAssets('0.3.3',{includeMacos:true});
  expect(Object.keys(plan.platforms)).toEqual(['windows-x86_64','linux-x86_64','darwin-x86_64','darwin-aarch64']);
  for(const name of Object.values(plan.platforms)) {
    expect(plan.required).toContain(name);
    expect(plan.required).toContain(`${name}.sig`);
  }
  expect(plan.required).toContain('TermTerm_0.3.3_amd64.deb');
  expect(plan.required).toContain('TermTerm-0.3.3-1.x86_64.rpm');
  expect(new Set(plan.required).size).toBe(plan.required.length);
  expect(()=>releaseAssets('0.3.3-dev.1')).toThrow();
});

it('requires a versioned Linux Team image in future releases without rewriting 0.4.1',()=>{
  expect(releaseAssets('0.4.1').required.some((name:string)=>name.includes('Team-Image'))).toBe(false);
  for(const version of ['0.4.2','0.5.0','1.0.0']) {
    const {required}=releaseAssets(version);
    expect(required).toContain(`TermTerm-Team-Image-${version}-linux-x64.tar.gz`);
    expect(required).toContain(`TermTerm-Team-Image-${version}-linux-x64.json`);
  }
});
it('does not advertise unbuilt Mac packages when macOS is deferred',()=>{
  const plan=releaseAssets('0.3.3');
  expect(Object.keys(plan.platforms)).toEqual(['windows-x86_64','linux-x86_64']);
  expect(plan.required.some((name:string)=>/macos|\.dmg|\.app\./.test(name))).toBe(false);
  for(const name of Object.values(plan.platforms)) expect(plan.required).toContain(`${name}.sig`);
});

it('requires Team service and operator guides starting with 0.4.1',()=>{
  for(const version of ['0.4.1','0.4.2','0.5.0','1.0.0']) {
    const plan=releaseAssets(version);
    expect(plan.required).toContain(`TermTerm-Team-Service-${version}-linux-x64.tar.gz`);
    for(const guide of ['TEAM_SERVICE_INSTALL_TR.md','TEAM_041_GUIDE_TR.md','TEAM_UPGRADES_TR.md','SOURCE_FINGERPRINT.json']) expect(plan.required).toContain(guide);
    expect(new Set(plan.required).size).toBe(plan.required.length);
  }
  expect(releaseAssets('0.4.0').required.some((name:string)=>name.includes('Team-Service'))).toBe(false);
});

it('keeps published 0.4.1/0.4.2 asset contracts unchanged',()=>{
  for(const version of ['0.4.1','0.4.2']) {
    const common = [`TermTerm_${version}_x64-setup.exe`, `TermTerm_${version}_x64-setup.exe.sig`,
      `TermTerm_${version}_amd64.AppImage`, `TermTerm_${version}_amd64.AppImage.sig`,
      `TermTerm-${version}-windows-x64-portable.zip`, `TermTerm-${version}-source.zip`,
      `TermTerm_${version}_amd64.deb`, `TermTerm-${version}-1.x86_64.rpm`, 'RELEASE-OKU.md', `VALIDATION_${version}.md`,
      `TermTerm-Team-Service-${version}-linux-x64.tar.gz`, 'TEAM_SERVICE_INSTALL_TR.md', 'TEAM_041_GUIDE_TR.md', 'TEAM_UPGRADES_TR.md', 'SOURCE_FINGERPRINT.json'];
    if(version==='0.4.2') common.push(`TermTerm-Team-Image-${version}-linux-x64.tar.gz`, `TermTerm-Team-Image-${version}-linux-x64.json`, 'TEAM_SOCKET_KURULUM_TR.md', 'TEAM_DB_OWNER_TEMPLATE_TR.md');
    expect(releaseAssets(version).required).toEqual(common);
  }
});

it('requires the exact 0.4.3 VPN source, broker, service and installer scope',()=>{
  const plan=releaseAssets('0.4.3');
  expect(plan.required).toContain('termterm-vpn-broker_0.4.3_amd64.deb');
  expect(plan.required).toContain('TermTerm-0.4.3-APPIMAGE-INSTALL.json');
  expect(plan.required).toContain('install-vpn-appimage.py');
  expect(plan.required).toContain('TermTerm-Team-Service-0.4.2-vpn-0.4.3-linux-x64.tar.gz');
  expect(plan.required).toContain('TermTerm-0.4.3-linux-dependency-source.tar.gz');
  expect(plan.required).toContain('TermTerm-0.4.3-macos-source-preparation.zip');
  expect(plan.required.some((name:string)=>/\.rpm$|\.dmg$|\.app\./.test(name))).toBe(false);
  expect(new Set(plan.required).size).toBe(plan.required.length);
  expect(()=>releaseAssets('0.4.3',{includeMacos:true})).toThrow('source only');
});

it('limits the 0.4.4 patch to signed Windows/Linux packages and their source and acceptance',()=>{
  const plan=releaseAssets('0.4.4');
  expect(plan.platforms).toEqual({'windows-x86_64':'TermTerm_0.4.4_x64-setup.exe','linux-x86_64':'TermTerm_0.4.4_amd64.AppImage'});
  for(const name of Object.values(plan.platforms)) expect(plan.required).toContain(`${name}.sig`);
  for(const name of ['termterm-vpn-broker_0.4.4_amd64.deb','TermTerm-0.4.4-APPIMAGE-INSTALL.json','PATCH-ACCEPTANCE.json','TermTerm-0.4.4-windows-source.zip','TermTerm-0.4.4-linux-source.zip']) expect(plan.required).toContain(name);
  expect(plan.required.some((name:string)=>/macos|Team-|TEAM-|\.rpm$|\.dmg$/.test(name))).toBe(false);
  expect(new Set(plan.required).size).toBe(plan.required.length);
  expect(()=>releaseAssets('0.4.4',{includeMacos:true})).toThrow('Windows and Linux');
});
