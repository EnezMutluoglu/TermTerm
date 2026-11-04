import {readFile,writeFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {randomBytes,randomUUID} from 'node:crypto';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[],catalog=JSON.parse(await readFile('src/database/catalog.json','utf8')),contracts=JSON.parse(await readFile('src/database/actionInputs.json','utf8'));
async function pgAdmin(sql){await writeFile('.lab/database/monitor-admin.sql',sql);execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f','/mnt/c/Users/sonx/Desktop/termterm/.lab/database/monitor-admin.sql'],{stdio:'pipe',windowsHide:true});}
try {for(const profile of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
 const engine=profile.engine,username='tt_monitor_'+randomBytes(4).toString('hex'),password='Tt9!'+randomBytes(20).toString('hex'),sessions=[],tests=[];
 const open=async p=>{const s=await rpc({op:'open',profile:p});sessions.push(s.sessionId);return s.sessionId;};
 const admin=await open(profile),q=(sql,sessionId=admin)=>rpc({op:'query',sessionId,sql});let created=false;
 try {
  await q('CREATE TABLE tt_monitor_probe (id INT PRIMARY KEY, value INT)');await q('INSERT INTO tt_monitor_probe VALUES (1,0)');
  if(engine==='postgresql')await pgAdmin(`CREATE ROLE ${username} LOGIN PASSWORD '${password}'; GRANT pg_monitor TO ${username}; GRANT SELECT ON tt_monitor_probe TO ${username};`);
  if(engine==='mssql'){await q(`CREATE LOGIN ${username} WITH PASSWORD='${password}';CREATE USER ${username} FOR LOGIN ${username};USE master;GRANT VIEW SERVER PERFORMANCE STATE TO ${username};USE termterm_sql_lab;GRANT VIEW DATABASE STATE TO ${username};GRANT SELECT ON dbo.tt_monitor_probe TO ${username};`);}
  if(engine==='oracle'){await q(`CREATE USER ${username} IDENTIFIED BY "${password}"`);await q(`GRANT CREATE SESSION TO ${username}`);execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','docker','exec','-i','-u','oracle','termterm-db-oracle','sqlplus','-s','/ as sysdba'],{input:`WHENEVER SQLERROR EXIT FAILURE\nALTER SESSION SET CONTAINER=FREEPDB1;\nGRANT SELECT ON SYS.GV_$SESSION TO ${username};\nexit\n`,stdio:['pipe','pipe','pipe'],windowsHide:true});await q(`GRANT SELECT ON SYSTEM.tt_monitor_probe TO ${username}`);}
  created=true;const monitor=await open({...profile,username,password});
  const target=engine==='oracle'?'SYSTEM.tt_monitor_probe':engine==='mssql'?'dbo.tt_monitor_probe':'public.tt_monitor_probe';
  assert.equal((await q(`SELECT value FROM ${target}`,monitor)).results[0].rows[0][0],'0');await assert.rejects(()=>q(`UPDATE ${target} SET value=999`,monitor));tests.push('restricted-monitor-read-allowed-write-denied');
  await rpc({op:'transaction',sessionId:admin,action:'autocommit',enabled:false});await q('UPDATE tt_monitor_probe SET value=1 WHERE id=1');
  const contender=await open(profile),operationId=randomUUID();const running=rpc({op:'query',sessionId:contender,operationId,sql:'UPDATE tt_monitor_probe SET value=2 WHERE id=1',timeout:20}).then(()=>({completed:true}),e=>({state:e.state}));
  const actionId=engine==='postgresql'?'PG-MON-007':engine==='mssql'?'MS-MON-005':'OR-MON-005';let blocked;
  for(let i=0;i<12;i++){await new Promise(r=>setTimeout(r,100));blocked=await rpc({op:'check',sessionId:monitor,action:catalog.find(a=>a.id===actionId),contract:contracts[actionId],parameters:{}});if(blocked.results.some(r=>r.rows?.length))break;}
  assert.equal(blocked.state,'succeeded');assert.ok(blocked.results.some(r=>r.rows?.length));tests.push('controlled-blocking-visible-to-restricted-monitor');
  await assert.rejects(()=>rpc({op:'cancel',sessionId:contender,operationId:'outdated-operation'}));await rpc({op:'cancel',sessionId:contender,operationId});const cancelled=await running;assert.equal(cancelled.state,'cancelled');tests.push('matching-cancel-confirmed-old-cancel-rejected');
  await rpc({op:'transaction',sessionId:admin,action:'rollback'});await rpc({op:'transaction',sessionId:admin,action:'autocommit',enabled:true});
  assert.equal((await q('SELECT value FROM tt_monitor_probe')).results[0].rows[0][0],'0');tests.push('cancelled-write-and-rollback-preserve-data');
  report.push({engine,status:'passed',tests});
 }catch(e){report.push({engine,status:'failed',tests,error:e.message.replaceAll(password,'[redacted]').replaceAll(profile.password,'[redacted]')});}
 finally {
  for(const id of sessions.slice(1))await rpc({op:'close',sessionId:id}).catch(()=>{});
  await rpc({op:'transaction',sessionId:admin,action:'rollback'}).catch(()=>{});await rpc({op:'transaction',sessionId:admin,action:'autocommit',enabled:true}).catch(()=>{});
  await q('DROP TABLE tt_monitor_probe').catch(()=>{});
  if(created){if(engine==='postgresql')await pgAdmin(`DROP OWNED BY ${username}; DROP ROLE ${username};`);if(engine==='mssql')await q(`DROP USER ${username};DROP LOGIN ${username};`);if(engine==='oracle')await q(`DROP USER ${username}`);}
  await rpc({op:'close',sessionId:admin}).catch(()=>{});
 }
}}finally{close();}
await writeFile('artifacts/database-workspace/monitoring-tests.json',JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
