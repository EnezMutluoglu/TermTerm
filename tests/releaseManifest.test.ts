import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {afterEach, expect, it} from 'vitest';
// @ts-expect-error Plain Node release script.
import {releaseAssets} from '../scripts/release-assets.mjs';
// @ts-expect-error Plain Node release script.
import {verifyReleaseManifest} from '../scripts/release-manifest.mjs';

const dirs:string[]=[];
afterEach(()=>{for(const dir of dirs.splice(0))fs.rmSync(dir,{recursive:true,force:true});});
function fixture() {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'termterm-release-'));dirs.push(dir);
  const names:string[]=releaseAssets('0.4.3').required.filter((name:string)=>name!=='RELEASE-MANIFEST.json');
  for(const name of names)fs.writeFileSync(path.join(dir,name),name);
  const hash=(name:string)=>crypto.createHash('sha256').update(fs.readFileSync(path.join(dir,name))).digest('hex');
  const write=(name:string,value:unknown)=>fs.writeFileSync(path.join(dir,name),JSON.stringify(value));
  const desktop={version:'0.4.3',identity:'local.termterm.desktop'};
  write('WINDOWS-PACKAGE-RECEIPT.json',{...desktop,binaryMetadata:{productVersion:'0.4.3',fileVersion:'0.4.3'},sourceZipSha256:hash('TermTerm-0.4.3-windows-source.zip'),files:{'TermTerm_0.4.3_x64-setup.exe':{sha256:hash('TermTerm_0.4.3_x64-setup.exe')}}});
  write('TermTerm-0.4.3-APPIMAGE-INSTALL.json',{candidateVersion:'0.4.3',applicationIdentifier:desktop.identity,imageSha256:hash('TermTerm_0.4.3_amd64.AppImage')});
  write('LINUX-PACKAGE-RECEIPT.json',{...desktop,sourceSha256:hash('TermTerm-0.4.3-linux-source.zip'),appImageManifestSha256:hash('TermTerm-0.4.3-APPIMAGE-INSTALL.json'),files:Object.fromEntries(['TermTerm_0.4.3_amd64.AppImage','TermTerm_0.4.3_amd64.deb','termterm-vpn-broker_0.4.3_amd64.deb'].map(name=>[name,{sha256:hash(name)}]))});
  write('MACOS-SOURCE-RECEIPT.json',{version:'0.4.3',files:{'termterm-vpn-source.zip':{sha256:hash('TermTerm-0.4.3-macos-source.zip')}}});
  const teamCompatibility={serviceVersion:'0.4.2',apiVersion:1,schemaVersion:66,minimumClientVersion:'0.4.1',clientVersions:['0.4.1','0.4.2','0.4.3']};
  const team={serviceVersion:'0.4.2',desktopCandidateContext:'0.4.3',apiVersion:1,schemaVersion:66,buildProfile:'release',binarySha256:'1'.repeat(64),imageId:'sha256:fixture',archive:{sha256:hash('TermTerm-Team-Service-0.4.2-vpn-0.4.3-linux-x64.tar.gz')},imageArchive:{sha256:hash('TermTerm-Team-Image-0.4.2-vpn-0.4.3-linux-x64.tar.gz')}};
  write('TEAM-CATALOGUE-DELTA.json',{after:teamCompatibility});
  write('TEAM-DELIVERY-RECEIPT.json',{...team,publicPromoted:true,compiledSourceEqualsPublicSource:true,catalogueDeltaSha256:hash('TEAM-CATALOGUE-DELTA.json')});
  write('TermTerm-Team-Image-0.4.2-vpn-0.4.3-linux-x64.json',{version:'0.4.2',archive:team.imageArchive,binarySha256:team.binarySha256,imageId:team.imageId});
  write('RELEASE-ACCEPTANCE.json',{version:'0.4.3',windowsUpgrade:true,linuxRuntime:true,teamPolicy:true,windowsInstallerSha256:hash('TermTerm_0.4.3_x64-setup.exe'),linuxAppImageSha256:hash('TermTerm_0.4.3_amd64.AppImage'),teamBinarySha256:team.binarySha256});
  write('WINDOWS-UPGRADE-ACCEPTANCE.json',{ok:true,candidateVersion:'0.4.3',installerSha256:hash('TermTerm_0.4.3_x64-setup.exe')});
  write('LINUX-RUNTIME-ACCEPTANCE.json',{ok:true,version:'0.4.3',packageReceiptSha256:hash('LINUX-PACKAGE-RECEIPT.json'),appImageSha256:hash('TermTerm_0.4.3_amd64.AppImage'),brokerSha256:hash('termterm-vpn-broker_0.4.3_amd64.deb')});
  write('TEAM-CATALOGUE-ACCEPTANCE.json',{ok:true,candidateVersion:'0.4.3',binarySha256:team.binarySha256,imageId:team.imageId,checks:Object.fromEntries(['verified-tls-postgres','unix-socket-postgres'].map(transport=>[transport,{ownerSelection043:true,oldClientsReceive426:true,'043LoginAndRuntime':true}]))});
  write('TEAM-UPGRADE-64-66-EVIDENCE.json',{ok:true,previousReleaseSchema:64,targetSchema:66,steps:[[64,65],[65,66]].map(([sourceSchema,targetSchema])=>({sourceSchema,targetSchema,phase:'ready',backupSha256Verified:true,realRestoreVerified:true,retainedSyntheticRowVerifiedNow:true,allAppliedMigrationHashesMatchCurrentPackage:true}))});
  for(const [receipt,original,released] of [
    ['WINDOWS-PACKAGE-RECEIPT.json','TermTerm-Release-Candidate-0.4.3-windows-x64-portable.zip','TermTerm-0.4.3-windows-x64-portable.zip'],
    ['WINDOWS-PACKAGE-RECEIPT.json','TermTerm-Release-Candidate-0.4.3-dependency-source.zip','TermTerm-0.4.3-windows-dependency-source.zip'],
    ['LINUX-PACKAGE-RECEIPT.json','termterm-vpn-linux-dependency-source.tar.gz','TermTerm-0.4.3-linux-dependency-source.tar.gz'],
    ['MACOS-SOURCE-RECEIPT.json','TermTerm-0.4.3-macos-source-preparation.zip','TermTerm-0.4.3-macos-source-preparation.zip'],
  ]) {const file=path.join(dir,receipt),value=JSON.parse(fs.readFileSync(file,'utf8'));value.files[original]={sha256:hash(released)};write(receipt,value);}
  const linuxReceipt=JSON.parse(fs.readFileSync(path.join(dir,'LINUX-RUNTIME-ACCEPTANCE.json'),'utf8'));linuxReceipt.packageReceiptSha256=hash('LINUX-PACKAGE-RECEIPT.json');write('LINUX-RUNTIME-ACCEPTANCE.json',linuxReceipt);
  const seal=()=>write('RELEASE-MANIFEST.json',{formatVersion:1,version:'0.4.3',teamCompatibility,assets:Object.fromEntries(names.map(name=>[name,{sha256:hash(name),bytes:fs.statSync(path.join(dir,name)).size}]))});
  const change=(name:string,edit:(value:any)=>void)=>{const value=JSON.parse(fs.readFileSync(path.join(dir,name),'utf8'));edit(value);write(name,value);};
  seal();return {dir,seal,change};
}
it('accepts a closed manifest whose package, source, installer and component receipts match',()=>{
  expect(verifyReleaseManifest(fixture().dir,'0.4.3').version).toBe('0.4.3');
});
it.each(['updater.key','private-vault.zip','../outside.zip'])('rejects unknown/private/traversal asset %s',name=>{
  const {dir,change}=fixture();
  change('RELEASE-MANIFEST.json',m=>{m.assets[name]={sha256:'0'.repeat(64),bytes:1};});
  expect(()=>verifyReleaseManifest(dir,'0.4.3')).toThrow('allowlist');
});
it('rejects missing, extra and modified files',()=>{
  const {dir}=fixture();fs.unlinkSync(path.join(dir,'install-vpn-appimage.py'));
  expect(()=>verifyReleaseManifest(dir,'0.4.3')).toThrow();
  fs.writeFileSync(path.join(dir,'install-vpn-appimage.py'),'altered');
  expect(()=>verifyReleaseManifest(dir,'0.4.3')).toThrow('hash mismatch');
  fs.writeFileSync(path.join(dir,'updater.key'),'private');
  expect(()=>verifyReleaseManifest(dir,'0.4.3')).toThrow('Unknown release asset');
});
it.each(['version','source','metadata','component','catalogue','acceptance'])('rejects a resealed %s mismatch',kind=>{
  const {dir,change,seal}=fixture();
  if(kind==='version')change('WINDOWS-PACKAGE-RECEIPT.json',m=>{m.version='0.4.2';});
  if(kind==='source')change('LINUX-PACKAGE-RECEIPT.json',m=>{m.sourceSha256='0'.repeat(64);});
  if(kind==='metadata')change('WINDOWS-PACKAGE-RECEIPT.json',m=>{m.binaryMetadata.fileVersion='0.4.2';});
  if(kind==='component')change('TEAM-DELIVERY-RECEIPT.json',m=>{m.serviceVersion='0.4.3';});
  if(kind==='catalogue')change('TEAM-CATALOGUE-DELTA.json',m=>{m.after.minimumClientVersion='0.4.3';});
  if(kind==='acceptance')change('RELEASE-ACCEPTANCE.json',m=>{m.windowsUpgrade=false;});
  seal();expect(()=>verifyReleaseManifest(dir,'0.4.3')).toThrow();
});
it('keeps explicit approval, exact commit and clean main guards before any feed write',()=>{
  const script=path.resolve('scripts/publish-approved-release.mjs');
  expect(()=>execFileSync(process.execPath,[script],{stdio:'pipe'})).toThrow('Explicit owner-approved');
  expect(()=>execFileSync(process.execPath,[script,'--approved-version','0.4.3'],{stdio:'pipe'})).toThrow('exact current commit');
  const sha=execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
  expect(()=>execFileSync(process.execPath,[script,'--approved-version','0.4.3','--approved-commit',sha],{stdio:'pipe'})).toThrow('clean owner-approved main');
});
