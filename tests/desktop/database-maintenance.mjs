import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=process.cwd(),work=path.join(root,'.lab/database/plans-native-'+Date.now());
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const db=request=>invoke('database_request',{request});
async function select(label,value){await browser.waitUntil(()=>browser.execute((label,value)=>Array.from(document.querySelector(`select[aria-label="${label}"]`)?.options??[]).some(o=>o.value===value),label,value));await browser.execute((label,value)=>{const e=document.querySelector(`select[aria-label="${label}"]`);e.value=value;e.dispatchEvent(new Event('change',{bubbles:true}));},label,value);}
describe('Maintenance change plans native',()=>{
 let profiles;
 before(async()=>{
  await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).map(p=>({...p,id:crypto.randomUUID()}));
  await invoke('vault_lock');await invoke('vault_create',{path:path.join(work,'plans.ttvault'),name:'Bakım önizleme testi',password:'Disposable-plan-password'});
  await invoke('records_save',{records:profiles.map(p=>({id:p.id,kind:'databaseConnection',data:{...p,sshHostId:'',environment:'test'},updatedAt:Date.now()}))});
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
 });
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: preview, explicit approval, real apply and concurrent DDL`,async()=>{
  const p=profiles.find(p=>p.engine===engine),schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='Ödeme planı_'+Date.now().toString(36),quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=quote(schema)+'.'+quote(table);
  const sessionId=(await db({op:'open',recordId:p.id})).sessionId,q=sql=>db({op:'query',sessionId,sql});
  try{
   await q(`CREATE TABLE ${target}(id INT PRIMARY KEY, counter INT)`);await q(`INSERT INTO ${target} VALUES(1,42)`);if(engine==='mssql')await q(`CREATE STATISTICS [Ölçüm] ON ${target}(counter)`);
   await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`).click();await $(`button=${schema}`).waitForDisplayed({timeout:30000});
   await $('button*=Kontroller ve yönetim').click();const action=engine==='postgresql'?'PG-OPS-004':engine==='mssql'?'MS-OPS-003':'OR-OPS-003';await $('input[placeholder="Kontrol veya işlem ara…"]').setValue(action);await $('button=Değişiklik planı').click();
   await select('Bakım şeması',schema);await select('Bakım tablosu',table);if(engine==='mssql')await select('Bakım istatistiği','Ölçüm');
   await $('button=Değişiklik planını hazırla').click();await $('.db-change-preview').waitForDisplayed();assert.equal(await $('button=Onaylanan planı uygula').isEnabled(),false);
   const context=await $('.db-change-preview').getText();assert.ok(context.includes(p.host));assert.ok(context.includes(p.database));assert.ok(context.includes(p.username));assert.ok((await $('.db-maintenance-confirm input').getSize()).width>=16);
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-maintenance-preview.png`));
   await $('.db-maintenance-confirm input').click();await $('button=Onaylanan planı uygula').click();await $('.db-change-outcome').waitForDisplayed();assert.ok((await $('.db-change-outcome').getText()).includes('Tamamlandı'));
   assert.ok((await $('.db-change-outcome').getText()).includes('estimated_rows'));await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-maintenance-result.png`));
   assert.equal(await $('.db-change-preview').isExisting(),false);
   await $('button=Değişiklik planını hazırla').click();await $('.db-change-preview').waitForDisplayed();await q(`ALTER TABLE ${target} ADD new_column INT`);await $('.db-maintenance-confirm input').click();await $('button=Onaylanan planı uygula').click();await $('.db-change-outcome').waitForDisplayed();assert.ok((await $('.db-change-outcome').getText()).includes('Hedef değişti'));
   await $('.modal > header button').click();assert.equal((await q(`SELECT counter FROM ${target}`)).results[0].rows[0][0],'42');
  }finally{await q(`DROP TABLE ${target}`).catch(()=>{});await db({op:'close',sessionId}).catch(()=>{});}
 });
 it('locked vault rejects all new maintenance IPC',async()=>{await invoke('vault_lock');for(const op of ['changeOptions','changePreview','changeApply','changeDiscard'])await assert.rejects(()=>db({op,sessionId:'old'}));});
});
