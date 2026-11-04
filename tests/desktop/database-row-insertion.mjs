import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=process.cwd(),work=path.join(root,'.lab/database/insert-native-'+Date.now());
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const selectMode=(name,value)=>browser.execute((name,value)=>{const e=document.querySelector(`.db-row-insert select[aria-label="${name} seçimi"]`);e.value=value;e.dispatchEvent(new Event('change',{bubbles:true}));},name,value);
const db=request=>invoke('database_request',{request});
describe('Single row insertion native',()=>{
 let profiles;
 before(async()=>{
  await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});
  profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).map(p=>({...p,id:crypto.randomUUID()}));
  await invoke('vault_lock');await invoke('vault_create',{path:path.join(work,'inserts.ttvault'),name:'Satır ekleme testi',password:'Disposable-row-insert-password'});
  await invoke('records_save',{records:profiles.map(p=>({id:p.id,kind:'databaseConnection',data:{...p,sshHostId:'',environment:'test'},updatedAt:Date.now()}))});
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
 });
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: default, NULL, bound value, confirmation and refreshed result`,async()=>{
  const p=profiles.find(p=>p.engine===engine),schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='tt_insert_ui_'+Date.now().toString(36),quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=quote(schema)+'.'+quote(table),col=n=>engine==='oracle'?n.toUpperCase():n;
  const sessionId=(await db({op:'open',recordId:p.id})).sessionId,q=sql=>db({op:'query',sessionId,sql});let created=false;
  try{
   const identity=engine==='mssql'?'BIGINT IDENTITY(1,1)':engine==='oracle'?'NUMBER GENERATED ALWAYS AS IDENTITY':'BIGINT GENERATED ALWAYS AS IDENTITY';
   await q(`CREATE TABLE ${target}(id ${identity} PRIMARY KEY, label ${engine==='oracle'?'NVARCHAR2(100)':engine==='mssql'?'NVARCHAR(100)':'VARCHAR(100)'} DEFAULT 'default value' NOT NULL, amount DECIMAL(28,4), note ${engine==='oracle'?'VARCHAR2(100)':'VARCHAR(100)'})`);created=true;
   await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`).click();
   await $(`button=${schema}`).waitForDisplayed({timeout:30000});await $(`button=${schema}`).click();await $(`button=${table}`).waitForDisplayed();await $(`button=${table}`).click();
   await $('button=Yeni satır…').waitForEnabled();await $('button=Yeni satır…').click();
   const choice=n=>$(`.db-row-insert select[aria-label="${col(n)} seçimi"]`),input=n=>$(`.db-row-insert input[aria-label="${col(n)} yeni değer"]`);
   await choice('label').waitForEnabled();assert.equal(await choice('id').isEnabled(),false);
   await selectMode(col('label'),'value');await input('label').waitForDisplayed();await input('label').setValue('İstanbul deneme');
   await selectMode(col('amount'),'value');await input('amount').waitForDisplayed();await input('amount').setValue('9007199254740993.1234');
   await selectMode(col('note'),'null');await $('button=Eklenecek satırı önizle').click();
   await $('.db-row-insert input[type="checkbox"]').waitForEnabled();assert.equal(await $('button=Onayla ve 1 satır ekle').isEnabled(),false);
   assert.equal((await q(`SELECT COUNT(*) FROM ${target}`)).results[0].rows[0][0],'0');
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-row-insert-preview.png`));
   await $('.db-row-insert input[type="checkbox"]').click();await $('button=Onayla ve 1 satır ekle').click();
   await $('.db-edit-outcome[data-state="succeeded"]').waitForDisplayed();const row=(await q(`SELECT label,amount,note FROM ${target}`)).results[0].rows[0];assert.deepEqual(row,['İstanbul deneme','9007199254740993.1234',null]);
   assert.equal(await $('button=Onayla ve 1 satır ekle').isExisting(),false);await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-row-insert-result.png`));
   await $('button=Tabloya dön').click();await browser.waitUntil(async()=>await $('.db-browser tbody tr').isExisting());await $('.modal header button').click();
  }finally{if(await $('.modal header button').isExisting())await $('.modal header button').click();if(created)await q(`DROP TABLE ${target}`).catch(()=>{});await db({op:'close',sessionId}).catch(()=>{});}
 });
 after(async()=>{await invoke('vault_lock');});
});
