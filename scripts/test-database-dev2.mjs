import {readFile,writeFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper();
const profiles=JSON.parse(await readFile('.lab/database/profiles.json','utf8'));
const catalog=JSON.parse(await readFile('src/database/catalog.json','utf8'));
const contracts=JSON.parse(await readFile('src/database/actionInputs.json','utf8'));
const report=[];
try {for(const profile of profiles){
 const tests=[];let sessionId;
 try {
  const opened=await rpc({op:'open',profile});sessionId=opened.sessionId;
  assert.equal(opened.capabilities.engine,profile.engine);assert.ok(opened.capabilities.major);tests.push('capability-discovery');
  const q=sql=>rpc({op:'query',sessionId,sql});
  const engine=profile.engine,schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM';
  const table='TT_İstanbul Page',qt=engine==='mssql'?'['+table+']':'"'+table+'"';
  await q(`CREATE TABLE ${qt} (id ${engine==='oracle'?'NUMBER(20)':'DECIMAL(20,0)'} PRIMARY KEY, label ${engine==='oracle'?'NVARCHAR2(100)':'VARCHAR(100)'} NOT NULL)`);
  try {
   const source=engine==='postgresql'?"SELECT 9007199254740993+n, 'İstanbul' FROM generate_series(0,249) n":engine==='mssql'?"SELECT TOP (250) CAST(9007199254740992 AS DECIMAL(20,0))+ROW_NUMBER() OVER(ORDER BY a.object_id),N'İstanbul' FROM sys.all_objects a":"SELECT 9007199254740992+LEVEL,'İstanbul' FROM dual CONNECT BY LEVEL<=250";
   await q(`INSERT INTO ${qt} ${source}`);
   const page=after=>rpc({op:'browse',sessionId,schema,table,pageSize:100,...after&&{after}});
   const a=await page(),b=await page(a.next),c=await page(b.next);
   assert.equal(a.stable,true);assert.equal(a.rows.length,100);assert.equal(b.rows.length,100);assert.equal(c.rows.length,50);assert.equal(c.next,null);
   assert.equal(a.rows[0][0],'9007199254740993');assert.equal(new Set([...a.rows,...b.rows,...c.rows].map(r=>r[0])).size,250);tests.push('unicode-table-exact-decimal-keyset-three-pages');
   assert.ok(a.detail.primaryKeys.length);assert.ok(a.detail.indexes.length);assert.equal(a.detail.columns.length,2);tests.push('column-index-primary-key-details');
   const name=a.columns[0].name;
   const filtered=await rpc({op:'browse',sessionId,schema,table,pageSize:500,filters:[{column:name,operator:'=',value:{kind:'decimal',value:'9007199254740993'}}]});assert.equal(filtered.rows.length,1);tests.push('typed-filter-exact-number');
   await assert.rejects(()=>rpc({op:'browse',sessionId,schema,table,pageSize:100,sort:'id;DROP TABLE x'}));tests.push('identifier-injection-rejected');
   await rpc({op:'transaction',sessionId,action:'autocommit',enabled:false});
   await q(`UPDATE ${qt} SET label='changed' WHERE ${name}=9007199254740993`);
   assert.equal((await rpc({op:'status',sessionId})).transaction,true);
   if(engine==='oracle') {await q('CREATE TABLE tt_implicit_probe (id INT)');assert.equal((await rpc({op:'status',sessionId})).transaction,false);await q('DROP TABLE tt_implicit_probe');tests.push('oracle-ddl-implicit-commit-observed');}
   else {await rpc({op:'detail',sessionId,schema,table});assert.equal((await rpc({op:'status',sessionId})).transaction,true);tests.push('metadata-does-not-touch-editor-transaction');}
   await rpc({op:'transaction',sessionId,action:'rollback'});await rpc({op:'transaction',sessionId,action:'autocommit',enabled:true});
   if(engine==='postgresql'){await q('BEGIN');await assert.rejects(()=>q('SELECT 1/0'));assert.equal((await rpc({op:'status',sessionId})).transaction,true);await q('ROLLBACK');tests.push('postgres-aborted-transaction-observed');}
   if(engine!=='oracle'){const plan=await rpc({op:'plan',sessionId,sql:`SELECT * FROM ${qt}`});assert.ok(plan.rows.length);await assert.rejects(()=>rpc({op:'plan',sessionId,sql:`SELECT * FROM ${qt}; DELETE FROM ${qt}`}));tests.push('estimated-plan-and-batch-rejection');}
   await assert.rejects(()=>rpc({op:'query',sessionId,sql:'  '}));assert.equal((await rpc({op:'status',sessionId})).state,'failed');tests.push('invalid-query-state-cleared');
   const id=engine==='postgresql'?'PG-MON-026':engine==='mssql'?'MS-MON-020':'OR-MON-013';
   const parameters=engine==='postgresql'?{pg_stat_statements_view:{schema:'public',table:'pg_stat_statements'}}:engine==='mssql'?{from_time:new Date(Date.now()-86400000).toISOString(),to_time:new Date().toISOString()}:{min_executions:1,row_limit:100};
   const result=await rpc({op:'check',sessionId,action:catalog.find(a=>a.id===id),contract:contracts[id],parameters});assert.equal(result.state,'succeeded',JSON.stringify(result));tests.push('parameterized-monitoring-action');
   await assert.rejects(()=>rpc({op:'check',sessionId,action:catalog.find(a=>a.id===id),contract:contracts[id],parameters:{...parameters,unexpected:'value'}}));tests.push('unexpected-parameter-rejected');
  } finally {await q(`DROP TABLE ${qt}`).catch(()=>{});}
  report.push({engine:profile.engine,version:opened.version,status:'passed',tests});
 }catch(e){report.push({engine:profile.engine,status:'failed',tests,error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally {if(sessionId)await rpc({op:'close',sessionId}).catch(()=>{});}
}}finally {close();}
await writeFile('artifacts/database-workspace/dev2-tests.json',JSON.stringify({date:new Date().toISOString(),report},null,2));
console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
