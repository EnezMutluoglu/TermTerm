import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import {createHash,randomUUID} from 'node:crypto';
import assert from 'node:assert/strict';
const fixtures=path.resolve('.lab/keepass-fixtures'),work=path.resolve('.lab/keepass-native-'+Date.now());
async function invoke(command,args={}){const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);if(r.error)throw Error(r.error);return r.value;}
const hash=b=>createHash('sha256').update(b).digest('hex');
describe('KeePass encrypted import native worker',()=>{
 before(async()=>{await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));await fs.mkdir(work,{recursive:true});});
 for(const format of ['kdbx','xml'])it(`${format}: preview, copy/update/skip, encrypted reopen and source unchanged`,async()=>{
  const source=path.join(fixtures,'sample.'+format),vaultPath=path.join(work,format+'.ttvault'),password='Disposable-native-vault-password',before=hash(await fs.readFile(source));
  await invoke('vault_lock');let v=await invoke('vault_create',{path:vaultPath,name:'KeePass '+format,password});
  const args={path:source,format:'auto',...(format==='kdbx'?{password:'test-only-password',keyFile:path.join(fixtures,'sample.keyx')}:{})};
  if(format==='kdbx') {await assert.rejects(()=>invoke('import_preview',{...args,password:'wrong'}));await assert.rejects(()=>invoke('import_preview',{...args,keyFile:null}));}
  const preview=await invoke('import_preview',args);assert.equal(preview.records.filter(r=>r.kind==='host').length,1);assert.equal(preview.records.filter(r=>r.kind==='group').length,2);
  const imported=await invoke('import_apply',{records:preview.records,policy:'copy',vaultId:v.id});assert.equal(imported.added,3);assert.equal(imported.failed,0);
  const skipped=await invoke('import_apply',{records:preview.records,policy:'skip',vaultId:v.id});assert.equal(skipped.skipped,3);assert.equal(skipped.added,0);
  const updated=await invoke('import_apply',{records:preview.records,policy:'update',vaultId:v.id});assert.equal(updated.updated,3);
  await invoke('vault_lock');v=await invoke('vault_open',{path:vaultPath,password});const h=v.records.find(r=>r.kind==='host'),groups=v.records.filter(r=>r.kind==='group');
  assert.equal(h.data.password,format==='kdbx'?'Disposable!{Password}':'{literal}!pass');assert.equal(h.data.port,2222);assert.ok(groups.some(g=>g.id===h.data.groupId&&g.data.label==='İstanbul'));assert.equal(hash(await fs.readFile(source)),before);
  const bytes=await fs.readFile(vaultPath);assert.ok(!bytes.includes(Buffer.from(h.data.password)));assert.ok(!bytes.includes(Buffer.from(h.data.address)));
  await browser.refresh();await browser.waitUntil(async()=>await $('.group-main').isExisting());await $('.group-main').click();await $('.group-main').waitForDisplayed();await $('.group-main').click();await $('.record-card').waitForDisplayed();await browser.saveScreenshot(path.resolve(`artifacts/database-workspace/keepass-${format}-import.png`));
 });
 it('cancellation leaves vault unchanged and worker slot reusable',async()=>{
  // A real encrypted reader is cancelled while its isolated KDF is running.
  const before=await invoke('vault_info');
  const id=randomUUID(),args={path:path.join(fixtures,'sample.kdbx'),format:'auto',password:'test-only-password',keyFile:path.join(fixtures,'sample.keyx'),operationId:id};
  await browser.execute((args)=>{window.__keepassPending=true;window.__TAURI__.core.invoke('import_preview',args).then(()=>{window.__keepassResult='finished';},e=>{window.__keepassResult=String(e);}).finally(()=>{window.__keepassPending=false;});},args);
  const start=Date.now();await invoke('operation_cancel',{id});await browser.waitUntil(async()=>!await browser.execute(()=>window.__keepassPending),{timeout:3000});assert.ok(Date.now()-start<3000);assert.match(await browser.execute(()=>window.__keepassResult),/cancel/i);
  const preview=await invoke('import_preview',{...args,operationId:randomUUID()});assert.equal(preview.records.filter(r=>r.kind==='host').length,1);assert.deepEqual((await invoke('vault_info')).records,before.records);
 });
 it('vault lock cancels active reader without importing records',async()=>{
  const args={path:path.join(fixtures,'sample.kdbx'),format:'auto',password:'test-only-password',keyFile:path.join(fixtures,'sample.keyx'),operationId:randomUUID()};
  await browser.execute((args)=>{window.__keepassPending=true;window.__TAURI__.core.invoke('import_preview',args).then(()=>{window.__keepassResult='finished';},e=>{window.__keepassResult=String(e);}).finally(()=>{window.__keepassPending=false;});},args);
  await invoke('vault_lock');await browser.waitUntil(async()=>!await browser.execute(()=>window.__keepassPending),{timeout:3000});assert.match(await browser.execute(()=>window.__keepassResult),/cancel/i);await assert.rejects(()=>invoke('vault_info'));
 });
 it('file/password/keyfile fields reach the import worker through the real wizard',async()=>{
  await invoke('vault_create',{path:path.join(work,'wizard.ttvault'),name:'KeePass sihirbaz',password:'Disposable-wizard-password'});await browser.refresh();await $('button[title="Import connections"]').waitForDisplayed();await $('button[title="Import connections"]').click();
  const field=label=>$(`//label[span[text()='${label}']]//input`);
  await field('Source file').setValue(path.join(fixtures,'sample.kdbx'));await field('Source password (if encrypted)').setValue('test-only-password');await field('KeePass keyfile (isteğe bağlı)').setValue(path.join(fixtures,'sample.keyx'));
  await $('button=Preview import').click();await $('button=Import records').waitForEnabled({timeout:30000});assert.equal((await invoke('vault_info')).records.filter(r=>r.kind==='host').length,0);
  await browser.saveScreenshot(path.resolve('artifacts/database-workspace/keepass-wizard-preview.png'));await $('button=Import records').click();await browser.waitUntil(async()=>(await invoke('vault_info')).records.filter(r=>r.kind==='host').length===1);
 });
 after(async()=>{await invoke('vault_lock');});
});
