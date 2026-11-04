import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {randomBytes} from 'node:crypto';
import assert from 'node:assert/strict';
const root=process.cwd(),stamp=Date.now().toString(36),work=path.join(root,'.lab/database/provision-native-'+stamp),adminName='tt_native_'+stamp,adminPassword='Tt9!'+randomBytes(20).toString('hex');
async function pg(sql){const file=path.join(root,'.lab/database/provision-native.sql');await fs.writeFile(file,sql);execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f','/mnt/c/Users/sonx/Desktop/termterm/.lab/database/provision-native.sql'],{stdio:'pipe',windowsHide:true});}
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const db=request=>invoke('database_request',{request});
describe('Database provisioning native',()=>{
 let profiles;
 before(async()=>{
  await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
  await fs.mkdir(work,{recursive:true});await pg(`CREATE ROLE ${adminName} LOGIN CREATEDB CREATEROLE PASSWORD '${adminPassword}';`);
  profiles=JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).map(p=>({...p,id:crypto.randomUUID(),...(p.engine==='postgresql'?{username:adminName,password:adminPassword}:{})}));
  profiles.push({...JSON.parse(await fs.readFile('.lab/database/profiles.json','utf8')).find(p=>p.engine==='postgresql'),id:crypto.randomUUID(),label:'PostgreSQL · Extension lab'});
  await invoke('vault_lock');await invoke('vault_create',{path:path.join(work,'provision.ttvault'),name:'DB kurulum testi',password:'Disposable-provision-vault-password'});
  await invoke('records_save',{records:profiles.map(p=>({id:p.id,kind:'databaseConnection',data:{...p,sshHostId:'',environment:'test'},updatedAt:Date.now()}))});
  await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
 });
 after(async()=>{await invoke('vault_lock').catch(()=>{});await pg(`DROP ROLE IF EXISTS ${adminName};`);});
 for(const engine of ['postgresql','mssql','oracle'])it(`${engine}: real creation, secret-free preview and one-time passwords`,async()=>{
  const p=profiles.find(p=>p.engine===engine),name='tt_ui_'+engine+'_'+stamp,qn=x=>engine==='mssql'?'['+x+']':'"'+(engine==='oracle'?x.toUpperCase():x)+'"';
  const sessionId=(await db({op:'open',recordId:p.id})).sessionId,q=sql=>db({op:'query',sessionId,sql});
  try{
   await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`).click();await $('button=DB / kullanıcı oluştur').waitForEnabled({timeout:30000});await $('button=DB / kullanıcı oluştur').click();
   await $('input[aria-label="Yeni veritabanı adı"]').setValue(name);await browser.waitUntil(()=>$('button=Kurulum planını hazırla').isEnabled());await $('button=Kurulum planını hazırla').click();await $('.db-change-preview').waitForDisplayed();
   assert.equal(await $('button=Onaylanan kurulumu uygula').isEnabled(),false);assert.equal(await $('[data-secret="password"]').isExisting(),false);assert.ok((await $('.db-change-preview pre').getText()).includes('<OWNER_PASSWORD>'));
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-provision-preview.png`));
   await $('.db-maintenance-confirm input').click();await $('button=Onaylanan kurulumu uygula').click();await $('[data-secret="password"]').waitForDisplayed({timeout:60000});
   // Never return generated secrets to WDIO logs, screenshot files or assertions.
   assert.equal(await browser.execute(()=>Array.from(document.querySelectorAll('[data-secret="password"]')).every(e=>e.textContent.length===21&&/[A-Z]/.test(e.textContent)&&/[a-z]/.test(e.textContent)&&/[0-9]/.test(e.textContent))),true);
   assert.equal(await $('.db-change-outcome > strong').getText(),'Tamamlandı');
   await $('button=Parolaları aldım, kapat').click();assert.equal(await $('[data-secret="password"]').isExisting(),false);
   await $('button=DB / kullanıcı oluştur').click();assert.equal(await $('[data-secret="password"]').isExisting(),false);
   await $('input[aria-label="Yeni veritabanı adı"]').setValue(name);await $('button=Mevcut / yarım kurulumu denetle').click();await $('.db-provision-inspection').waitForDisplayed();
   assert.ok((await $('.db-provision-inspection').getText()).includes('Salt okunur'));
   await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${engine}-provision-inspection.png`));await $('.modal > header button').click();
   const result=await q(engine==='oracle'?`SELECT count(*) FROM dba_users WHERE username='${name.toUpperCase()}_OWNER'`:engine==='mssql'?`SELECT count(*) FROM sys.databases WHERE name='${name}'`:`SELECT count(*) FROM pg_database WHERE datname='${name}'`);assert.equal(result.results[0].rows[0][0],'1');
  }finally{
   if(await $('.modal > header button').isExisting())await $('.modal > header button').click();
   if(engine==='postgresql'){await pg(`DROP DATABASE IF EXISTS ${name} WITH(FORCE);`);for(const suffix of ['_appuser','_readonly','_developer','_app','_owner',''])await pg(`DROP ROLE IF EXISTS ${name+suffix};`);}
   else if(engine==='mssql'){await q(`IF DB_ID('${name}') IS NOT NULL BEGIN ALTER DATABASE ${qn(name)} SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE ${qn(name)}; END`);for(const suffix of ['_appuser','_owner'])await q(`IF SUSER_ID('${name+suffix}') IS NOT NULL DROP LOGIN ${qn(name+suffix)}`);}
   else{for(const suffix of ['_appuser','_owner'])await q(`DROP USER ${qn(name+suffix)} CASCADE`).catch(()=>{});for(const suffix of ['_readonly','_developer','_app',''])await q(`DROP ROLE ${qn(name+suffix)}`).catch(()=>{});}
   await db({op:'close',sessionId}).catch(()=>{});
  }
 });
 it('PaaS evidence panel and explicit provider limitations',async()=>{
  const p=profiles.find(p=>p.label==='PostgreSQL · Extension lab');await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`).click();await $('button=DB / kullanıcı oluştur').waitForEnabled({timeout:30000});await $('button=DB / kullanıcı oluştur').click();await browser.waitUntil(()=>$('button=PaaS ve extension\'lar').isEnabled());
  await $('button=PaaS ve extension\'lar').click();await $('select[aria-label="Kurulacak extension"]').waitForDisplayed();
  assert.ok((await $('.db-provision').getText()).includes('gerçek Azure/AWS/Huawei ortamı doğrulanmadı'));
  await browser.execute(()=>{const e=document.querySelector('select[aria-label="Kurulacak extension"]');e.value='hstore';e.dispatchEvent(new Event('change',{bubbles:true}));});
  await $('button=Extension planını hazırla').click();await $('.db-change-preview').waitForDisplayed();assert.ok((await $('.db-change-preview pre').getText()).includes('CREATE EXTENSION "hstore"'));assert.equal(await $('button=Onaylanan kurulumu uygula').isEnabled(),false);
  await browser.saveScreenshot(path.join(root,'artifacts/database-workspace/postgresql-paas-extensions.png'));await $('.modal > header button').click();
 });
 it('locked vault rejects provisioning and extension IPC',async()=>{await invoke('vault_lock');for(const op of ['provisionPreview','provisionApply','extensionEnvironment','extensionPreview','extensionApply'])await assert.rejects(()=>db({op,sessionId:'old'}));});
});
