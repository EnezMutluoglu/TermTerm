import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=process.cwd(),work=path.join(root,'.lab/database/schema-native-'+Date.now());
async function invoke(command,args={}) {
 const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);
 if(r.error)throw Error(r.error);return r.value;
}
const db=request=>invoke('database_request',{request});
describe('Encrypted schema history native',()=>{
 let profiles,vaultPath;const retained=[];
 before(async()=>{
  await browser.setTimeout({script:120000});
  assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});vaultPath=path.join(work,'snapshots.ttvault');
  profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).map(p=>({...p,id:crypto.randomUUID()}));
  await invoke('vault_lock');await invoke('vault_create',{path:vaultPath,name:'Şema geçmişi testi',password:'Disposable-schema-test-password'});
  await invoke('records_save',{records:profiles.map(p=>({id:p.id,kind:'databaseConnection',data:{...p,sshHostId:'',environment:'test'},updatedAt:Date.now()}))});
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
 });
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: capture, structure diff, navigation and persisted IPC`,async()=>{
  const profile=profiles.find(p=>p.engine===engine),schema='TT_SCHEMA_UI_'+Date.now().toString(36).toUpperCase(),quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',qs=quote(schema),qt=qs+'.'+quote('Ödemeler');
  const opened=await db({op:'open',recordId:profile.id}),sessionId=opened.sessionId,q=sql=>db({op:'query',sessionId,sql});let created=false;
  try{
   await q(engine==='oracle'?`CREATE USER ${qs} NO AUTHENTICATION`:`CREATE SCHEMA ${qs}`);created=true;
   if(engine==='oracle')await q(`ALTER USER ${qs} QUOTA 20M ON USERS`);
   await q(`CREATE TABLE ${qt}(id INT PRIMARY KEY, amount DECIMAL(20,2))`);
   await $(`//div[contains(@class,'db-connection')][.//strong[text()='${profile.label}']]//button[@title='Bağlan / yeni sorgu']`).click();
   await $(`button=${schema}`).waitForDisplayed({timeout:30000});await $(`button=${schema}`).click();
   await $('button=Şema geçmişi').click();
   await $('.db-schema-history button.primary').waitForEnabled();await $('.db-schema-history button.primary').click();
   await browser.waitUntil(async()=>(await db({op:'snapshotList',recordId:profile.id})).items.length===1,{timeout:40000});
   const a=(await db({op:'snapshotList',recordId:profile.id})).items[0];
   await q(`ALTER TABLE ${qt} ADD ${quote('açıklama')} ${engine==='oracle'?'VARCHAR2(80)':'VARCHAR(80)'}`);
   await q(`CREATE INDEX ${engine==='oracle'?qs+'.':''}${quote('ix_tutar')} ON ${qt}(amount)`);
   await $('.db-schema-history button.primary').waitForEnabled();await $('.db-schema-history button.primary').click();
   await browser.waitUntil(async()=>(await db({op:'snapshotList',recordId:profile.id})).items.length===2,{timeout:40000});
   const b=(await db({op:'snapshotList',recordId:profile.id})).items.find(i=>i.id!==a.id);
   await $('.db-schema-history button.primary').waitForEnabled();
   for(const [label,id] of [['Önce görüntüsü',a.id],['Sonra görüntüsü',b.id]])await browser.execute((label,id)=>{const select=document.querySelector(`select[aria-label="${label}"]`);select.value=id;select.dispatchEvent(new Event('change',{bubbles:true}));},label,id);
   await $('button=Karşılaştır').waitForEnabled();
   await $('button=Karşılaştır').click();await $('.db-schema-diff tbody tr').waitForDisplayed();
   const text=await $('.db-schema-diff').getText();assert.ok(text.includes('açıklama'));assert.ok(text.includes('ix_tutar'));assert.ok(text.includes('Yeni görünür'));
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-schema-history.png`));
   await $('button=SQL çalışma alanı').click();await $('button=Şema geçmişi').click();
   await browser.waitUntil(async()=>(await $('.db-snapshot-library summary').getText()).includes('(2)'));
   const other=profiles.find(p=>p.id!==profile.id);await assert.rejects(()=>db({op:'snapshotGet',recordId:other.id,id:a.id}));
   retained.push({recordId:profile.id,id:a.id});
  }finally{
   await q(`DROP TABLE ${qt}`).catch(()=>{});
   if(created)await q(engine==='oracle'?`DROP USER ${qs} CASCADE`:`DROP SCHEMA ${qs}`).catch(()=>{});
   await db({op:'close',sessionId}).catch(()=>{});
  }
 });
 it('survives vault lock/reopen; closed sessions cannot capture; delete is scoped',async()=>{
  const saved=retained[0];assert.ok(saved);
  await invoke('vault_lock');await assert.rejects(()=>db({op:'snapshotGet',...saved}));
  await invoke('vault_open',{path:vaultPath,password:'Disposable-schema-test-password'});
  const restored=await db({op:'snapshotGet',...saved});assert.ok(restored.objects[0].detail.columns.length);
  const opened=await db({op:'open',recordId:saved.recordId});await db({op:'close',sessionId:opened.sessionId});
  await assert.rejects(()=>db({op:'snapshotCapture',sessionId:opened.sessionId,schema:restored.schema}));
  await assert.rejects(()=>db({op:'schemaSnapshot',recordId:saved.recordId}));
  await db({op:'snapshotDelete',...saved});await assert.rejects(()=>db({op:'snapshotGet',...saved}));
  await invoke('vault_lock');
 });
});
