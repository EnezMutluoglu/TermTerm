import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),tests=[],base='tt_ext_'+randomBytes(5).toString('hex'),password='Tt9!'+randomBytes(20).toString('hex');let id;
async function admin(sql){const file=path.resolve('.lab/database/extensions-admin.sql');await writeFile(file,sql);if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',file],{stdio:'pipe'});else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f','/mnt/c/Users/sonx/Desktop/termterm/.lab/database/extensions-admin.sql'],{stdio:'pipe',windowsHide:true});}
try{
 await admin(`CREATE ROLE ${base} LOGIN PASSWORD '${password}';CREATE DATABASE ${base} OWNER ${base};`);
 const p=JSON.parse(await readFile('.lab/database/profiles.json','utf8')).find(p=>p.engine==='postgresql');if(process.platform==='linux'&&p.caPath)p.caPath=path.resolve('.lab/ca.crt');
 id=(await rpc({op:'open',profile:{...p,database:base,username:base,password}})).sessionId;
 const q=sql=>rpc({op:'query',sessionId:id,sql}),preview=extension=>rpc({op:'extensionPreview',sessionId:id,extension,schema:'public'}),apply=p=>rpc({op:'extensionApply',sessionId:id,planId:p.planId,hash:p.hash});
 const env=await rpc({op:'extensionEnvironment',sessionId:id});assert.equal(env.identity[0].rolsuper,'f');assert.ok(env.extensions.some(e=>e.name==='hstore'));tests.push('read-only-provider-and-allowlist-evidence');
 await assert.rejects(()=>rpc({op:'extensionPreview',sessionId:id,extension:'hstore',schema:'public',sql:'DROP SCHEMA public'}));await assert.rejects(()=>preview('fake; DROP TABLE x'));tests.push('injected-sql-and-missing-extension-rejected');
 const memberships=JSON.stringify((await q('SELECT * FROM pg_auth_members ORDER BY roleid,member,grantor')).results);
 let plan=await preview('hstore');assert.equal(plan.roleChanges.length,0);assert.equal((await apply({...plan,hash:'tampered'})).state,'failed');tests.push('no-role-grants-in-plan-hash-verified');
 plan=await preview('hstore');await rpc({op:'extensionDiscard',sessionId:id,planId:plan.planId});assert.equal((await apply(plan)).state,'failed');tests.push('discard-and-replay-rejected');
 plan=await preview('hstore');assert.equal((await apply(plan)).state,'succeeded');assert.equal((await q("SELECT count(*) FROM pg_extension WHERE extname='hstore'")).results[0].rows[0][0],'1');assert.equal((await apply(plan)).state,'failed');tests.push('trusted-extension-real-install-without-superuser');
 assert.equal(JSON.stringify((await q('SELECT * FROM pg_auth_members ORDER BY roleid,member,grantor')).results),memberships);tests.push('role-grantor-admin-inherit-set-options-unchanged');
 await assert.rejects(()=>preview('hstore'));tests.push('existing-extension-not-overwritten');
 plan=await preview('file_fdw');assert.equal((await apply(plan)).state,'failed');assert.equal((await q("SELECT count(*) FROM pg_extension WHERE extname='file_fdw'")).results[0].rows[0][0],'0');tests.push('untrusted-extension-permission-denial-rolls-back');
 plan=await preview('pg_trgm');await q('CREATE SCHEMA evidence_change');assert.equal((await apply(plan)).state,'conflict');tests.push('changed-permission-scope-invalidates-plan');
 await rpc({op:'close',sessionId:id});await assert.rejects(()=>preview('pg_trgm'));tests.push('closed-session-rejected');id=null;
 const report={date:new Date().toISOString(),status:'passed',tests,cloud:'Azure/AWS/Huawei: no live account, not verified'};await writeFile(`artifacts/database-workspace/${process.platform==='linux'?'linux-':''}extension-tests.json`,JSON.stringify(report,null,2));console.log(report);
}finally{if(id)await rpc({op:'close',sessionId:id}).catch(()=>{});close();await admin(`DROP DATABASE IF EXISTS ${base} WITH(FORCE);DROP ROLE IF EXISTS ${base};`);}
