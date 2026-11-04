import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const root=process.cwd(),work=path.join(root,'.lab/database/values-native-'+Date.now());
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const db=request=>invoke('database_request',{request});
describe('Full values and session evidence native',()=>{
 let profiles;
 before(async()=>{
  await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).map(p=>({...p,id:crypto.randomUUID()}));
  await invoke('vault_lock');await invoke('vault_create',{path:path.join(work,'values.ttvault'),name:'Büyük değer ve oturum testi',password:'Disposable-full-values-password'});
  await invoke('records_save',{records:profiles.map(p=>({id:p.id,kind:'databaseConnection',data:{...p,sshHostId:'',environment:'test'},updatedAt:Date.now()}))});
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
 });
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: explicit full value UI, export service bytes and transaction panel`,async()=>{
  const p=profiles.find(p=>p.engine===engine),schema=engine==='postgresql'?'public':engine==='mssql'?'dbo':'SYSTEM',table='tt_value_ui_'+Date.now().toString(36),quote=n=>engine==='mssql'?'['+n+']':'"'+n+'"',target=quote(schema)+'.'+quote(table),text='İstanbul 😀\n<literal> içerik </literal>\r\n'.repeat(500);
  const id=(await db({op:'open',recordId:p.id})).sessionId,q=(sql,parameters={})=>db({op:'query',sessionId:id,sql,parameters});
  try{
   await q(`CREATE TABLE ${target}(id INT PRIMARY KEY,txt ${engine==='oracle'?'CLOB':engine==='mssql'?'NVARCHAR(MAX)':'TEXT'},counter INT)`);await q(`INSERT INTO ${target} VALUES(1,:text,0)`,{text});
   await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`).click();await $(`button=${schema}`).waitForDisplayed({timeout:30000});await $(`button=${schema}`).click();await $(`button=${table}`).waitForDisplayed();await $(`button=${table}`).click();
   await $('.db-full-value').waitForDisplayed();await $('.db-full-value').click();assert.equal(await $('.db-lob-viewer pre').isExisting(),false);await $('button=Tam değeri yükle').click();await $('.db-value-hash').waitForDisplayed();assert.ok((await $('.db-value-hash').getText()).includes(createHash('sha256').update(text).digest('hex')));
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-full-value.png`));
   const output=path.join(work,engine+'.txt');
   // Embedded WebDriver cannot control the OS save dialog. Verify export through the real
   // service; this is not an end-to-end test of the Windows file chooser.
   assert.equal(await $('.db-lob-viewer').$('button=Dosyaya kaydet').isEnabled(),true);
   const value=await db({op:'lobRead',sessionId:id,schema,table,column:engine==='oracle'?'TXT':'txt',keys:{[engine==='oracle'?'ID':'id']:'1'}});
   await invoke('database_file',{action:'writeBytes',path:output,content:value.base64});
   assert.deepEqual(await fs.readFile(output),Buffer.from(text));
   await $('.modal > header button').click();await db({op:'transaction',sessionId:id,action:'autocommit',enabled:false});await q(`UPDATE ${target} SET counter=1`);
   await $('button=İndeks ve yedek').click();await $('button=Bağlantılar ve transaction').click();await $('input[aria-label="Transaction yaş sınırı"]').setValue('1');await $('button=Kanıtları getir').click();await $('[data-source="transactions"] table').waitForDisplayed();assert.ok((await $('[data-source="transactions"]').getText()).toLowerCase().includes('age_seconds'));
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-connection-transactions.png`));
   await db({op:'transaction',sessionId:id,action:'rollback'});await db({op:'transaction',sessionId:id,action:'autocommit',enabled:true});assert.equal((await q(`SELECT counter FROM ${target}`)).results[0].rows[0][0],'0');
  }finally{await db({op:'transaction',sessionId:id,action:'rollback'}).catch(()=>{});await db({op:'transaction',sessionId:id,action:'autocommit',enabled:true}).catch(()=>{});await q(`DROP TABLE ${target}`).catch(()=>{});await db({op:'close',sessionId:id}).catch(()=>{});}
 });
 it('binary export is byte-exact; invalid/oversize and locked-vault exports are rejected',async()=>{
  const sqlPath=path.join(work,'unicode.sql'),sql='SELECT \'İstanbul 😀\';\r\n';await invoke('database_file',{action:'write',path:sqlPath,content:sql});assert.equal(await invoke('database_file',{action:'read',path:sqlPath}),sql);
  const data=Buffer.from(Array.from({length:256},(_,i)=>i)),output=path.join(work,'bytes.bin');await invoke('database_file',{action:'writeBytes',path:output,content:data.toString('base64')});assert.deepEqual(await fs.readFile(output),data);
  await assert.rejects(()=>invoke('database_file',{action:'writeBytes',path:output,content:'not base64!'}));assert.deepEqual(await fs.readFile(output),data);
  await assert.rejects(()=>invoke('database_file',{action:'writeBytes',path:output,content:Buffer.alloc(8*1024*1024+1).toString('base64')}));assert.deepEqual(await fs.readFile(output),data);
  await assert.rejects(()=>invoke('database_file',{action:'writeBytes',path:path.join(work,'unsafe.exe'),content:'AA=='}));
  await invoke('vault_lock');await assert.rejects(()=>invoke('database_file',{action:'writeBytes',path:output,content:'AA=='}));await assert.rejects(()=>db({op:'lobRead',sessionId:'closed',schema:'public',table:'x',column:'y',keys:{}}));assert.deepEqual(await fs.readFile(output),data);
 });
});
