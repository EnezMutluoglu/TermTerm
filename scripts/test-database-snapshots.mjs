import { readFile, writeFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import path from 'node:path';
import {randomBytes} from 'node:crypto';
import { helper } from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
const profiles=JSON.parse(await readFile('.lab/database/profiles.json','utf8'));
try {for(const profile of profiles) {
 if(process.platform==='linux' && profile.engine==='postgresql'){ profile.caPath=path.resolve('.lab/ca.crt'); profile.port=55432; }
 const tests=[];let sessionId,created=false;
 const engine=profile.engine,schema='TT_Şema_%_'+Date.now().toString(36),qname=n=>engine==='mssql'?'['+n.replaceAll(']',']]')+']':'"'+n.replaceAll('"','""')+'"',qs=qname(schema),qt=qs+'.'+qname('Ödemeler_%');
 try {
  const opened=await rpc({op:'open',profile});sessionId=opened.sessionId;
  const q=sql=>rpc({op:'query',sessionId,sql});
  await q(engine==='oracle'?`CREATE USER ${qs} NO AUTHENTICATION`:`CREATE SCHEMA ${qs}`);created=true;
  if(engine==='oracle') await q(`ALTER USER ${qs} QUOTA 20M ON USERS`);
  const capture=()=>rpc({op:'schemaSnapshot',sessionId,schema});
  const empty=await capture();assert.equal(empty.objects.length,0);tests.push('empty-visible-schema');
  await q(`CREATE TABLE ${qt}(id INT PRIMARY KEY, amount DECIMAL(20,2))`);
  await q(`INSERT INTO ${qt} VALUES(1,12.34)`);
  const a=await capture();assert.equal(a.objects.length,1);assert.equal(a.objects[0].name,'Ödemeler_%');assert.equal(a.objects[0].detail.columns.length,2);assert.ok(a.objects[0].detail.primaryKeys.length);tests.push('unicode-and-literal-wildcard-schema-table');
  await q(`ALTER TABLE ${qt} ADD ${qname('açıklama')} ${engine==='oracle'?'VARCHAR2(80)':'VARCHAR(80)'}`);
  await q(`CREATE INDEX ${engine==='oracle'?qs+'.':''}${qname('ix_tutar')} ON ${qt}(amount)`);
  const b=await capture();assert.equal(a.source,b.source);assert.equal(b.objects[0].detail.columns.length,3);assert.ok(b.objects[0].detail.indexes.some(i=>i.index_name==='ix_tutar'));tests.push('column-and-index-changes-captured');
  await rpc({op:'transaction',sessionId,action:'autocommit',enabled:false});
  await q(`UPDATE ${qt} SET amount=99 WHERE id=1`);
  await capture();assert.equal((await rpc({op:'status',sessionId})).transaction,true);
  await rpc({op:'transaction',sessionId,action:'rollback'});await rpc({op:'transaction',sessionId,action:'autocommit',enabled:true});
  const data=await q(`SELECT amount FROM ${qt}`);assert.equal(Number(data.results[0].rows[0][0]),12.34);tests.push('capture-isolated-from-editor-transaction');
  await assert.rejects(()=>rpc({op:'schemaSnapshot',sessionId,schema:schema+'missing'}));tests.push('invisible-schema-rejected');
  await q(`DROP TABLE ${qt}`);assert.equal((await capture()).objects.length,0);tests.push('removed-object-no-stale-metadata');
  if(engine==='postgresql') {
   await q(`CREATE TABLE ${qs}.wide(${Array.from({length:1001},(_,i)=>`c${i} INT`).join(',')})`);
   await assert.rejects(capture,/1000/);await q(`DROP TABLE ${qs}.wide`);tests.push('metadata-overflow-fails-without-partial-snapshot');
   for(let start=0;start<201;start+=25)await q(Array.from({length:Math.min(25,201-start)},(_,i)=>`CREATE TABLE ${qs}.t${start+i}(id INT)`).join(';'));
   await assert.rejects(capture,/200/);tests.push('schema-object-limit-fails');
  }
  if(engine==='mssql') {
   const user='tt_schema_'+randomBytes(5).toString('hex'),password='Tt9!'+randomBytes(20).toString('hex');let restricted;
   try {
    await q(`CREATE TABLE ${qt}(id INT NOT NULL, note VARCHAR(20) NULL)`);
    await q(`CREATE UNIQUE INDEX ix_include ON ${qt}(id) INCLUDE(note)`);
    await q(`INSERT INTO ${qt}(id,note) VALUES(1,NULL),(2,'two')`);
    const page=await rpc({op:'browse',sessionId,schema,table:'Ödemeler_%',pageSize:100});
    assert.equal(page.stable,true);assert.equal(page.rows.length,2);tests.push('unique-index-included-column-not-a-pagination-key');
    await q(`CREATE LOGIN ${user} WITH PASSWORD='${password}'`);
    await q(`CREATE USER ${user} FOR LOGIN ${user}`);
    await q(`GRANT SELECT ON ${qt} TO ${user}`);
    restricted=(await rpc({op:'open',profile:{...profile,username:user,password}})).sessionId;
    const visible=await rpc({op:'schemaSnapshot',sessionId:restricted,schema});assert.equal(visible.objects.length,1);assert.notEqual(visible.source,empty.source);
    await q(`REVOKE SELECT ON ${qt} FROM ${user}`);
    const hidden=await rpc({op:'schemaSnapshot',sessionId:restricted,schema}).catch(e=>({error:e}));
    assert.ok(hidden.error || hidden.objects.length===0);tests.push('restricted-principal-and-revoked-metadata-visibility');
   } finally {
    if(restricted)await rpc({op:'close',sessionId:restricted}).catch(()=>{});
    await q(`DROP USER ${user}`).catch(()=>{});await q(`DROP LOGIN ${user}`).catch(()=>{});await q(`DROP TABLE ${qt}`).catch(()=>{});
   }
  }
  report.push({engine,version:opened.version,status:'passed',tests});
 }catch(e){report.push({engine,status:'failed',tests,error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally {
  if(sessionId){if(created)await rpc({op:'query',sessionId,sql:engine==='oracle'?`DROP USER ${qs} CASCADE`:engine==='postgresql'?`DROP SCHEMA ${qs} CASCADE`:`DROP SCHEMA ${qs}`}).catch(()=>{});await rpc({op:'close',sessionId}).catch(()=>{});}
 }
}}finally{close();}
await writeFile(`artifacts/database-workspace/${process.platform==='linux'?'linux-':''}schema-snapshot-tests.json`,JSON.stringify({date:new Date().toISOString(),report},null,2));
console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
