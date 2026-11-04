import {readFile,writeFile,readdir} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import path from 'node:path';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const root=path.resolve('artifacts/database-workspace/TermTerm-0.5.0-dev.12-windows');
const manifest=JSON.parse(await readFile(path.join(root,'manifest.json'),'utf8'));
const profiles=JSON.parse(await readFile('.lab/database/profiles.json','utf8'));
const actual=[];async function files(dir){for(const item of await readdir(dir,{withFileTypes:true})){const file=path.join(dir,item.name);if(item.isDirectory())await files(file);else if(item.name!=='manifest.json')actual.push(path.relative(root,file).replaceAll('\\','/'));}}await files(root);
assert.deepEqual(actual.sort(),manifest.map(f=>f.path).sort());
for(const file of manifest){assert.ok(!/\.ttvault|\.ttbackup|\.lab\/|\.key$|SqlScriptTest|RowEditorTest|ChangePlansTest|ProvisioningTest/.test(file.path));const bytes=await readFile(path.join(root,file.path));assert.equal(bytes.length,file.bytes);assert.equal(createHash('sha256').update(bytes).digest('hex'),file.sha256);for(const p of profiles)assert.ok(!bytes.includes(Buffer.from(p.password)),'Private lab credential found in '+file.path);}
const html=await readFile('dist/index.html','utf8');assert.ok(!html.includes('preload-helper'));
process.env.TERMTERM_DB_HELPER=path.join(root,'database-helper');const {rpc,close}=helper(),engines=[];
try{for(const p of profiles){const opened=await rpc({op:'open',profile:p});try{const r=await rpc({op:'query',sessionId:opened.sessionId,sql:p.engine==='oracle'?'SELECT 1 FROM dual':'SELECT 1'});assert.equal(r.results[0].rows[0][0],'1');assert.equal(opened.capabilities.engine,p.engine);engines.push(p.engine);}finally{await rpc({op:'close',sessionId:opened.sessionId});}}}finally{close();}
const mosh=spawnSync(path.join(root,'mosh/bin/mosh-client.exe'),['--version'],{encoding:'utf8',windowsHide:true,env:{...process.env,TERMINFO:path.join(root,'mosh/usr/share/terminfo')}});
assert.equal(mosh.status,0,mosh.stderr);assert.match(mosh.stdout+mosh.stderr,/mosh/i);
const worker=spawnSync(path.join(root,'TermTerm-Database-Dev.exe'),['--termterm-keepass-worker'],{cwd:root,encoding:'utf8',windowsHide:true,timeout:45000,input:JSON.stringify({path:path.resolve('.lab/keepass-fixtures/sample.kdbx'),password:'test-only-password',key_file:path.resolve('.lab/keepass-fixtures/sample.keyx')})});
assert.equal(worker.status,0,'Packaged KeePass reader failed');const imported=JSON.parse(worker.stdout);assert.ok(!imported.error,imported.error);assert.equal(imported.preview.records.filter(r=>r.kind==='host').length,1);
const report={keepass:'packaged-isolated-reader-passed',mosh:'packaged-runtime-passed',date:new Date().toISOString(),version:'0.5.0-dev.12',files:manifest.length,bytes:manifest.reduce((n,f)=>n+f.bytes,0),manifest:'passed',labSecrets:'absent',testFrontend:'absent',bundledJavaConnections:engines};
await writeFile('artifacts/database-workspace/package-dev12-validation.json',JSON.stringify(report,null,2));console.log(report);
