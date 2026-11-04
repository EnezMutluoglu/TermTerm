import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes} from 'node:crypto';
import assert from 'node:assert/strict';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
async function pgAdmin(sql){const file=path.resolve('.lab/database/plans-admin.sql');await writeFile(file,sql);if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',file],{stdio:'pipe'});else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f','/mnt/c/Users/sonx/Desktop/termterm/.lab/database/plans-admin.sql'],{stdio:'pipe',windowsHide:true});}
try{for(const original of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
 const profile={...original};if(process.platform==='linux'&&profile.caPath)profile.caPath=path.resolve('.lab/ca.crt');
 const engine=profile.engine,schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='tt_plan_'+randomBytes(5).toString('hex'),statistic='tt_stat';
 const qn=n=>engine==='mssql'?'['+n.replaceAll(']',']]')+']':'"'+n.replaceAll('"','""')+'"',target=qn(schema)+'.'+qn(table),tests=[];let id,other,version;
 const q=sql=>rpc({op:'query',sessionId:id,sql});
 const request={op:'changePreview',actionId:{postgresql:'PG-OPS-004',mssql:'MS-OPS-003',oracle:'OR-OPS-003'}[engine],schema,table,statistic:engine==='mssql'?statistic:''};
 const preview=()=>rpc({...request,sessionId:id}),apply=p=>rpc({op:'changeApply',sessionId:id,planId:p.planId,hash:p.hash});
 try{
  const opened=await rpc({op:'open',profile});id=opened.sessionId;version=opened.version;other=(await rpc({op:'open',profile})).sessionId;
  await q(`CREATE TABLE ${target}(id INT PRIMARY KEY, counter INT)`);await q(`INSERT INTO ${target} VALUES(1,0)`);
  if(engine==='mssql')await q(`CREATE STATISTICS ${qn(statistic)} ON ${target}(counter)`);
  const options=await rpc({op:'changeOptions',sessionId:id,schema,table});assert.equal(options.actionId,request.actionId);if(engine==='mssql')assert.ok(options.statistics.includes(statistic));tests.push('metadata-statistic-options');
  let p=await preview();assert.equal(p.hash.length,64);assert.equal(p.target.table,table);assert.equal(p.connection.database,profile.database);assert.equal(p.connection.username,profile.username);assert.equal(p.connection.host,profile.host);assert.equal('password' in p.connection,false);assert.ok(p.sql.includes(engine==='oracle'?'GATHER_TABLE_STATS':engine==='mssql'?'UPDATE STATISTICS':'ANALYZE'));tests.push('explicit-target-sql-baseline-preview');
  assert.equal((await rpc({op:'changeApply',sessionId:id,planId:p.planId,hash:'tampered'})).state,'failed');await assert.rejects(()=>rpc({...request,sessionId:id,sql:'DROP TABLE anything'}));await assert.rejects(()=>rpc({...request,sessionId:id,actionId:'DROP'}));tests.push('hash-sql-and-action-tampering-rejected');
  await assert.rejects(()=>rpc({...request,sessionId:id,table:table+'; DROP TABLE x'}));tests.push('invalid-identifier-target-rejected');
  p=await preview();assert.equal((await rpc({op:'changeApply',sessionId:other,planId:p.planId,hash:p.hash})).state,'failed');tests.push('plan-bound-to-session');
  await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:false});await q(`UPDATE ${target} SET counter=9`);
  // End this transaction before maintenance (Oracle gathering stats may wait on it); use
  // an unrelated editor table to prove the maintenance connection never commits editor work.
  await rpc({op:'transaction',sessionId:id,action:'rollback'});await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:true});
  const result=await apply(p);assert.equal(result.state,'succeeded',JSON.stringify(result));assert.equal(result.after.estimated_rows,'1');tests.push('real-statistics-updated-and-postread');
  assert.equal((await apply(p)).state,'failed');tests.push('same-plan-cannot-run-twice');
  p=await preview();await q(`ALTER TABLE ${target} ADD extra INT`);assert.equal((await apply(p)).state,'conflict');tests.push('concurrent-ddl-invalidates-plan');
  p=await preview();await rpc({op:'changeDiscard',sessionId:id,planId:p.planId});assert.equal((await apply(p)).state,'failed');tests.push('discard-invalidates-plan');
  p=await preview();const newer=await preview();assert.equal((await apply(p)).state,'failed');await rpc({op:'changeDiscard',sessionId:id,planId:newer.planId});tests.push('new-preview-invalidates-previous');
  if(engine==='oracle'){
   await q(`BEGIN DBMS_STATS.LOCK_TABLE_STATS('${qn(schema)}','${qn(table)}'); END;`);
   await assert.rejects(preview);await q(`BEGIN DBMS_STATS.UNLOCK_TABLE_STATS('${qn(schema)}','${qn(table)}'); END;`);tests.push('locked-oracle-statistics-rejected');
  }
  const editor=target.replace(qn(table),qn(table+'_editor'));await q(`CREATE TABLE ${editor}(id INT)`);
  try{await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:false});await q(`INSERT INTO ${editor} VALUES(42)`);p=await preview();assert.equal((await apply(p)).state,'succeeded');await rpc({op:'transaction',sessionId:id,action:'rollback'});assert.equal((await q(`SELECT COUNT(*) FROM ${editor}`)).results[0].rows[0][0],'0');tests.push('maintenance-does-not-commit-editor');}finally{await rpc({op:'transaction',sessionId:id,action:'rollback'});await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:true});await q(`DROP TABLE ${editor}`);}
  const username='tt_plan_user_'+randomBytes(4).toString('hex'),password='Tt9!'+randomBytes(20).toString('hex');let reader,created=false;
  try{
   if(engine==='postgresql')await pgAdmin(`CREATE ROLE ${username} LOGIN PASSWORD '${password}';`);
   else if(engine==='mssql'){await q(`CREATE LOGIN ${username} WITH PASSWORD='${password}'`);await q(`CREATE USER ${username} FOR LOGIN ${username}`);}
   else{await q(`CREATE USER ${username} IDENTIFIED BY "${password}"`);await q(`GRANT CREATE SESSION TO ${username}`);}created=true;
   await q(`GRANT SELECT ON ${target} TO ${username}`);reader=(await rpc({op:'open',profile:{...profile,username,password}})).sessionId;
   await assert.rejects(()=>rpc({...request,sessionId:reader}));tests.push('select-only-user-cannot-maintain');
   const privilege=engine==='postgresql'?`MAINTAIN ON ${target}`:engine==='mssql'?`ALTER ON ${target}`:'ANALYZE ANY';
   await q(`GRANT ${privilege} TO ${username}`);const authorized=await rpc({...request,sessionId:reader});
   await q(`REVOKE ${privilege} FROM ${username}`);assert.equal((await rpc({op:'changeApply',sessionId:reader,planId:authorized.planId,hash:authorized.hash})).state,'failed');tests.push('revoked-permission-rechecked-before-apply');
  }finally{if(reader)await rpc({op:'close',sessionId:reader}).catch(()=>{});if(created){if(engine==='postgresql')await pgAdmin(`DROP OWNED BY ${username};DROP ROLE ${username};`);else{await q(`DROP USER ${username}`);if(engine==='mssql')await q(`DROP LOGIN ${username}`);}}}
  await rpc({op:'close',sessionId:other});await assert.rejects(()=>rpc({...request,sessionId:other}));tests.push('closed-session-rejected');
  report.push({engine,version,status:'passed',tests});
 }catch(e){report.push({engine,status:'failed',tests,error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally{if(id){await rpc({op:'transaction',sessionId:id,action:'rollback'}).catch(()=>{});await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:true}).catch(()=>{});await q(`DROP TABLE ${target}`).catch(()=>{});await rpc({op:'close',sessionId:id}).catch(()=>{});}if(other)await rpc({op:'close',sessionId:other}).catch(()=>{});}
}}finally{close();}
await writeFile(`artifacts/database-workspace/${process.platform==='linux'?'linux-':''}change-plan-tests.json`,JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
