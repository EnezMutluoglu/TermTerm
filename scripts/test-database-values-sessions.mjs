import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes,createHash} from 'node:crypto';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
async function pgAdmin(sql){const file=path.resolve('.lab/database/values-admin.sql');await writeFile(file,sql);if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',file],{stdio:'pipe'});else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f','/mnt/c/Users/sonx/Desktop/termterm/.lab/database/values-admin.sql'],{stdio:'pipe',windowsHide:true});}
try {for(const original of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
 const profile={...original};if(process.platform==='linux'&&profile.caPath)profile.caPath=path.resolve('.lab/ca.crt');
 const engine=profile.engine,schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='tt_full_'+randomBytes(4).toString('hex'),qname=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=qname(schema)+'.'+qname(table),tests=[],ids=[];
 const open=async()=>{const r=await rpc({op:'open',profile});ids.push(r.sessionId);return r.sessionId;};let id;
 try{
  id=await open();const q=(sql,parameters={},sessionId=id)=>rpc({op:'query',sessionId,sql,parameters});
  await q(`CREATE TABLE ${target}(id INT PRIMARY KEY,txt ${engine==='oracle'?'CLOB':engine==='mssql'?'NVARCHAR(MAX)':'TEXT'},data ${engine==='oracle'?'BLOB':engine==='mssql'?'VARBINARY(MAX)':'BYTEA'},counter INT)`);
  const text='İstanbul 😀,\n<script>literal</script>\r\n'.repeat(700),binary=Buffer.from(Array.from({length:256},(_,i)=>i)),hex=binary.toString('hex');
  await q(`INSERT INTO ${target}(id,txt,data,counter) VALUES(1,:value,${engine==='postgresql'?`decode('${hex}','hex')`:engine==='mssql'?`CONVERT(VARBINARY(MAX),'${hex}',2)`:`HEXTORAW('${hex}')`},0)`,{value:text});
  await q(`INSERT INTO ${target}(id,txt,counter) VALUES(2,NULL,0)`);
  const page=await rpc({op:'browse',sessionId:id,schema,table,pageSize:100}),col=n=>page.columns.find(c=>c.name.toLowerCase()===n).name;
  assert.ok(page.largeValues.columns.includes(col('txt')));assert.ok(page.largeValues.columns.includes(col('data')));assert.equal(page.editing.enabled,false);tests.push('bounded-browse-advertises-explicit-full-value-read');
  const full=(column,key='1',sessionId=id)=>rpc({op:'lobRead',sessionId,schema,table,column:col(column),keys:{[col('id')]:key}});
  const t=await full('txt');assert.equal(Buffer.from(t.base64,'base64').toString('utf8'),text);assert.equal(t.bytes,Buffer.byteLength(text));assert.equal(t.sha256,createHash('sha256').update(text).digest('hex'));tests.push('full-unicode-clob-exact-byte-length-and-sha256');
  const b=await full('data');assert.equal(b.binary,true);assert.deepEqual(Buffer.from(b.base64,'base64'),binary);tests.push('binary-all-byte-values-exact');
  assert.equal((await full('txt','2')).isNull,true);await q(`UPDATE ${target} SET txt='' WHERE id=2`);const empty=await full('txt','2');assert.equal(empty.bytes,0);assert.equal(empty.isNull,engine==='oracle');tests.push('null-empty-engine-semantics');
  await assert.rejects(()=>full('txt','1 OR 1=1'));await assert.rejects(()=>rpc({op:'lobRead',sessionId:id,schema,table,column:'txt;DROP TABLE x',keys:{[col('id')]:'1'}}));await assert.rejects(()=>rpc({op:'lobRead',sessionId:id,schema,table,column:col('txt'),keys:{}}));tests.push('identifier-key-injection-and-missing-key-rejected');
  await q(`UPDATE ${target} SET txt='new value' WHERE id=1`);assert.equal(Buffer.from((await full('txt')).base64,'base64').toString(),'new value');await q(`DELETE FROM ${target} WHERE id=2`);await assert.rejects(()=>full('txt','2'));tests.push('fresh-value-and-deleted-row');
  if(engine==='oracle')await q(`INSERT INTO ${target}(id,txt,counter) VALUES(3,EMPTY_CLOB(),0)`);else await q(`INSERT INTO ${target}(id,txt,counter) VALUES(3,${engine==='postgresql'?"repeat('x',8388609)":"REPLICATE(CAST('x' AS VARCHAR(MAX)),8388609)"},0)`);
  if(engine==='oracle')await q(`DECLARE v CLOB;BEGIN SELECT txt INTO v FROM ${target} WHERE id=3 FOR UPDATE;FOR i IN 1..257 LOOP DBMS_LOB.WRITEAPPEND(v,32767,RPAD('x',32767,'x'));END LOOP;END;`);
  await assert.rejects(()=>full('txt','3'),/8 MiB/);tests.push('over-limit-rejected-without-partial-export');
  const other=await open();await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:false});await q(`UPDATE ${target} SET counter=1 WHERE id=1`);
  // Reading through the second session must retain the editor transaction.
  const evidence=await rpc({op:'evidence',sessionId:other,panel:'connections'});assert.ok(evidence.parts.every(p=>!['failed','permission_denied','unsupported'].includes(p.status)),JSON.stringify(evidence.parts));const tx=evidence.parts.find(p=>p.id==='transactions');assert.equal(tx.status,'available');assert.ok(tx.rows.length>0);assert.equal((await rpc({op:'status',sessionId:id})).transaction,true);tests.push('real-open-transaction-visible-with-age-and-groups');
  await rpc({op:'transaction',sessionId:id,action:'rollback'});await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:true});assert.equal((await q(`SELECT counter FROM ${target} WHERE id=1`)).results[0].rows[0][0],'0');tests.push('monitoring-does-not-commit-editor-transaction');
  const username='tt_lob_'+randomBytes(4).toString('hex'),password='Tt9!'+randomBytes(20).toString('hex');let reader,created=false;
  try{
   if(engine==='postgresql')await pgAdmin(`CREATE ROLE ${username} LOGIN PASSWORD '${password}';`);
   else if(engine==='mssql'){await q(`CREATE LOGIN ${username} WITH PASSWORD='${password}'`);await q(`CREATE USER ${username} FOR LOGIN ${username}`);}
   else{await q(`CREATE USER ${username} IDENTIFIED BY "${password}"`);await q(`GRANT CREATE SESSION TO ${username}`);}created=true;
   await q(`GRANT SELECT ON ${target} TO ${username}`);reader=(await rpc({op:'open',profile:{...profile,username,password}})).sessionId;ids.push(reader);
   assert.equal(Buffer.from((await full('txt','1',reader)).base64,'base64').toString(),'new value');
   const restricted=await rpc({op:'evidence',sessionId:reader,panel:'connections'}),capacity=restricted.parts.find(p=>p.id==='capacity');
   if(engine==='oracle')assert.ok(['permission_denied','unsupported'].includes(capacity.status));else{const column=capacity.columns.findIndex(c=>['client_connections','user_sessions'].includes(c.name.toLowerCase()));assert.equal(capacity.rows[0][column],null);}
   tests.push('restricted-full-read-and-unknown-global-connection-total');
   await q(`REVOKE SELECT ON ${target} FROM ${username}`);await assert.rejects(()=>full('txt','1',reader));tests.push('revoked-read-permission-rechecked');
  }finally{if(reader)await rpc({op:'close',sessionId:reader}).catch(()=>{});if(created){if(engine==='postgresql')await pgAdmin(`DROP OWNED BY ${username};DROP ROLE ${username};`);else{await q(`DROP USER ${username}`);if(engine==='mssql')await q(`DROP LOGIN ${username}`);}}}
  const noKey=target.replace(qname(table),qname(table+'_nokey'));await q(`CREATE TABLE ${noKey}(txt ${engine==='oracle'?'CLOB':'TEXT'})`);
  try{await assert.rejects(()=>rpc({op:'lobRead',sessionId:id,schema,table:table+'_nokey',column:'txt',keys:{}}));}finally{await q(`DROP TABLE ${noKey}`);}tests.push('keyless-table-read-rejected');
  await rpc({op:'close',sessionId:other});await assert.rejects(()=>full('txt','1',other));tests.push('closed-session-full-value-rejected');
  report.push({engine,status:'passed',tests});
 }catch(e){report.push({engine,status:'failed',tests,error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally{if(id){await rpc({op:'transaction',sessionId:id,action:'rollback'}).catch(()=>{});await rpc({op:'transaction',sessionId:id,action:'autocommit',enabled:true}).catch(()=>{});await rpc({op:'query',sessionId:id,sql:`DROP TABLE ${target}`}).catch(()=>{});}for(const sessionId of ids)await rpc({op:'close',sessionId}).catch(()=>{});}
}}finally{close();}
await writeFile(`artifacts/database-workspace/${process.platform==='linux'?'linux-':''}values-sessions-tests.json`,JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
