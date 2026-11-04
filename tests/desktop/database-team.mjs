// Disposable real Team PostgreSQL plus three real JDBC engines. No production vault.
import {browser,$,$$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
const root=process.cwd(),suffix=Date.now().toString(36),password='Disposable-Team-SQL-native-1234';
const work=path.join(root,'.lab/database/team-native-'+suffix);
let profile,owner={username:'sqlowner_'+suffix},monitor={username:'sqlmonitor_'+suffix},viewer={username:'sqlviewer_'+suffix},teamId,vaultId,profiles,records,group,documents,books;
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const request=(action,body={})=>invoke('team_request',{action,body});
const db=request=>invoke('database_request',{request});
const rec=(kind,data)=>({id:crypto.randomUUID(),kind,data,updatedAt:Date.now()});
const login=(a,register=false)=>invoke('team_auth',{profile,login:a.username,email:a.username+'@example.invalid',password,register});
const open=()=>invoke('team_open_vault',{vaultId,teamId,name:'Team SQL laboratuvarı'});
async function permission(userId,recordId,permission,effect='allow'){
 const overview=await request('overview');
 return request('acl_set',{vaultId,teamId,userId,recordId,permission,effect,aclRevision:overview.teams.find(t=>t.id===teamId).aclRevision});
}
async function second(action,body={}){
 return new Promise((resolve,reject)=>{
  const child=spawn('wsl.exe',['-d','Ubuntu-24.04','-u','root','--','python3','/mnt/c/Users/sonx/Desktop/termterm/tests/team_rpc.py'],{windowsHide:true});
  let out='',err='';child.stdout.on('data',b=>out+=b);child.stderr.on('data',b=>err+=b);child.on('error',reject);child.on('close',code=>code?reject(Error(err)):resolve(JSON.parse(out)));
  child.stdin.end(JSON.stringify({login:owner.username,password,action,body}));
 });
}
describe('Team SQL permissions and native operations',()=>{
 before(async()=>{
  await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});await invoke('vault_lock');
  profile=JSON.parse(await fs.readFile(process.env.TERMTERM_TEAM_PROFILE??'.lab/dev10-team.json','utf8'));profile.database='termterm_team_e2e';
  viewer.id=(await login(viewer,true)).user.id;await invoke('team_logout');
  monitor.id=(await login(monitor,true)).user.id;await invoke('team_logout');
  owner.id=(await login(owner,true)).user.id;
  teamId=(await request('create_team',{name:'SQL yetki testi '+suffix})).id;
  vaultId=(await request('create_vault',{teamId,name:'Team SQL laboratuvarı'})).id;
  await request('member_set',{teamId,userId:monitor.id,role:'operator',active:true});
  await request('member_set',{teamId,userId:viewer.id,role:'observer',active:true});await open();
  group=rec('group',{label:'Paylaşılan veritabanları',groupId:''});
  profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8'));
  records=await Promise.all(profiles.map(async p=>rec('databaseConnection',{...p,caPath:'',caPem:p.caPath?await fs.readFile(p.caPath,'utf8'):'',sshHostId:'',groupId:group.id,environment:'test'})));
  documents=records.map(r=>rec('savedQuery',{label:r.data.engine+' · İstanbul sorgusu',connectionId:r.id,groupId:group.id,sql:r.data.engine==='oracle'?"SELECT 'İstanbul' AS label, 9007199254740993 AS n FROM dual":`SELECT ${r.data.engine==='mssql'?'N':''}'İstanbul' AS label, CAST(9007199254740993 AS DECIMAL(20,0)) AS n`}));
  books=records.map(r=>rec('databaseRunbook',{label:r.data.engine+' · Kontrol adımları',connectionId:r.id,groupId:group.id,description:'Elle yürütülen test',steps:[{title:'Unicode ve hassasiyet',sql:documents.find(d=>d.data.connectionId===r.id).data.sql,description:'Değerleri doğrula'},{title:'Bağlantı kontrolü',sql:r.data.engine==='oracle'?'SELECT 2 FROM dual':'SELECT 2',description:'İkinci adım'}]}));
  await invoke('records_save',{records:[...books,...documents,group,...records]});await request('sync');
 });
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: encrypted shared profile opens and executes real SQL`,async()=>{
  const record=records.find(r=>r.data.engine===engine);
  const {sessionId}=await db({op:'open',recordId:record.id,profile:{password:'renderer-profile-must-be-ignored'}});
  try{
   const sql=engine==='oracle'?"SELECT 'İstanbul' AS label, 9007199254740993 AS n FROM dual":`SELECT ${engine==='mssql'?'N':''}'İstanbul' AS label, CAST(9007199254740993 AS DECIMAL(20,0)) AS n`;
   const loaded=await db({op:'libraryLoad',recordId:documents.find(d=>d.data.connectionId===record.id).id});
   assert.equal(loaded.data.sql,sql);
   const result=await db({op:'query',sessionId,sql:loaded.data.sql});assert.deepEqual(result.results[0].rows[0],['İstanbul','9007199254740993']);
   const output=path.join(work,engine+'.csv');await invoke('database_file',{action:'write',path:output,content:'verified',sessionId});assert.equal(await fs.readFile(output,'utf8'),'verified');
   assert.ok((await db({op:'schemas',sessionId})).rows.length>0);
   const runbook=await db({op:'libraryLoad',recordId:books.find(d=>d.data.connectionId===record.id).id});
   const step=await db({op:'query',sessionId,sql:runbook.data.steps[1].sql});assert.equal(step.results[0].rows[0][0],'2');
  }finally{await db({op:'close',sessionId});}
 });
 it('owner exports encrypted SQL backup with current server permission',async()=>{
  const output=path.join(work,'sql.ttbackup');
  await invoke('backup_create',{path:output,password,includeProfiles:false,ids:[...documents,...books].map(r=>r.id)});
  const contents=await fs.readFile(output);
  for(const record of records)assert.equal(contents.includes(Buffer.from(record.data.password)),false);
  const preview=await invoke('backup_preview',{path:output,password});
  assert.equal(preview.vaults[0].records.filter(r=>r.kind==='databaseConnection').length,3);
  for(const kind of ['savedQuery','databaseRunbook'])assert.equal(preview.vaults[0].records.filter(r=>r.kind===kind).length,3);
  for(const doc of preview.vaults[0].records.filter(r=>['savedQuery','databaseRunbook'].includes(r.kind)))assert.ok(preview.vaults[0].records.some(r=>r.kind==='databaseConnection'&&r.id===doc.data.connectionId));
  await assert.rejects(()=>invoke('records_delete',{ids:[records[0].id]}),/kayıt/);
 });
 it('rejects malformed documents and persists edits through cache reopen',async()=>{
  const doc=documents[0];
  await assert.rejects(()=>invoke('records_save',{records:[rec('savedQuery',{...doc.data,parameters:{password:'must-not-persist'}})]}),/saklanamaz/);
  await assert.rejects(()=>invoke('records_save',{records:[rec('savedQuery',{...doc.data,connectionId:group.id})]}),/bağlantısına/);
  await assert.rejects(()=>invoke('records_save',{records:[rec('databaseRunbook',{...books[0].data,steps:[]})]}),/adım/);
  doc.data._teamRevision=(await db({op:'libraryLoad',recordId:doc.id})).data._teamRevision;
  doc.data.description='Yeniden açılışta korunan açıklama';await invoke('records_save',{records:[doc]});await request('sync');
  await invoke('team_logout');await login(owner);await open();
  assert.equal((await db({op:'libraryLoad',recordId:doc.id})).data.description,doc.data.description);
 });
 it('native library creates, reorders, loads and deletes a runbook without automatic execution',async()=>{
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
  const record=records.find(r=>r.data.engine==='postgresql');
  await $(`//div[contains(@class,'db-connection')][.//strong[text()='${record.data.label}']]//button[@title='Bağlan / yeni sorgu']`).click();
  await $('.cm-editor').waitForDisplayed({timeout:30000});await $('button=Sorgu kitaplığı').click();
  await browser.execute(()=>{const s=document.querySelector('[aria-label="SQL belge türü"]');s.value='databaseRunbook';s.dispatchEvent(new Event('change',{bubbles:true}));});
  await $('[aria-label="SQL belge adı"]').setValue('UI · İstanbul runbook');
  await $('[aria-label="Adım 1 SQL"]').setValue('SELECT 11');
  await $('button=Adım ekle').click();await $('[aria-label="Adım 2 SQL"]').setValue('SELECT 22');
  await $$('[title="Adımı yukarı taşı"]').then(items=>items[1].click());
  await $('button=Belgeyi kaydet').click();await $('[role="status"]').waitForDisplayed();await request('sync');
  const info=await invoke('vault_info');const created=info.records.find(r=>r.data.label==='UI · İstanbul runbook');assert.ok(created);
  assert.equal((await db({op:'libraryLoad',recordId:created.id})).data.steps[0].sql,'SELECT 22');
  await browser.execute(()=>{document.querySelector('.db-library > section').scrollTop=0;});
  await browser.saveScreenshot(path.join(root,'artifacts/database-workspace/team-sql-library.png'));
  await $('button=Editöre yükle · adım 1').click();await $('.db-library').waitForExist({reverse:true});
  assert.ok((await $('.cm-content').getText()).includes('SELECT 22'));
  assert.equal(await $$('.db-results tbody tr').length,0);
  await $('button=Sorgu kitaplığı').click();await $(`//div[contains(@class,'db-library')]//aside//button[contains(.,'UI · İstanbul runbook')]`).click();
  await $('button=Belgeyi sil…').waitForEnabled();await $('button=Belgeyi sil…').click();await $('button=Silmeyi onayla').click();
  await browser.waitUntil(async()=>!(await invoke('vault_info')).records.some(r=>r.id===created.id),{timeout:15000});
  await request('sync');await assert.rejects(()=>db({op:'libraryLoad',recordId:created.id}));
  await $('[aria-label="Close dialog"]').click();
 });
 it('shows the shared SQL UI and blocks non-portable profile references',async()=>{
  await assert.rejects(()=>invoke('records_save',{records:[rec('databaseConnection',{...records[0].data,caPath:'C:/private.pem'})]}),/CA/);
  await assert.rejects(()=>invoke('records_save',{records:[rec('databaseConnection',{...records[0].data,sshHostId:crypto.randomUUID()})]}),/tüneli/);
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
  const record=records.find(r=>r.data.engine==='postgresql');
  await $(`//div[contains(@class,'db-connection')][.//strong[text()='${record.data.label}']]//button[@title='Bağlan / yeni sorgu']`).click();
  await $('.cm-editor').waitForDisplayed({timeout:30000});
  await browser.saveScreenshot(path.join(root,'artifacts/database-workspace/team-sql-owner.png'));
  await permission(monitor.id,record.id,'read');await permission(monitor.id,record.id,'sql_monitor');
  await permission(viewer.id,record.id,'read');
  for(const doc of [...documents,...books].filter(d=>d.data.connectionId===record.id)){await permission(monitor.id,doc.id,'read');await permission(viewer.id,doc.id,'read');}
 });
 it('read-only observer can browse the library without opening a SQL connection',async()=>{
  await invoke('team_logout');await login(viewer);await open();
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
  assert.equal(await $('[title="Bağlan / yeni sorgu"]').isEnabled(),false);
  await $('[title="Sorgu kitaplığını görüntüle"]').click();
  await $(`//div[contains(@class,'db-library')]//aside//button[contains(.,'postgresql · İstanbul sorgusu')]`).click();
  await browser.waitUntil(async()=>(await $('[aria-label="Adım 1 SQL"]').getValue()).includes('İstanbul'));
  assert.equal(await $('button=Belgeyi kaydet').isEnabled(),false);
  assert.equal(await $('button=Editöre yükle').isEnabled(),false);
  await browser.saveScreenshot(path.join(root,'artifacts/database-workspace/team-sql-library-reader.png'));
 });
 it('monitor sees only permitted connection and cannot execute, administer or export through IPC',async()=>{
  await invoke('team_logout');await login(monitor);const info=await open();
  const visible=info.records.filter(r=>r.kind==='databaseConnection');assert.equal(visible.length,1);assert.equal(visible[0].data.password,undefined);
  const record=visible[0];
  const doc=documents.find(d=>d.data.connectionId===record.id);
  assert.equal((await db({op:'libraryLoad',recordId:doc.id})).data.sql,doc.data.sql);
  await assert.rejects(()=>invoke('records_save',{records:[{...doc,data:{...doc.data,sql:'SELECT 0'}}]}));
  await assert.rejects(()=>invoke('backup_create',{path:path.join(work,'document-denied.ttbackup'),password,includeProfiles:false,ids:[doc.id]}));
  const {sessionId}=await db({op:'open',recordId:record.id});
  await assert.rejects(()=>db({op:'query',sessionId,sql:'SELECT 1'}));
  for(const op of ['provisionInspect','rowDeletePreview','changeOptions']){
   const s=(await db({op:'open',recordId:record.id})).sessionId;
   await assert.rejects(()=>db({op,sessionId:s}));
  }
  await assert.rejects(()=>invoke('database_file',{action:'write',recordId:record.id,path:path.join(work,'denied.csv'),content:'denied'}));
  await assert.rejects(()=>invoke('backup_create',{path:path.join(work,'denied.ttbackup'),password,includeProfiles:false,ids:[record.id]}));
  await assert.rejects(()=>fs.stat(path.join(work,'denied.csv')));
  await assert.rejects(()=>request('sql_authorize',{vaultId,recordId:record.id,permission:'sql_run'}));
  for(const forbidden of records.filter(r=>r.id!==record.id))await assert.rejects(()=>db({op:'open',recordId:forbidden.id}));
  const catalog=JSON.parse(await fs.readFile('src/database/catalog.json','utf8'));
  const action=catalog.find(a=>a.engine==='postgresql' && a.risk==='R0' && a.id==='PG-001')??catalog.find(a=>a.engine==='postgresql' && a.risk==='R0');
  const checked=await db({op:'check',recordId:record.id,actionId:action.id,sql:'DROP DATABASE ignored'});assert.equal(checked.state,'succeeded');
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
  await $(`//div[contains(@class,'db-connection')]//button[@title='Bağlan / yeni sorgu']`).click();await $('.cm-editor').waitForDisplayed();
  assert.equal(await $('.db-toolbar button.primary').isEnabled(),false);
  await browser.saveScreenshot(path.join(root,'artifacts/database-workspace/team-sql-monitor.png'));
 });
 it('a second client revokes a live session and further operations fail',async()=>{
  const record=records.find(r=>r.data.engine==='postgresql');
  const {sessionId}=await db({op:'open',recordId:record.id});
  await browser.executeAsync(done=>{
   window.__sqlClosed=[];
   window.__TAURI__.event.listen('database-session-closed',({payload})=>window.__sqlClosed.push(payload.sessionId)).then(()=>done());
  });
  const overview=await second('overview');
  await second('acl_set',{vaultId,teamId,userId:monitor.id,recordId:record.id,permission:'sql_monitor',effect:'deny',aclRevision:overview.teams.find(t=>t.id===teamId).aclRevision});
  await browser.waitUntil(()=>browser.execute(id=>window.__sqlClosed.includes(id),sessionId),{timeout:25000,timeoutMsg:'Idle SQL lease did not close revoked session'});
  await assert.rejects(()=>db({op:'schemas',sessionId}));
  await assert.rejects(()=>db({op:'open',recordId:record.id}));
 });
 after(async()=>{await invoke('team_logout').catch(()=>{});});
});
