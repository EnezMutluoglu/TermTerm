import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes} from 'node:crypto';
import assert from 'node:assert/strict';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper({timeoutMs:180000}),report=[];
async function pgAdmin(sql){const file=path.resolve('.lab/database/provision-admin.sql');await writeFile(file,sql);if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',file],{stdio:'pipe'});else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f','/mnt/c/Users/sonx/Desktop/termterm/.lab/database/provision-admin.sql'],{stdio:'pipe',windowsHide:true});}
const profiles=JSON.parse(await readFile('.lab/database/profiles.json','utf8'));
const cases=profiles.map(p=>({profile:p,mode:'schema'}));if(process.env.TEST_NEW_PDB==='1')cases.push({profile:{...profiles.find(p=>p.engine==='oracle'),database:'FREE',username:'SYS',oracleRole:'sysdba'},mode:'pdb'});
try{for(const {profile:original,mode} of cases){
 const profile={...original},engine=profile.engine,base='tt_prov_'+randomBytes(5).toString('hex'),pgUser=base+'_admin',pgPass='Tt9!'+randomBytes(20).toString('hex'),tests=[];if(process.platform==='linux'&&profile.caPath)profile.caPath=path.resolve('.lab/ca.crt');let id,owner,app,reader,result,adminCreated=false;
 const n=x=>engine==='oracle'?x.toUpperCase():x,qn=x=>engine==='mssql'?'['+x+']':'"'+n(x)+'"';
 const q=(sql,sessionId=id)=>rpc({op:'query',sessionId,sql}),first=r=>r.results[0].rows[0][0];
 const args={op:'provisionPreview',name:base,locale:engine==='postgresql'?(process.env.TEST_LOCALE||'C.UTF-8'):'Turkish_CI_AS',oracleMode:mode,tablespace:'USERS',quotaMb:256,pdbMaxMb:4096};
 const apply=p=>rpc({op:'provisionApply',sessionId:id,planId:p.planId,hash:p.hash});
 try{
  if(engine==='postgresql'){await pgAdmin(`CREATE ROLE ${pgUser} LOGIN CREATEDB CREATEROLE PASSWORD '${pgPass}';`);adminCreated=true;profile.username=pgUser;profile.password=pgPass;}
  const opened=await rpc({op:'open',profile});id=opened.sessionId;
  if(engine==='oracle'&&mode==='pdb'){const env=await q("SELECT SYS_CONTEXT('USERENV','CON_NAME'),value FROM v$parameter WHERE name='db_create_file_dest'");tests.push('root-omf:'+JSON.stringify(env.results[0].rows));}
  await assert.rejects(()=>rpc({...args,sessionId:id,name:'bad;drop'}));await assert.rejects(()=>rpc({...args,sessionId:id,sql:'SELECT 1'}));tests.push('invalid-name-and-extra-sql-rejected');
  const inspect=()=>rpc({...args,op:'provisionInspect',sessionId:id});
  if(mode==='schema'){
   const empty=await inspect();assert.equal(empty.state,'inspection');assert.ok(empty.sections.some(s=>s.state==='missing'));assert.equal('credentials' in empty,false);tests.push('missing-target-inspection-does-not-create');
   await q(engine==='postgresql'?`CREATE ROLE ${qn(base)} NOLOGIN`:engine==='mssql'?`CREATE LOGIN ${qn(base+'_owner')} WITH PASSWORD='${pgPass}'`:`CREATE ROLE ${qn(base)}`);
   const partial=await inspect();assert.ok(partial.sections.some(s=>s.state==='observed'&&JSON.stringify(s.rows).includes(n(base))));assert.ok(!JSON.stringify(partial).includes(pgPass));await assert.rejects(()=>rpc({...args,sessionId:id}));
   await q(engine==='postgresql'||engine==='oracle'?`DROP ROLE ${qn(base)}`:`DROP LOGIN ${qn(base+'_owner')}`);tests.push('partial-target-evidence-no-replay-or-secret');
  }
  let p=await rpc({...args,sessionId:id});assert.ok(JSON.stringify(p).includes('<OWNER_PASSWORD>'));assert.equal('credentials' in p,false);tests.push('secret-free-preview');
  await assert.rejects(()=>apply({...p,hash:'tampered'}));tests.push('hash-tampering-rejected');
  p=await rpc({...args,sessionId:id});await rpc({op:'provisionDiscard',sessionId:id,planId:p.planId});await assert.rejects(()=>apply(p));tests.push('discarded-plan-rejected');
  p=await rpc({...args,sessionId:id});result=await apply(p);assert.equal(result.state,'succeeded',result.message+' completed '+result.completedSteps+'/'+result.totalSteps);tests.push('fresh-target-created-and-real-logins-verified');
  const observed=await inspect();assert.equal(observed.state,'inspection');assert.ok(!observed.sections.some(s=>s.state==='unknown'),JSON.stringify(observed.sections));assert.ok(observed.sections.some(s=>s.state==='observed'));tests.push('complete-target-readonly-inspection');
  for(const c of result.credentials){assert.equal(c.password.length,21);for(const regex of [/[A-Z]/,/[a-z]/,/[0-9]/,/[!#%+_=-]/])assert.match(c.password,regex);}assert.equal(new Set(result.credentials.map(c=>c.password)).size,result.credentials.length);tests.push('independent-21-character-mixed-passwords');
  await assert.rejects(()=>apply(p));await assert.rejects(()=>rpc({...args,sessionId:id}));tests.push('credentials-not-replayed-existing-target-rejected');
  const creds=suffix=>result.credentials.find(c=>c.username===n(base+suffix));
  const targetProfile={...profile,database:result.database};
  owner=(await rpc({op:'open',profile:{...targetProfile,oracleRole:'normal',...creds('_owner')}})).sessionId;
  app=(await rpc({op:'open',profile:{...targetProfile,oracleRole:'normal',...creds('_appuser')}})).sessionId;
  const schema=engine==='oracle'?qn(base+'_owner'):engine==='mssql'?'dbo':'public',table=schema+'.tt_future';
  await q(`CREATE TABLE ${table}(id INT PRIMARY KEY, value INT)`,owner);
  await q(`INSERT INTO ${table} VALUES(1,42)`,app);assert.equal(first(await q(`SELECT value FROM ${table}`,app)),'42');await q(`UPDATE ${table} SET value=43 WHERE id=1`,app);tests.push('future-owner-table-app-crud');
  await assert.rejects(()=>q(`DROP TABLE ${table}`,app));await assert.rejects(()=>q(`CREATE TABLE ${schema}.tt_denied(id INT)`,app));tests.push('app-cannot-drop-or-create-owner-table');
  // Temporary read-only login is test-only, not part of the product template.
  const ro=n(base+'_test_ro'),roPass='Tt9!'+randomBytes(20).toString('hex');
  if(engine==='postgresql'){await q(`CREATE ROLE ${qn(ro)} LOGIN PASSWORD '${roPass}' IN ROLE ${qn(base+'_readonly')}`);}
  else if(engine==='mssql'){await q(`CREATE LOGIN ${qn(ro)} WITH PASSWORD='${roPass}'`);const target=(await rpc({op:'open',profile:targetProfile})).sessionId;try{await q(`CREATE USER ${qn(ro)} FOR LOGIN ${qn(ro)}`,target);await q(`ALTER ROLE ${qn(base+'_readonly')} ADD MEMBER ${qn(ro)}`,target);}finally{await rpc({op:'close',sessionId:target});}}
  else{if(mode==='pdb')await q(`ALTER SESSION SET CONTAINER=${qn(base)}`);await q(`CREATE USER ${qn(ro)} IDENTIFIED BY "${roPass}"`);await q(`GRANT ${qn(base+'_readonly')} TO ${qn(ro)}`);}
  reader=(await rpc({op:'open',profile:{...targetProfile,oracleRole:'normal',username:ro,password:roPass}})).sessionId;
  assert.equal(first(await q(`SELECT value FROM ${table}`,reader)),'43');await assert.rejects(()=>q(`INSERT INTO ${table} VALUES(2,5)`,reader));await assert.rejects(()=>q(`UPDATE ${table} SET value=0`,reader));await assert.rejects(()=>q(`DELETE FROM ${table}`,reader));tests.push('real-readonly-login-select-allowed-writes-denied');
  if(engine==='postgresql'){
   await q('CREATE SEQUENCE public.tt_future_seq',owner);await q("SELECT nextval('public.tt_future_seq')",app);await q('SELECT last_value FROM public.tt_future_seq',reader);await assert.rejects(()=>q("SELECT nextval('public.tt_future_seq')",reader));
   await q('CREATE FUNCTION public.tt_future_fn() RETURNS int LANGUAGE SQL AS $$SELECT 42$$',owner);assert.equal(first(await q('SELECT public.tt_future_fn()',app)),'42');await assert.rejects(()=>q('SELECT public.tt_future_fn()',reader));
   await q('CREATE SCHEMA future_schema',owner);await q('CREATE TABLE future_schema.tt_later(id int)',owner);await q('INSERT INTO future_schema.tt_later VALUES(1)',app);await q('SELECT * FROM future_schema.tt_later',reader);tests.push('future-schema-function-and-sequence-defaults-and-public-denial');
   const acl=await q(`SELECT rolname,rolsuper::text,rolcreatedb::text,rolcreaterole::text,rolbypassrls::text FROM pg_roles WHERE rolname IN ('${base}_owner','${base}_appuser')`);assert.ok(acl.results[0].rows.every(r=>r.slice(1).every(v=>v==='false')));tests.push('owner-and-app-have-no-server-admin-flags');
  }
  if(engine==='oracle'){await assert.rejects(()=>q(`SELECT * FROM ${table} FOR UPDATE`,reader));assert.equal(first(await q("SELECT count(*) FROM session_privs WHERE privilege IN ('SELECT ANY TABLE','CREATE USER','GRANT ANY ROLE')",app)),'0');tests.push('schema-scoped-not-global-admin-readonly-no-for-update');}
  await assert.rejects(()=>rpc({...args,sessionId:app,name:base+'_denied'}));tests.push('app-cannot-provision-another-database');
  await q(`DELETE FROM ${table}`,app);tests.push('app-delete-allowed');
  await rpc({op:'close',sessionId:reader});reader=null;
  if(engine==='postgresql')await q(`DROP ROLE ${qn(ro)}`);else if(engine==='oracle')await q(`DROP USER ${qn(ro)}`);
  report.push({engine,mode,version:opened.version,status:'passed',tests});
 }catch(e){report.push({engine,mode,status:'failed',tests,error:e.message});}
 finally{
  for(const sessionId of [owner,app,reader])if(sessionId)await rpc({op:'close',sessionId}).catch(()=>{});
  if(id){
   // Only random, task-owned fixture names are ever removed.
   if(engine==='postgresql'){await pgAdmin(`DROP DATABASE IF EXISTS ${base} WITH (FORCE);`);for(const suffix of ['_test_ro','_appuser','_readonly','_developer','_app','_owner',''])await pgAdmin(`DROP ROLE IF EXISTS ${base+suffix};`);}
   else if(engine==='mssql'){await q(`IF DB_ID('${base}') IS NOT NULL BEGIN ALTER DATABASE ${qn(base)} SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE ${qn(base)}; END`);for(const suffix of ['_test_ro','_appuser','_owner'])await q(`IF SUSER_ID('${base+suffix}') IS NOT NULL DROP LOGIN ${qn(base+suffix)}`);}
   else if(mode==='pdb'){await q('ALTER SESSION SET CONTAINER="CDB$ROOT"').catch(()=>{});await q(`ALTER PLUGGABLE DATABASE ${qn(base)} CLOSE IMMEDIATE`).catch(()=>{});await q(`DROP PLUGGABLE DATABASE ${qn(base)} INCLUDING DATAFILES`).catch(()=>{});}
   else{for(const suffix of ['_test_ro','_appuser','_owner'])await q(`DROP USER ${qn(base+suffix)} CASCADE`).catch(()=>{});for(const suffix of ['_readonly','_developer','_app',''])await q(`DROP ROLE ${qn(base+suffix)}`).catch(()=>{});}
   await rpc({op:'close',sessionId:id}).catch(()=>{});
  }
  if(adminCreated)await pgAdmin(`DROP ROLE ${pgUser};`);
 }
}}finally{close();}
await writeFile(`artifacts/database-workspace/${process.platform==='linux'?'linux-':''}provision-tests.json`,JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
