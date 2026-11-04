import {readFile,writeFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {randomBytes} from 'node:crypto';
import path from 'node:path';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
const catalog=JSON.parse(await readFile('src/database/catalog.json','utf8')),contracts=JSON.parse(await readFile('src/database/actionInputs.json','utf8'));
async function pgAdmin(sql){const file=path.resolve('.lab/database/evidence-admin.sql');await writeFile(file,sql);if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',file],{stdio:'pipe'});else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f','/mnt/c/Users/sonx/Desktop/termterm/.lab/database/evidence-admin.sql'],{stdio:'pipe',windowsHide:true});}
try{for(const original of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
 const profile={...original};if(process.platform==='linux'&&profile.caPath)profile.caPath=path.resolve('.lab/ca.crt');
 const engine=profile.engine,schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',suffix=randomBytes(4).toString('hex'),table='TT_%_İ_'+suffix,quote=n=>engine==='mssql'?'['+n.replaceAll(']',']]')+']':'"'+n.replaceAll('"','""')+'"',target=quote(schema)+'.'+quote(table),sessions=[],tests=[];
 const open=async p=>{const r=await rpc({op:'open',profile:p});sessions.push(r.sessionId);return r.sessionId;};let id;
 try{
  id=await open(profile);const q=(sql,sessionId=id)=>rpc({op:'query',sessionId,sql});
  await q(`CREATE TABLE ${target} (${quote('id')} INT PRIMARY KEY,${quote('İ, sütun')} INT)`);await q(`INSERT INTO ${target} VALUES(1,42)`);
  await q(`CREATE TABLE ${quote(schema)}.${quote(table+'extra')} (${quote('sibling_only')} INT)`);
  assert.ok((await rpc({op:'metadataOptions',sessionId:id,kind:'schemas'})).items.includes(schema));
  assert.ok((await rpc({op:'metadataOptions',sessionId:id,kind:'tables',schema})).items.includes(table));
  assert.deepEqual((await rpc({op:'metadataOptions',sessionId:id,kind:'columns',schema,table})).items,['id','İ, sütun']);tests.push('unicode-comma-literal-wildcard-exact-metadata');
  assert.deepEqual((await rpc({op:'metadataOptions',sessionId:id,kind:'columns',schema,table:'missing_'+suffix})).items,[]);await assert.rejects(()=>rpc({op:'metadataOptions',sessionId:id,kind:'sql',schema,table}));tests.push('missing-object-and-invalid-kind');
  const actionId=engine==='postgresql'?'PG-MON-043':engine==='mssql'?'MS-MON-035':'OR-MON-039';
  const params={table:{schema,table},selection:['id','İ, sütun'],page_size:10,...engine!=='oracle'&&{unique_key:'id'}};
  const checked=await rpc({op:'check',sessionId:id,action:catalog.find(a=>a.id===actionId),contract:contracts[actionId],parameters:params});assert.equal(checked.status,'ok',JSON.stringify(checked));assert.equal(checked.results[0].rows[0][1],'42');tests.push('metadata-selected-columns-execute-real-catalog');
  const result=await rpc({op:'evidence',sessionId:id,panel:'indexes',schema,table});
  assert.ok(result.parts.find(p=>p.id==='indexes').rows.length);assert.ok(result.parts.every(p=>!['failed','permission_denied','partial'].includes(p.status)),JSON.stringify(result.parts));tests.push('index-dependencies-usage-size-and-epoch-sources');
  await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:false});await q(`UPDATE ${target} SET ${quote('İ, sütun')}=99`);
  await rpc({op:'evidence',sessionId:id,panel:'indexes',schema,table});assert.equal((await rpc({op:'status',sessionId:id})).transaction,true);await rpc({op:'transaction',sessionId:id,action:'rollback'});await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:true});assert.equal((await q(`SELECT ${quote('İ, sütun')} FROM ${target}`)).results[0].rows[0][0],'42');tests.push('evidence-isolated-from-editor-transaction');
  const backups=await rpc({op:'evidence',sessionId:id,panel:'backups'});assert.ok(backups.parts.every(p=>!['failed','permission_denied'].includes(p.status)),JSON.stringify(backups.parts));assert.ok(backups.parts.some(p=>p.id==='restores'));if(engine==='postgresql')assert.equal(backups.parts.find(p=>p.id==='backups').status,'unsupported');tests.push('backup-sources-and-unknown-not-healthy');
  await assert.rejects(()=>rpc({op:'evidence',sessionId:id,panel:'sql',sql:'DROP TABLE x'}));await assert.rejects(()=>rpc({op:'evidence',sessionId:id,panel:'indexes',schema,table:'x;DROP TABLE x'}));tests.push('read-only-operation-allowlist-and-identifiers');
  const user='tt_ev_'+suffix,password='Tt9!'+randomBytes(20).toString('hex');let reader,created=false;
  try{
   if(engine==='postgresql')await pgAdmin(`CREATE ROLE ${user} LOGIN PASSWORD '${password}';`);
   else if(engine==='mssql'){await q(`CREATE LOGIN ${user} WITH PASSWORD='${password}'`);await q(`CREATE USER ${user} FOR LOGIN ${user}`);}
   else{await q(`CREATE USER ${user} IDENTIFIED BY "${password}"`);await q(`GRANT CREATE SESSION TO ${user}`);}created=true;
   await q(`GRANT SELECT ON ${target} TO ${user}`);reader=await open({...profile,username:user,password});
   assert.deepEqual((await rpc({op:'metadataOptions',sessionId:reader,kind:'columns',schema,table})).items,['id','İ, sütun']);
   const r=await rpc({op:'evidence',sessionId:reader,panel:'indexes',schema,table});assert.equal(r.parts.find(p=>p.id==='indexes').status,'available');
   if(engine!=='postgresql')assert.ok(r.parts.some(p=>p.status==='permission_denied'||p.status==='unsupported'),JSON.stringify(r));
   await assert.rejects(()=>q(`UPDATE ${target} SET ${quote('İ, sütun')}=7`,reader));tests.push('restricted-metadata-and-partial-evidence-no-write-permission');
  }finally{if(reader)await rpc({op:'close',sessionId:reader}).catch(()=>{});if(created){if(engine==='postgresql')await pgAdmin(`DROP OWNED BY ${user};DROP ROLE ${user};`);else{await q(`DROP USER ${user}`);if(engine==='mssql')await q(`DROP LOGIN ${user}`);}}}
  await rpc({op:'close',sessionId:id});await assert.rejects(()=>rpc({op:'evidence',sessionId:id,panel:'backups'}));tests.push('closed-session-rejected');
  id=await open(profile);report.push({engine,status:'passed',tests});
 }catch(e){report.push({engine,status:'failed',tests,error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally{if(id){await rpc({op:'transaction',sessionId:id,action:'rollback'}).catch(()=>{});for(const name of [table,table+'extra'])await rpc({op:'query',sessionId:id,sql:`DROP TABLE ${quote(schema)}.${quote(name)}`}).catch(()=>{});}for(const sessionId of sessions)await rpc({op:'close',sessionId}).catch(()=>{});}
}}finally{close();}
await writeFile(`artifacts/database-workspace/${process.platform==='linux'?'linux-':''}evidence-tests.json`,JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
