import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=process.cwd(),work=path.join(root,'.lab/database/edit-native-'+Date.now());
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const db=request=>invoke('database_request',{request});
describe('Safe row editing native',()=>{
 let profiles,vaultPath;
 before(async()=>{
  await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});vaultPath=path.join(work,'row-edits.ttvault');
  profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).map(p=>({...p,id:crypto.randomUUID()}));
  await invoke('vault_lock');await invoke('vault_create',{path:vaultPath,name:'Satır düzenleme testi',password:'Disposable-row-editor-password'});
  await invoke('records_save',{records:profiles.map(p=>({id:p.id,kind:'databaseConnection',data:{...p,sshHostId:'',environment:'test'},updatedAt:Date.now()}))});
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
 });
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: preview, save, conflict, discard and row refresh`,async()=>{
  const p=profiles.find(p=>p.engine===engine),schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='tt_edit_ui_'+Date.now().toString(36),quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=quote(schema)+'.'+quote(table);
  const sessionId=(await db({op:'open',recordId:p.id})).sessionId,q=sql=>db({op:'query',sessionId,sql}),col=n=>engine==='oracle'?n.toUpperCase():n;let created=false;
  try{
   await q(`CREATE TABLE ${target}(id INT PRIMARY KEY,amount DECIMAL(20,4),label ${engine==='oracle'?'NVARCHAR2(100)':engine==='mssql'?'NVARCHAR(100)':'VARCHAR(100)'},note ${engine==='oracle'?'VARCHAR2(100)':'VARCHAR(100)'})`);created=true;
   await q(`INSERT INTO ${target}(id,amount,label,note) VALUES(1,10.1234,'initial',NULL)`);
   await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`).click();
   await $(`button=${schema}`).waitForDisplayed({timeout:30000});await $(`button=${schema}`).click();await $(`button=${table}`).waitForDisplayed();await $(`button=${table}`).click();
   const edit=async()=>{await $('.db-browser tbody tr:first-child button').waitForEnabled();await $('.db-browser tbody tr:first-child button').click();await $(`input[aria-label="${col('amount')} değeri"]`).waitForEnabled();};
   await edit();assert.equal(await $(`input[aria-label="${col('id')} değeri"]`).isEnabled(),false);
   await $(`input[aria-label="${col('amount')} değeri"]`).setValue('120.1256');await $(`input[aria-label="${col('label')} değeri"]`).setValue('İstanbul — yeni kayıt');
   await $('button*=Değişiklikleri önizle').click();await $('button=Onayla ve kaydet').waitForEnabled();
   assert.equal((await q(`SELECT amount FROM ${target}`)).results[0].rows[0][0],'10.1234');
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-row-edit-preview.png`));
   await $('button=Onayla ve kaydet').click();await $('.db-edit-outcome.succeeded').waitForDisplayed();assert.equal((await q(`SELECT label FROM ${target}`)).results[0].rows[0][0],'İstanbul — yeni kayıt');
   await $('.db-row-editor header button').click();await edit();await $(`input[aria-label="${col('label')} değeri"]`).setValue('my stale edit');
   await $('button*=Değişiklikleri önizle').click();await $('button=Onayla ve kaydet').waitForEnabled();await q(`UPDATE ${target} SET note='other writer' WHERE id=1`);
   await $('button=Onayla ve kaydet').click();await $('.db-edit-outcome.conflict').waitForDisplayed();assert.ok((await $('.db-edit-outcome').getText()).includes('other writer'));
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-row-edit-conflict.png`));assert.equal((await q(`SELECT label FROM ${target}`)).results[0].rows[0][0],'İstanbul — yeni kayıt');
   await $('.db-row-editor header button').click();await edit();await $(`input[aria-label="${col('label')} değeri"]`).setValue('discard this');await $('button=Değişiklikleri geri al').click();assert.equal(await $(`input[aria-label="${col('label')} değeri"]`).getValue(),'İstanbul — yeni kayıt');
   await $('.db-row-editor header button').click();await $('.db-browser tbody tr:first-child button').waitForEnabled();
   await $('.modal header button[title="Close"]').isExisting().then(async exists=>{if(exists)await $('.modal header button[title="Close"]').click();else await $('.modal header button').click();});
  }finally{if(created)await q(`DROP TABLE ${target}`).catch(()=>{});await db({op:'close',sessionId}).catch(()=>{});}
 });
 it('vault lock and closed session invalidate edit IPC',async()=>{
  const p=profiles[0],sessionId=(await db({op:'open',recordId:p.id})).sessionId;
  await db({op:'close',sessionId});await assert.rejects(()=>db({op:'rowEditOpen',sessionId,schema:'public',table:'unknown',keys:{id:'1'}}));
  await invoke('vault_lock');await assert.rejects(()=>db({op:'rowEditApply',sessionId,draftId:'forged',planId:'forged'}));
 });
});
