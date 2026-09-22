// Never invoked by CI. Run only after the owner explicitly approves this version.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { releaseAssets } from './release-assets.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const args=process.argv.slice(2);
const get=(key)=>args[args.indexOf(key)+1];
const version=JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8')).version;
const git=(...a)=>execFileSync('git',a,{cwd:root,encoding:'utf8',stdio:['pipe','pipe','pipe']}).trim();
const sha=git('rev-parse','HEAD');
if(!args.includes('--approved-version')||get('--approved-version')!==version||!/^\d+\.\d+\.\d+$/.test(version)) throw Error('Explicit owner-approved stable version is required');
if(!args.includes('--approved-commit')||get('--approved-commit')!==sha) throw Error('Approval must name the exact current commit SHA');
if(git('branch','--show-current')!=='main'||git('status','--porcelain','--untracked-files=no')) throw Error('Publish only a clean owner-approved main commit');
const repo='EnezMutluoglu/TermTerm';
const dir=path.join(root,'artifacts',`release-${version}`);
const {platforms,required}=releaseAssets(version,{includeMacos:args.includes('--include-macos')});
for(const name of required) if(!fs.existsSync(path.join(dir,name))) throw Error(`Missing release asset: ${name}`);
const feed={version,approved:true,notes:fs.readFileSync(path.join(dir,'RELEASE-OKU.md'),'utf8'),pub_date:git('show','-s','--format=%cI',sha),platforms:Object.fromEntries(Object.entries(platforms).map(([target,name])=>[target,{signature:fs.readFileSync(path.join(dir,`${name}.sig`),'utf8').trim(),url:`https://github.com/${repo}/releases/download/v${version}/${name}`}]))};
fs.writeFileSync(path.join(dir,'latest.json'),JSON.stringify(feed,null,2)+'\n');
const files=[...required,'latest.json'];
fs.writeFileSync(path.join(dir,'SHA256SUMS.txt'),files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(dir,name))).digest('hex')}  ${name}`).join('\n')+'\n');
files.push('SHA256SUMS.txt');
if(!args.includes('--publish')) { console.log(`Prepared ${version} at ${sha}; no tag or release created. Add --publish only with owner approval.`); process.exit(0); }
// Use the normal GitHub credential helper without printing/storing its token.
let token=process.env.GITHUB_TOKEN;
if(!token){const credential=execFileSync('git',['-c','credential.interactive=never','credential','fill'],{cwd:root,input:'protocol=https\nhost=github.com\n\n',encoding:'utf8',stdio:['pipe','pipe','pipe']});token=credential.split('\n').find(l=>l.startsWith('password='))?.slice(9);}
if(!token) throw Error('GitHub authentication unavailable');
const headers={Authorization:`Bearer ${token}`,Accept:'application/vnd.github+json','X-GitHub-Api-Version':'2022-11-28'};
async function api(route,method='GET',body){const r=await fetch(`https://api.github.com/repos/${repo}${route}`,{method,headers:{...headers,'Content-Type':'application/json'},body:body?JSON.stringify(body):undefined});if(!r.ok)throw Error(`GitHub ${method} ${route}: HTTP ${r.status}`);return r.json();}
const remote=await api('/branches/main');
if(remote.commit.sha!==sha)throw Error('Remote main differs from the approved commit');
let release;
try{release=await api(`/releases/tags/v${version}`);}catch(e){if(!String(e).includes('HTTP 404'))throw e;}
if(release&&!release.draft)throw Error('An already published release cannot be overwritten');
release??=await api('/releases','POST',{tag_name:`v${version}`,target_commitish:sha,name:`TermTerm ${version}`,body:feed.notes,draft:true,prerelease:false});
for(const name of files){
  const bytes=fs.readFileSync(path.join(dir,name));
  const digest='sha256:'+crypto.createHash('sha256').update(bytes).digest('hex');
  const existing=release.assets?.find(a=>a.name===name);
  if(existing){
    if(existing.state==='uploaded'&&existing.size===bytes.length&&existing.digest===digest){console.log(`Verified existing draft asset: ${name}`);continue;}
    throw Error(`Draft asset ${name} differs or lacks a verified digest; review manually before retrying`);
  }
  const url=release.upload_url.replace(/\{.*$/,'')+`?name=${encodeURIComponent(name)}`;
  const r=await fetch(url,{method:'POST',headers:{...headers,'Content-Type':'application/octet-stream','Content-Length':String(bytes.length)},body:bytes});
  if(!r.ok)throw Error(`Asset upload failed: ${name}, HTTP ${r.status}; draft remains unpublished`);
  const asset=await r.json();
  if(asset.size!==bytes.length||(asset.digest&&asset.digest!==digest))throw Error(`Uploaded asset mismatch: ${name}`);
  console.log(`Uploaded ${name} (${bytes.length} bytes)`);
}
const checked=await api(`/releases/${release.id}`);
if(files.some(name=>!checked.assets.some(a=>a.name===name&&a.state==='uploaded')))throw Error('Incomplete draft; publication cancelled');
const published=await api(`/releases/${release.id}`,'PATCH',{draft:false,prerelease:false,make_latest:'true'});
console.log(`Published owner-approved release: ${published.html_url}`);
