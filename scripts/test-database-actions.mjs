import {readFile,writeFile} from 'node:fs/promises';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),catalog=JSON.parse(await readFile('src/database/catalog.json','utf8')),contracts=JSON.parse(await readFile('src/database/actionInputs.json','utf8'));
const report={date:new Date().toISOString(),actions:{}};
try{for(const profile of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
 const {sessionId,version}=await rpc({op:'open',profile});const q=sql=>rpc({op:'query',sessionId,sql});const engine=profile.engine,schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM';const table=engine==='oracle'?'TT_ACTION_PROBE':'tt_action_probe';
 await q(`CREATE TABLE ${table} (id INT PRIMARY KEY, label ${engine==='oracle'?'VARCHAR2(60)':'VARCHAR(60)'})`);
 try{await q(`INSERT INTO ${table} VALUES(1,'İstanbul')`);
 const objectId=engine==='mssql'?Number((await q("SELECT OBJECT_ID('dbo.tt_action_probe')")).results[0].rows[0][0]):1;
 if(engine==='oracle')await q('SELECT 12345 FROM dual');
 const oracleId=engine==='oracle'?(await q("SELECT sql_id FROM v$sql WHERE sql_text LIKE 'SELECT 12345%' AND ROWNUM=1")).results[0].rows[0]?.[0]:null;
 const parameters={requested_roles:[profile.username],db_name:profile.database,app_user:profile.username,reader_user:profile.username,app_schema:schema,database_name:profile.database,min_seconds:1,pid:1,parent_table:`${schema}.${table}`,pg_stat_statements_view:{schema:'public',table:'pg_stat_statements'},role_name:profile.username,schema_name:schema,publication_name:null,table_name:engine==='oracle'?table:`${schema}.${table}`,selection:engine==='oracle'?['ID','LABEL']:['id','label'],table:{schema,table},unique_key:engine==='oracle'?'ID':'id',page_size:100,expected_owner:profile.username,owner_login:profile.username,app_login:profile.username,reader_login:profile.username,migrator_login:profile.username,qualified_table:`${schema}.${table}`,from_time:new Date(Date.now()-86400000).toISOString(),to_time:new Date().toISOString(),object_id:objectId,approved_session_name:'system_health',session_id:1,owner_user:schema,rw_role:'CONNECT',ro_role:'CONNECT',row_limit:100,min_executions:1,sql_id:oracleId??'0000000000000',child_number:0,lookback_hours:1,lookback_days:1,lookback_minutes:5};
 for(const a of catalog.filter(a=>a.engine===engine)){
 const base={engine,version,risk:a.risk};if(a.risk!=='R0'){report.actions[a.id]={...base,status:'requires-change-plan'};continue;}
 const contract=contracts[a.id];if(contract.license){report.actions[a.id]={...base,status:'license-gated'};continue;}
 if(a.id.includes('-CLOUD-')||a.id==='MS-MON-036'){report.actions[a.id]={...base,status:'requires-cloud-environment'};continue;}
 if(a.id==='PG-MON-010'){report.actions[a.id]={...base,status:'requires-standby'};continue;}
 const values=Object.fromEntries(Object.entries(contract.parameters).map(([k,v])=>[k,Object.hasOwn(parameters,k)?parameters[k]:v.default]));
 try {const result=await rpc({op:'check',sessionId,action:a,contract,parameters:values});report.actions[a.id]={...base,status:result.state==='succeeded'?'passed':result.status,durationMs:result.elapsedMs,resultStatus:result.status,parameterNames:Object.keys(values),resultColumns:result.results.map(r=>(r.columns??[]).map(c=>c.name)),...result.message&&{error:result.message}};}
 catch(e){report.actions[a.id]={...base,status:'failed',error:e.message.replaceAll(profile.password,'[redacted]')};}
 }
 }finally {await q(`DROP TABLE ${table}`);await rpc({op:'close',sessionId});}
}}finally{close();}
await writeFile('artifacts/database-workspace/dev2-catalog-validation.json',JSON.stringify(report,null,2));
await writeFile('src/database/validation.json',JSON.stringify(report,null,2)+'\n');
const totals={};for(const a of Object.values(report.actions))totals[a.status]=(totals[a.status]??0)+1;console.log(totals);for(const [id,a]of Object.entries(report.actions))if(['failed','permission_denied','unsupported','unknown_outcome'].includes(a.status))console.log(id,a.status,a.error);process.exitCode=Object.values(report.actions).some(a=>a.status==='failed')?1:0;
