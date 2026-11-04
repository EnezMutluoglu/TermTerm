import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes} from 'node:crypto';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
try{for(const profile of JSON.parse(await readFile('.lab/database/profiles.json','utf8')).filter(p=>p.engine==='postgresql')){
 const engine=profile.engine,schema=engine==='postgresql'?'public':'dbo',table='tt_plan_cut_'+randomBytes(5).toString('hex'),qn=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=qn(schema)+'.'+qn(table);let id,holder;
 const q=sql=>rpc({op:'query',sessionId:id,sql});
 try{
  id=(await rpc({op:'open',profile})).sessionId;holder=(await rpc({op:'open',profile})).sessionId;
  await q(`CREATE TABLE ${target}(id INT PRIMARY KEY,v INT)`);await q(`INSERT INTO ${target} VALUES(1,42)`);if(engine==='mssql')await q(`CREATE STATISTICS [sample_stat] ON ${target}(v)`);
  const p=await rpc({op:'changePreview',sessionId:id,actionId:engine==='postgresql'?'PG-OPS-004':'MS-OPS-003',schema,table,statistic:engine==='mssql'?'sample_stat':''});
  await rpc({op:'transaction',sessionId:holder,action:'autocommit',enabled:false});await rpc({op:'query',sessionId:holder,sql:engine==='postgresql'?`LOCK TABLE ${target} IN SHARE UPDATE EXCLUSIVE MODE`:`SELECT * FROM ${target} WITH (TABLOCKX,HOLDLOCK)`});
  const running=rpc({op:'changeApply',sessionId:id,planId:p.planId,hash:p.hash});let victim;
  for(let i=0;i<30;i++){
   const sql=engine==='postgresql'?`SELECT pid FROM pg_stat_activity WHERE pid<>pg_backend_pid() AND state='active' AND query LIKE 'ANALYZE VERBOSE %${table}%' AND wait_event_type='Lock'`:`SELECT r.session_id FROM sys.dm_exec_requests r CROSS APPLY sys.dm_exec_sql_text(r.sql_handle) t WHERE r.session_id<>@@SPID AND t.text LIKE '%UPDATE STATISTICS %${table}%' AND r.wait_type LIKE 'LCK%'`;
   const rows=(await q(sql)).results[0].rows;if(rows.length===1){victim=rows[0][0];break;}await new Promise(r=>setTimeout(r,50));
  }
  if(!victim){const result=await running;throw Error('Owned maintenance connection not observed waiting: '+JSON.stringify(result));}assert.match(victim,/^\d+$/);
  await q(engine==='postgresql'?`SELECT pg_terminate_backend(${victim})`:`KILL ${victim}`);
  assert.equal((await running).state,'unknown_outcome');assert.equal((await rpc({op:'changeApply',sessionId:id,planId:p.planId,hash:p.hash})).state,'failed');
  await rpc({op:'transaction',sessionId:holder,action:'rollback'});assert.equal((await q(`SELECT v FROM ${target}`)).results[0].rows[0][0],'42');
  report.push({engine,status:'passed',tests:['lost-maintenance-connection-unknown','spent-plan-not-replayed','table-data-unchanged']});
 }catch(e){report.push({engine,status:'failed',error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally{if(holder){await rpc({op:'transaction',sessionId:holder,action:'rollback'}).catch(()=>{});await rpc({op:'close',sessionId:holder}).catch(()=>{});}if(id){await q(`DROP TABLE ${target}`).catch(()=>{});await rpc({op:'close',sessionId:id}).catch(()=>{});}}
}}finally{close();}
await writeFile('artifacts/database-workspace/change-plan-disconnect-tests.json',JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
