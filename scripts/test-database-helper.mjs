import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import {randomBytes} from 'node:crypto';
const root=path.resolve('database-helper');
const child=spawn(path.join(root,'runtime/bin/java'+(process.platform==='win32'?'.exe':'')),['-cp',[path.join(root,'out'),path.join(root,'lib/*')].join(process.platform==='win32'?';':':'),'Main'],{stdio:['pipe','pipe','pipe'],windowsHide:true});
const pending=new Map();let seq=0;
createInterface({input:child.stdout}).on('line',line=>{const r=JSON.parse(line),p=pending.get(r.id);if(p){pending.delete(r.id);clearTimeout(p.timer);r.error?p.reject(Object.assign(new Error(r.error.message),r.error)):p.resolve(r.result);}});
child.on('exit',()=>{for(const p of pending.values()){clearTimeout(p.timer);p.reject(new Error('Helper exited'));}pending.clear();});
const rpc=r=>new Promise((resolve,reject)=>{const id=String(++seq);const timer=setTimeout(()=>{pending.delete(id);reject(new Error('Test RPC timeout'));},60000);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({...r,id})+'\n');});
const credentials=Object.fromEntries((await readFile('.lab/database/credentials.env','utf8')).trim().split(/\r?\n/).map(l=>[l.slice(0,l.indexOf('=')),l.slice(l.indexOf('=')+1)]));
const pgConfig=JSON.parse(await readFile('.lab/team-windows.json','utf8'));
const profiles=[
 {label:'PostgreSQL · Laboratuvar',engine:'postgresql',host:'localhost',port:41432,database:'termterm_sql_lab',username:'termterm_sql_lab',password:credentials.PG_PASSWORD,tls:'verify-full',caPath:pgConfig.caPath},
 {label:'SQL Server · Laboratuvar',engine:'mssql',host:'127.0.0.1',port:41433,database:'termterm_sql_lab',username:'sa',password:credentials.MSSQL_SA_PASSWORD,tls:'disabled-local'},
 {label:'Oracle · Laboratuvar',engine:'oracle',host:'127.0.0.1',port:41521,database:'FREEPDB1',username:'system',password:credentials.ORACLE_PWD,tls:'disabled-local'},
];
const report=[];let failures=0;
for(const p of profiles){
 const tests=[];let sessionId;
 try{
  if(p.engine==='mssql'){const admin=await rpc({op:'open',profile:{...p,database:'master'}});try{await rpc({op:'query',sessionId:admin.sessionId,sql:"IF DB_ID(N'termterm_sql_lab') IS NULL CREATE DATABASE termterm_sql_lab"});}finally{await rpc({op:'close',sessionId:admin.sessionId});}}
  const opened=await rpc({op:'open',profile:p});sessionId=opened.sessionId;tests.push('connect');
  const query=sql=>rpc({op:'query',sessionId,sql});
  const suffix=p.engine==='oracle'?' FROM dual':'';
  const value=await query(`SELECT ${p.engine==='mssql'?"N'İstanbul'":"'İstanbul'"} AS city, CAST(9007199254740993 AS ${p.engine==='oracle'?'NUMBER(20)':'DECIMAL(20,0)'}) AS exact_value, CAST(NULL AS ${p.engine==='oracle'?'VARCHAR2(10)':'VARCHAR(10)'}) AS missing${suffix}`);
  assert.deepEqual(value.results[0].rows[0],['İstanbul','9007199254740993',null]);tests.push('unicode-null-decimal');
  const attack="İstanbul'; DROP TABLE tt_sql_probe; --";
  const bound=await rpc({op:'query',sessionId,sql:`SELECT CAST(:value AS ${p.engine==='oracle'?'NVARCHAR2(100)':p.engine==='postgresql'?'text':'NVARCHAR(100)'})${suffix}`,parameters:{value:attack}});
  assert.equal(bound.results[0].rows[0][0],attack);tests.push('bound-parameter-injection');
  assert.ok((await rpc({op:'schemas',sessionId})).columns.length);tests.push('schemas');
  const multiple=await query(`SELECT 1${suffix};${p.engine==='mssql'?'\nGO\n':''}SELECT 2${suffix};`);
  assert.equal(multiple.results.length,2);tests.push('multiple-results');
  const many=p.engine==='postgresql'?'SELECT generate_series(1,1200)':p.engine==='mssql'?'SELECT TOP (1200) a.object_id FROM sys.all_objects a CROSS JOIN sys.all_objects b':'SELECT LEVEL FROM dual CONNECT BY LEVEL<=1200';
  const capped=await query(many);assert.equal(capped.results[0].rows.length,1000);assert.equal(capped.results[0].truncated,true);tests.push('result-cap');

  await assert.rejects(()=>rpc({op:'open',profile:{...p,password:'intentionally-incorrect-password'}}));tests.push('wrong-password');
  await assert.rejects(()=>rpc({op:'open',profile:{...p,tls:'verify-full',caPath:''}}));tests.push('tls-rejects-untrusted');
  await query('CREATE TABLE tt_sql_probe (id INT PRIMARY KEY, label '+(p.engine==='oracle'?'VARCHAR2(60)':'VARCHAR(60)')+')');
  try{
    await rpc({op:'transaction',sessionId,action:'autocommit',enabled:false});
    await query("INSERT INTO tt_sql_probe VALUES (1, 'rollback')");
    await rpc({op:'transaction',sessionId,action:'rollback'});
    assert.equal((await query('SELECT COUNT(*) FROM tt_sql_probe')).results[0].rows[0][0],'0');tests.push('rollback');
    await query("INSERT INTO tt_sql_probe VALUES (2, 'commit')");await rpc({op:'transaction',sessionId,action:'commit'});
    assert.equal((await query('SELECT COUNT(*) FROM tt_sql_probe')).results[0].rows[0][0],'1');tests.push('commit');
    await rpc({op:'transaction',sessionId,action:'rollback'});await rpc({op:'transaction',sessionId,action:'autocommit',enabled:true});
    if(p.engine!=='postgresql'){
    const ro='tt_ro_'+randomBytes(4).toString('hex'),secret='Tt9!'+randomBytes(18).toString('hex');let readonly;
    try{
      if(p.engine==='postgresql'){
        await query(`CREATE ROLE ${ro} LOGIN PASSWORD '${secret}'`);
        await query(`GRANT SELECT ON tt_sql_probe TO ${ro}`);
      }else if(p.engine==='mssql'){
        await query(`CREATE LOGIN ${ro} WITH PASSWORD='${secret}'`);
        await query(`CREATE USER ${ro} FOR LOGIN ${ro}; GRANT SELECT ON dbo.tt_sql_probe TO ${ro}`);
      }else{
        await query(`CREATE USER ${ro} IDENTIFIED BY "${secret}"`);
        await query(`GRANT CREATE SESSION TO ${ro}`);await query(`GRANT SELECT ON tt_sql_probe TO ${ro}`);
      }
      readonly=await rpc({op:'open',profile:{...p,username:ro,password:secret}});
      const table=p.engine==='oracle'?'SYSTEM.tt_sql_probe':p.engine==='mssql'?'dbo.tt_sql_probe':'public.tt_sql_probe';
      assert.equal((await rpc({op:'query',sessionId:readonly.sessionId,sql:`SELECT COUNT(*) FROM ${table}`})).results[0].rows[0][0],'1');
      await assert.rejects(()=>rpc({op:'query',sessionId:readonly.sessionId,sql:`INSERT INTO ${table} VALUES (3,'not allowed')`}));
      tests.push('readonly-select-allowed-write-denied');
    }finally{
      if(readonly)await rpc({op:'close',sessionId:readonly.sessionId});
      if(p.engine==='postgresql'){await query(`DROP OWNED BY ${ro}`);await query(`DROP ROLE ${ro}`);}
      else if(p.engine==='mssql'){await query(`DROP USER IF EXISTS ${ro}`);await query(`DROP LOGIN ${ro}`);}
      else await query(`DROP USER ${ro}`);
    }
    }else{
      await query('BEGIN READ ONLY');
      await assert.rejects(()=>query("INSERT INTO tt_sql_probe VALUES (3,'not allowed')"));
      await query('ROLLBACK');tests.push('readonly-transaction-write-denied');
    }

  }finally{await query('DROP TABLE tt_sql_probe');}
  const sleep=p.engine==='postgresql'?'SELECT pg_sleep(20)':p.engine==='mssql'?"WAITFOR DELAY '00:00:20'":'BEGIN DBMS_SESSION.SLEEP(20); END;';
  const running=query(sleep).then(()=>false,()=>true);await new Promise(r=>setTimeout(r,750));await rpc({op:'cancel',sessionId});assert.ok(await running);tests.push('cancel');
  await query(`SELECT 1${suffix}`);tests.push('usable-after-cancel');
  report.push({engine:p.engine,version:opened.version,status:'passed',tests});
 }catch(e){failures++;report.push({engine:p.engine,status:'failed',tests,error:String(e.message).replaceAll(p.password,'[redacted]')});}
 finally{if(sessionId)await rpc({op:'close',sessionId}).catch(()=>{});}
}
child.stdin.end();
await mkdir('artifacts/database-workspace',{recursive:true});
await writeFile('artifacts/database-workspace/helper-tests.json',JSON.stringify({date:new Date().toISOString(),report},null,2));
await writeFile('.lab/database/profiles.json',JSON.stringify(profiles,null,2));
console.log(JSON.stringify(report,null,2));process.exitCode=failures?1:0;
