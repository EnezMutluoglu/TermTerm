import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
try{for(const originalProfile of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
 const profile={...originalProfile};if(process.platform==='linux'&&profile.caPath)profile.caPath=path.resolve('.lab/ca.crt');
 const engine=profile.engine,schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='TT_edit_'+randomBytes(4).toString('hex'),quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=quote(schema)+'.'+quote(table),tests=[],sessions=[];
 const open=async p=>{const r=await rpc({op:'open',profile:p});sessions.push(r.sessionId);return r.sessionId;};let owner;
 try{
  owner=await open(profile);const q=(sql,sessionId=owner)=>rpc({op:'query',sessionId,sql});
  await q(`CREATE TABLE ${target} (id DECIMAL(20,0) PRIMARY KEY, amount DECIMAL(28,4) CHECK(amount>=0), label ${engine==='oracle'?'NVARCHAR2(100)':'NVARCHAR(100)'}, note ${engine==='oracle'?'VARCHAR2(100)':'VARCHAR(100)'})`.replace(engine==='postgresql'?'NVARCHAR':'__unused__','VARCHAR'));
  await q(`INSERT INTO ${target}(id,amount,label,note) VALUES(9007199254740993,12.3456,'initial',NULL)`);
  await q(`INSERT INTO ${target}(id,amount,label,note) VALUES(2,1,'second',NULL)`);
  const page=await rpc({op:'browse',sessionId:owner,schema,table,pageSize:100});assert.equal(page.editing.enabled,true,page.editing.reason);
  const col=n=>page.columns.find(c=>c.name.toLowerCase()===n).name,keys={[col('id')]:'9007199254740993'};
  const edit=(sessionId=owner)=>rpc({op:'rowEditOpen',sessionId,schema,table,keys});
  const preview=(d,changes,sessionId=owner)=>rpc({op:'rowEditPreview',sessionId,draftId:d.draftId,changes});
  const apply=(d,p,sessionId=owner)=>rpc({op:'rowEditApply',sessionId,draftId:d.draftId,planId:p.planId});
  const current=async()=>{const r=await q(`SELECT amount,label,note FROM ${target} WHERE id=9007199254740993`);return r.results[0].rows[0];};
  let d=await edit();assert.equal(d.original[col('id')],'9007199254740993');assert.equal(d.original[col('note')],null);tests.push('fresh-row-exact-key-and-null');
  const text="İstanbul'; DROP TABLE never_run; --";
  let p=await preview(d,{[col('amount')]:'9007199254740993.1234',[col('label')]:text,[col('note')]:null});assert.ok(p.sql.includes('?'));assert.ok(!p.sql.includes(text));assert.equal((await current())[0],'12.3456');tests.push('preview-is-read-only-values-bound');
  await assert.rejects(()=>preview(d,{[col('id')]:'3'}));await assert.rejects(()=>apply(d,p));tests.push('key-change-and-stale-plan-rejected');
  p=await preview(d,{[col('amount')]:'9007199254740993.1234',[col('label')]:text});let saved=await apply(d,p);assert.equal(saved.state,'succeeded',JSON.stringify(saved));assert.equal((await current())[0],'9007199254740993.1234');assert.equal((await current())[1],text);await assert.rejects(()=>apply(d,p));tests.push('exact-decimal-unicode-literal-commit-and-replay-rejection');
  d=await edit();p=await preview(d,{[col('note')]:''});assert.equal(p.changes[0].after,engine==='oracle'?null:'');assert.equal((await apply(d,p)).state,'succeeded');assert.equal((await current())[2],engine==='oracle'?null:'');tests.push('empty-string-vs-null-engine-semantics');
  const second=await open(profile);d=await edit();const rival=await edit(second);
  p=await preview(d,{[col('label')]:'my edit'});const other=await preview(rival,{[col('note')]:'other writer'},second);assert.equal((await apply(rival,other,second)).state,'succeeded');
  const conflict=await apply(d,p);assert.equal(conflict.state,'conflict');assert.equal(conflict.current[col('note')],'other writer');assert.equal((await current())[1],text);tests.push('two-client-conflict-on-unchanged-field-no-overwrite');
  d=await edit();p=await preview(d,{[col('amount')]:'-1',[col('label')]:'must rollback'});const invalid=await apply(d,p);assert.equal(invalid.state,'failed');assert.equal(invalid.rolledBack,true);assert.equal((await current())[1],text);tests.push('constraint-error-rolls-back-all-fields');
  d=await edit();await assert.rejects(()=>preview(d,{[col('amount')]:'0.12345'}));await assert.rejects(()=>preview(d,{'fake;DROP TABLE x':'x'}));await assert.rejects(()=>preview(d,{[col('amount')]:42}));await rpc({op:'rowEditDiscard',sessionId:owner,draftId:d.draftId});await assert.rejects(()=>preview(d,{[col('label')]:'x'}));tests.push('rounding-injection-lossy-number-and-discard-rejected');
  await rpc({op:'transaction',sessionId:owner,action:'autocommit',enabled:false});await q(`UPDATE ${target} SET label='uncommitted' WHERE id=2`);
  d=await edit();p=await preview(d,{[col('label')]:'independent commit'});assert.equal((await apply(d,p)).state,'succeeded');assert.equal((await rpc({op:'status',sessionId:owner})).transaction,true);
  await rpc({op:'transaction',sessionId:owner,action:'rollback'});await rpc({op:'transaction',sessionId:owner,action:'autocommit',enabled:true});assert.equal((await current())[1],'independent commit');tests.push('row-commit-does-not-commit-editor-transaction');
  d=await edit();p=await preview(d,{[col('label')]:'blocked'});await rpc({op:'transaction',sessionId:second,action:'autocommit',enabled:false});await q(`UPDATE ${target} SET label='lock holder' WHERE id=9007199254740993`,second);
  const started=Date.now(),blocked=await apply(d,p);assert.equal(blocked.state,'failed');assert.equal(blocked.rolledBack,true);assert.ok(Date.now()-started<20000);await rpc({op:'transaction',sessionId:second,action:'rollback'});await rpc({op:'transaction',sessionId:second,action:'autocommit',enabled:true});assert.equal((await current())[1],'independent commit');tests.push('bounded-lock-wait-and-rollback');
  d=await edit();p=await preview(d,{[col('label')]:'deleted'});await q(`DELETE FROM ${target} WHERE id=9007199254740993`);assert.equal((await apply(d,p)).state,'conflict');tests.push('deleted-row-conflict');
  d=await rpc({op:'rowEditOpen',sessionId:owner,schema,table,keys:{[col('id')]:'2'}});p=await preview(d,{[col('label')]:'stale ddl'});
  await assert.rejects(()=>apply(d,p,second));
  await q(`ALTER TABLE ${target} ADD extra_flag INT`);assert.equal((await apply(d,p)).state,'conflict');tests.push('cross-session-plan-and-schema-change-rejected');
  const generated=quote(schema)+'.'+quote(table+'_gen');
  await q(`CREATE TABLE ${generated}(id INT PRIMARY KEY, value INT, calculated ${engine==='postgresql'?'INT GENERATED ALWAYS AS (value*2) STORED':engine==='mssql'?'AS (value*2)':'GENERATED ALWAYS AS (value*2) VIRTUAL'})`);
  try{await q(`INSERT INTO ${generated}(id,value) VALUES(1,3)`);const gd=await rpc({op:'rowEditOpen',sessionId:owner,schema,table:table+'_gen',keys:{[col('id')]:'1'}});const gc=gd.columns.find(c=>c.name.toLowerCase()==='calculated');assert.equal(gc.readOnly,true);await assert.rejects(()=>preview(gd,{[gc.name]:'999'}));await rpc({op:'rowEditDiscard',sessionId:owner,draftId:gd.draftId});}finally{await q(`DROP TABLE ${generated}`);}tests.push('generated-column-read-only');
  const typed=quote(schema)+'.'+quote(table+'_typed');
  await q(`CREATE TABLE ${typed}(id INT PRIMARY KEY,date_value DATE,time_value ${engine==='mssql'?'DATETIME2(6)':'TIMESTAMP(6)'},flag ${engine==='postgresql'?'BOOLEAN':engine==='mssql'?'BIT':'NUMBER'})`);
  try{
   await q(`INSERT INTO ${typed}(id) VALUES(1)`);
   const td=await rpc({op:'rowEditOpen',sessionId:owner,schema,table:table+'_typed',keys:{[col('id')]:'1'}}),name=n=>td.columns.find(c=>c.name.toLowerCase()===n).name;
   const changes={[name('date_value')]:'2026-10-05',[name('time_value')]:'2026-10-05 12:34:56.123456',[name('flag')]:engine==='oracle'?'1':'true'};
   const tp=await preview(td,changes);const result=await apply(td,tp);assert.equal(result.state,'succeeded',JSON.stringify(result));assert.ok(result.row[name('time_value')].includes('12:34:56.123456'));
  }finally{await q(`DROP TABLE ${typed}`);}tests.push('date-timestamp-boolean-oracle-number-roundtrip');
  const view=quote(schema)+'.'+quote(table+'_view');await q(`CREATE VIEW ${view} AS SELECT * FROM ${target}`);
  try{await assert.rejects(()=>rpc({op:'rowEditOpen',sessionId:owner,schema,table:table+'_view',keys:{[col('id')]:'2'}}));}finally{await q(`DROP VIEW ${view}`);}tests.push('view-read-only');
  const unsafe=quote(schema)+'.'+quote(table+'_lob');await q(`CREATE TABLE ${unsafe}(id INT PRIMARY KEY,data ${engine==='postgresql'?'BYTEA':engine==='mssql'?'VARBINARY(MAX)':'CLOB'})`);
  try{await assert.rejects(()=>rpc({op:'rowEditOpen',sessionId:owner,schema,table:table+'_lob',keys:{[col('id')]:'1'}}));}finally{await q(`DROP TABLE ${unsafe}`);}tests.push('unsupported-full-row-type-read-only');
  const longKey=quote(schema)+'.'+quote(table+'_longkey');await q(`CREATE TABLE ${longKey}(id ${engine==='oracle'?'VARCHAR2':'VARCHAR'}(3000) PRIMARY KEY,value INT)`);
  try{const page=await rpc({op:'browse',sessionId:owner,schema,table:table+'_longkey',pageSize:100});assert.equal(page.editing.enabled,false);assert.equal(page.stable,false);await assert.rejects(()=>rpc({op:'rowEditOpen',sessionId:owner,schema,table:table+'_longkey',keys:{[col('id')]:'1'}}));}finally{await q(`DROP TABLE ${longKey}`);}tests.push('potentially-truncated-key-not-editable-or-pageable');
  const noKey=quote(schema)+'.'+quote(table+'_nokey');await q(`CREATE TABLE ${noKey}(id INT)`);
  try{await assert.rejects(()=>rpc({op:'rowEditOpen',sessionId:owner,schema,table:table+'_nokey',keys:{[col('id')]:'1'}}));}finally{await q(`DROP TABLE ${noKey}`);}tests.push('missing-primary-key-read-only');
  {
   const user='tt_edit_'+randomBytes(5).toString('hex'),password='Tt9!'+randomBytes(20).toString('hex');let reader;
   try{
    if(engine==='postgresql'){
     const sqlFile=path.resolve(`.lab/database/row-edit-role-${user}.sql`);await writeFile(sqlFile,`CREATE ROLE ${user} LOGIN PASSWORD '${password}';`);
     if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-v','ON_ERROR_STOP=1','-f',sqlFile],{stdio:'pipe'});
     else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-v','ON_ERROR_STOP=1','-f',`/mnt/c/Users/sonx/Desktop/termterm/.lab/database/row-edit-role-${user}.sql`],{stdio:'pipe',windowsHide:true});
    }else if(engine==='mssql'){await q(`CREATE LOGIN ${user} WITH PASSWORD='${password}'`);await q(`CREATE USER ${user} FOR LOGIN ${user}`);}
    else {await q(`CREATE USER ${user} IDENTIFIED BY "${password}"`);await q(`GRANT CREATE SESSION TO ${user}`);}
    await q(`GRANT SELECT ON ${target} TO ${user}`);reader=await open({...profile,username:user,password});
    const rd=await rpc({op:'rowEditOpen',sessionId:reader,schema,table,keys:{[col('id')]:'2'}}),rp=await preview(rd,{[col('label')]:'forbidden'},reader);const result=await apply(rd,rp,reader);assert.equal(result.state,'failed');assert.equal(result.rolledBack,true);tests.push('read-only-account-write-denied');
   }finally{if(reader)await rpc({op:'close',sessionId:reader}).catch(()=>{});if(engine==='postgresql'){
     const sqlFile=path.resolve(`.lab/database/row-edit-role-${user}.sql`);await writeFile(sqlFile,`DROP OWNED BY ${user};DROP ROLE ${user};`);
     if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',sqlFile],{stdio:'pipe'});
     else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',`/mnt/c/Users/sonx/Desktop/termterm/.lab/database/row-edit-role-${user}.sql`],{stdio:'pipe',windowsHide:true});
    }else {await q(`DROP USER ${user}`).catch(()=>{});if(engine==='mssql')await q(`DROP LOGIN ${user}`).catch(()=>{});}}
  }
  report.push({engine,status:'passed',tests});
 }catch(e){report.push({engine,status:'failed',tests,error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally{for(const id of sessions.slice(1))await rpc({op:'close',sessionId:id}).catch(()=>{});if(owner){await rpc({op:'transaction',sessionId:owner,action:'rollback'}).catch(()=>{});await rpc({op:'transaction',sessionId:owner,action:'autocommit',enabled:true}).catch(()=>{});await rpc({op:'query',sessionId:owner,sql:`DROP TABLE ${target}`}).catch(()=>{});await rpc({op:'close',sessionId:owner}).catch(()=>{});}}
}}finally{close();}
await writeFile(`artifacts/database-workspace/${process.platform==='linux'?'linux-':''}row-editing-tests.json`,JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
