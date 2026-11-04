import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=process.cwd(),work=path.join(root,'.lab/database/evidence-native-'+Date.now());
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const db=request=>invoke('database_request',{request});
async function select(label,value){await browser.waitUntil(()=>browser.execute((label,value)=>Array.from(document.querySelector(`select[aria-label="${label}"]`)?.options??[]).some(o=>o.value===value),label,value));await browser.execute((label,value)=>{const e=document.querySelector(`select[aria-label="${label}"]`);e.value=value;e.dispatchEvent(new Event('change',{bubbles:true}));},label,value);}
describe('Metadata pickers and evidence native',()=>{
 let profiles;
 before(async()=>{
  await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).map(p=>({...p,id:crypto.randomUUID()}));
  await invoke('vault_lock');await invoke('vault_create',{path:path.join(work,'evidence.ttvault'),name:'İndeks ve yedek testi',password:'Disposable-evidence-password'});
  await invoke('records_save',{records:profiles.map(p=>({id:p.id,kind:'databaseConnection',data:{...p,sshHostId:'',environment:'test'},updatedAt:Date.now()}))});
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
 });
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: dependent pickers, live query, index and backup evidence`,async()=>{
  const p=profiles.find(p=>p.engine===engine),schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='Ödemeler_%_'+Date.now().toString(36),quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=quote(schema)+'.'+quote(table);
  const sessionId=(await db({op:'open',recordId:p.id})).sessionId,q=sql=>db({op:'query',sessionId,sql});
  try{
   await q(`CREATE TABLE ${target} (${quote('id')} INT PRIMARY KEY,${quote('Tutar, net')} DECIMAL(12,2))`);await q(`INSERT INTO ${target} VALUES(1,1234.56)`);
   await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`).click();await $(`button=${schema}`).waitForDisplayed({timeout:30000});
   await $('button*=Kontroller ve yönetim').click();const action=engine==='postgresql'?'PG-MON-043':engine==='mssql'?'MS-MON-035':'OR-MON-039';await $('input[placeholder="Kontrol veya işlem ara…"]').setValue(action);await $('button=Çalıştır').waitForEnabled();await $('button=Çalıştır').click();
   await select('table şema',schema);await select('table tablo',table);await $('input[aria-label="selection Tutar, net"]').waitForDisplayed();await $('input[aria-label="selection Tutar, net"]').click();await $('input[aria-label="selection id"]').click();if(engine!=='oracle')await select('unique_key','id');
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-metadata-picker.png`));
   await $('button=Parametrelerle çalıştır').click();await browser.waitUntil(async()=>(await $('.db-check-result').getText()).includes('1234.56'));
   await select('table şema',engine==='postgresql'?'pg_catalog':engine==='mssql'?'sys':'SYS');assert.equal(await $('select[aria-label="table tablo"]').getValue(),'');assert.equal((await $('.db-check-result').getText()).includes('1234.56'),false);
   await $('.modal > header button').click();await $('button=İndeks ve yedek').click();await select('İndeks şeması',schema);await select('İndeks tablosu',table);await $('button=Kanıtları getir').click();await $('[data-source="indexes"] table').waitForDisplayed();assert.ok((await $('[data-source="indexes"]').getText()).includes('Veri alındı'));
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-index-evidence.png`));
   await $('button=Yedek güncelliği').click();await $('button=Kanıtları getir').click();await $('[data-source="restores"]').waitForDisplayed();assert.ok((await $('.db-evidence').getText()).includes('geri yüklenebilirliğini kanıtlamaz'));
   if(engine==='postgresql')assert.ok((await $('[data-source="backups"]').getText()).includes('Kaynak bağlı değil'));
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-backup-evidence.png`));
  }finally{await q(`DROP TABLE ${target}`).catch(()=>{});await db({op:'close',sessionId}).catch(()=>{});}
 });
 it('locked vault rejects metadata and evidence IPC',async()=>{await invoke('vault_lock');await assert.rejects(()=>db({op:'metadataOptions',sessionId:'old',kind:'schemas'}));await assert.rejects(()=>db({op:'evidence',sessionId:'old',panel:'backups'}));});
});
