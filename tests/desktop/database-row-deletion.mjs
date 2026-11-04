import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=process.cwd(),work=path.join(root,'.lab/database/delete-native-'+Date.now());
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const db=request=>invoke('database_request',{request});
describe('Safe row deletion native',()=>{
 let profiles,vaultPath;
 before(async()=>{
  await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});vaultPath=path.join(work,'row-deletes.ttvault');
  profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).map(p=>({...p,id:crypto.randomUUID()}));
  await invoke('vault_lock');await invoke('vault_create',{path:vaultPath,name:'Satır silme testi',password:'Disposable-row-editor-password'});
  await invoke('records_save',{records:profiles.map(p=>({id:p.id,kind:'databaseConnection',data:{...p,sshHostId:'',environment:'test'},updatedAt:Date.now()}))});
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
 });
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: preview, confirmation, conflict, delete and row refresh`,async()=>{
  const p=profiles.find(p=>p.engine===engine),schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='tt_edit_ui_'+Date.now().toString(36),quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=quote(schema)+'.'+quote(table);
  const sessionId=(await db({op:'open',recordId:p.id})).sessionId,q=sql=>db({op:'query',sessionId,sql}),col=n=>engine==='oracle'?n.toUpperCase():n;let created=false;
  try{
   await q(`CREATE TABLE ${target}(id INT PRIMARY KEY,amount DECIMAL(20,4),label ${engine==='oracle'?'NVARCHAR2(100)':engine==='mssql'?'NVARCHAR(100)':'VARCHAR(100)'},note ${engine==='oracle'?'VARCHAR2(100)':'VARCHAR(100)'})`);created=true;
   await q(`INSERT INTO ${target}(id,amount,label,note) VALUES(1,10.1234,'initial',NULL)`);
   await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`).click();
   await $(`button=${schema}`).waitForDisplayed({timeout:30000});await $(`button=${schema}`).click();await $(`button=${table}`).waitForDisplayed();await $(`button=${table}`).click();
   const openDelete = async () => { await $('button=Sil…').waitForEnabled(); await $('button=Sil…').click(); await $('.db-row-delete input[type="checkbox"]').waitForEnabled({timeout:30000}); };
   await openDelete(); assert.equal(await $('button=Onayla ve 1 satırı sil').isEnabled(),false);
   assert.equal((await q(`SELECT COUNT(*) FROM ${target}`)).results[0].rows[0][0],'1');
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-row-delete-preview.png`));
   await q(`UPDATE ${target} SET note='other writer' WHERE id=1`);
   await $('.db-row-delete input[type="checkbox"]').click(); await $('button=Onayla ve 1 satırı sil').click();
   await $('.db-edit-outcome[data-state="conflict"]').waitForDisplayed(); assert.equal((await q(`SELECT COUNT(*) FROM ${target}`)).results[0].rows[0][0],'1');
   await $('button=Tabloya dön').click(); await openDelete();
   await $('.db-row-delete input[type="checkbox"]').click(); await $('button=Onayla ve 1 satırı sil').click();
   await $('.db-edit-outcome[data-state="succeeded"]').waitForDisplayed(); assert.equal((await q(`SELECT COUNT(*) FROM ${target}`)).results[0].rows[0][0],'0');
   assert.equal(await $('button=Onayla ve 1 satırı sil').isExisting(),false);
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-row-delete-result.png`));
   await $('button=Tabloya dön').click(); await browser.waitUntil(async()=>await $('.db-browser tbody tr').isExisting()===false);
   await $('.modal header button[title="Close"]').isExisting().then(async exists=>{if(exists)await $('.modal header button[title="Close"]').click();else await $('.modal header button').click();});
  }finally{if(created)await q(`DROP TABLE ${target}`).catch(()=>{});await db({op:'close',sessionId}).catch(()=>{});}
 });
 it('vault lock and closed session invalidate edit IPC',async()=>{
  const p=profiles[0],sessionId=(await db({op:'open',recordId:p.id})).sessionId;
  await db({op:'close',sessionId});await assert.rejects(()=>db({op:'rowDeletePreview',sessionId,schema:'public',table:'unknown',keys:{id:'1'}}));
  await invoke('vault_lock');await assert.rejects(()=>db({op:'rowDeleteApply',sessionId,hash:'forged',planId:'forged'}));
 });
});
