import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes} from 'node:crypto';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
try{for(const profile of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
 const engine=profile.engine,table='tt_edit_cut_'+randomBytes(5).toString('hex'),schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=quote(schema)+'.'+quote(table),trigger=table+'_tr',fn=table+'_fn';let sessionId;
 try{
  sessionId=(await rpc({op:'open',profile})).sessionId;const q=sql=>rpc({op:'query',sessionId,sql});
  await q(`CREATE TABLE ${target}(id INT PRIMARY KEY,value INT)`);await q(`INSERT INTO ${target} VALUES(1,10)`);
  if(engine==='postgresql'){await q(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(15); RETURN NEW; END $$`);await q(`CREATE TRIGGER ${trigger} BEFORE UPDATE ON ${target} FOR EACH ROW EXECUTE FUNCTION ${fn}()`);}
  else if(engine==='mssql')await q(`CREATE TRIGGER ${trigger} ON ${target} AFTER UPDATE AS BEGIN WAITFOR DELAY '00:00:15'; END`);
  else await q(`CREATE OR REPLACE TRIGGER ${trigger} BEFORE UPDATE ON ${target} FOR EACH ROW BEGIN DBMS_SESSION.SLEEP(15); END;`);
  const key=engine==='oracle'?'ID':'id',field=engine==='oracle'?'VALUE':'value';
  const d=await rpc({op:'rowEditOpen',sessionId,schema,table,keys:{[key]:'1'}}),p=await rpc({op:'rowEditPreview',sessionId,draftId:d.draftId,changes:{[field]:'20'}});
  const running=rpc({op:'rowEditApply',sessionId,draftId:d.draftId,planId:p.planId});
  let victim;
  for(let i=0;i<40;i++){
   const sql=engine==='postgresql'?`SELECT pid FROM pg_stat_activity WHERE pid<>pg_backend_pid() AND state='active' AND query LIKE '%${table}%' AND wait_event='PgSleep'`:engine==='mssql'?`SELECT r.session_id FROM sys.dm_exec_requests r CROSS APPLY sys.dm_exec_sql_text(r.sql_handle) t WHERE r.session_id<>@@SPID AND CHARINDEX('${table}',t.text)>0 AND r.wait_type='WAITFOR'`:`SELECT s.sid,s.serial# FROM v$session s JOIN v$sql q ON q.sql_id=s.sql_id AND q.child_number=s.sql_child_number WHERE s.sid<>SYS_CONTEXT('USERENV','SID') AND s.event='PL/SQL lock timer' AND INSTR(q.sql_text,'${table}')>0`;
   const rows=(await q(sql)).results[0].rows;if(rows.length===1){victim=rows[0];break;}await new Promise(r=>setTimeout(r,100));
  }
  if(!victim){await running;throw Error('Test could not identify the owned write connection');}
  const [id,serial]=victim;assert.match(id,/^\d+$/);if(serial)assert.match(serial,/^\d+$/);
  await q(engine==='postgresql'?`SELECT pg_terminate_backend(${id})`:engine==='mssql'?`KILL ${id}`:`ALTER SYSTEM KILL SESSION '${id},${serial}' IMMEDIATE`).catch(e=>{if(e.vendorCode!==31)throw e;});
  const outcome=await running;assert.equal(outcome.state,'unknown_outcome',JSON.stringify(outcome));
  assert.equal((await q(`SELECT value FROM ${target}`)).results[0].rows[0][0],'10');
  await assert.rejects(()=>rpc({op:'rowEditApply',sessionId,draftId:d.draftId,planId:p.planId}));
  report.push({engine,status:'passed',tests:['write-connection-loss-is-unknown','server-rollback-confirmed-by-new-read','spent-plan-not-replayed']});
 }catch(e){report.push({engine,status:'failed',error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally{if(sessionId){await rpc({op:'query',sessionId,sql:`DROP TABLE ${target}`}).catch(()=>{});if(engine==='postgresql')await rpc({op:'query',sessionId,sql:`DROP FUNCTION ${fn}()`}).catch(()=>{});await rpc({op:'close',sessionId}).catch(()=>{});}}
}}finally{close();}
await writeFile('artifacts/database-workspace/row-edit-disconnect-tests.json',JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
