import {readFile,writeFile} from 'node:fs/promises';
import {helper} from './database-rpc.mjs';
import assert from 'node:assert/strict';
const {rpc,close}=helper(),report=[];
try {for(const profile of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
 const admin=await rpc({op:'open',profile}),victim=await rpc({op:'open',profile}),q=sql=>rpc({op:'query',sessionId:admin.sessionId,sql});
 try {await q('CREATE TABLE tt_disconnect_probe (id INT PRIMARY KEY)');
  const identity=await rpc({op:'query',sessionId:victim.sessionId,sql:profile.engine==='postgresql'?'SELECT pg_backend_pid()':profile.engine==='mssql'?'SELECT @@SPID':"SELECT sid,serial# FROM v$session WHERE sid=SYS_CONTEXT('USERENV','SID')"});
  const [id,serial]=identity.results[0].rows[0];assert.match(id,/^\d+$/);if(serial)assert.match(serial,/^\d+$/);
  const sleep=profile.engine==='postgresql'?'SELECT pg_sleep(20)':profile.engine==='mssql'?"WAITFOR DELAY '00:00:20'":'BEGIN DBMS_SESSION.SLEEP(20); END;';
  const running=rpc({op:'query',sessionId:victim.sessionId,sql:`INSERT INTO tt_disconnect_probe VALUES(1);\n${sleep}`,timeout:25}).then(r=>r,e=>({state:e.state,sqlState:e.sqlState,vendorCode:e.vendorCode}));
  // Wait for the committed first statement, not for an arbitrary long soak.
  for(let i=0;i<30;i++){if((await q('SELECT COUNT(*) FROM tt_disconnect_probe')).results[0].rows[0][0]==='1')break;await new Promise(r=>setTimeout(r,50));}
  await q(profile.engine==='postgresql'?`SELECT pg_terminate_backend(${id})`:profile.engine==='mssql'?`KILL ${id}`:`ALTER SYSTEM KILL SESSION '${id},${serial}' IMMEDIATE`).catch(e=>{if(e.vendorCode!==31)throw e;});
  const outcome=await running;assert.equal(outcome.state,'unknown_outcome',JSON.stringify(outcome));
  assert.equal((await q('SELECT COUNT(*) FROM tt_disconnect_probe')).results[0].rows[0][0],'1');
  report.push({engine:profile.engine,status:'passed',tests:['connection-loss-after-committed-write-is-unknown','write-not-replayed']});
 }catch(e){report.push({engine:profile.engine,status:'failed',error:e.message.replaceAll(profile.password,'[redacted]')});}
 finally {await q('DROP TABLE tt_disconnect_probe').catch(()=>{});await rpc({op:'close',sessionId:victim.sessionId}).catch(()=>{});await rpc({op:'close',sessionId:admin.sessionId});}
}}finally{close();}
await writeFile('artifacts/database-workspace/disconnect-tests.json',JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
