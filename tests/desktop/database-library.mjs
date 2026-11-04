// Personal SQL library works without PostgreSQL/Team or a reachable SQL target.
import {browser,$} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
const work=path.resolve('.lab/database/library-personal-'+Date.now());
const password='Disposable-library-native-1234', vaultPath=path.join(work,'library.ttvault');
const connection={id:crypto.randomUUID(),kind:'databaseConnection',updatedAt:Date.now(),data:{label:'Kişisel · Çevrimdışı kitaplık',engine:'postgresql',host:'127.0.0.1',port:1,database:'unreachable',username:'test',password:'disposable-local-only',tls:'disabled-local',groupId:''}};
async function invoke(command,args={}) {
  const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);
  if(r.error)throw Error(r.error);return r.value;
}
let book;
describe('Personal SQL library without a server',()=>{
  before(async()=>{
    await browser.setTimeout({script:120000});assert.ok((await invoke('app_info')).defaultVaultPath.includes('databasedev.e2e'));
    await fs.mkdir(work,{recursive:true});await invoke('vault_lock');
    await invoke('vault_create',{path:vaultPath,name:'Kişisel SQL kitaplığı',password});await invoke('records_save',{records:[connection]});
  });
  it('creates a runbook from the disconnected connection and never executes it',async()=>{
    await browser.refresh();await $('button=Veritabanları').waitForDisplayed();await $('button=Veritabanları').click();
    await $('[title="Sorgu kitaplığını görüntüle"]').click();
    await browser.execute(()=>{const s=document.querySelector('[aria-label="SQL belge türü"]');s.value='databaseRunbook';s.dispatchEvent(new Event('change',{bubbles:true}));});
    await $('[aria-label="SQL belge adı"]').setValue('Günlük PostgreSQL kontrolü');
    await $('[aria-label="SQL belge açıklaması"]').setValue('Önce oturumları incele, sonra kilitleri kontrol et. Her adımı ayrı çalıştır.');
    await $('[aria-label="Adım 1 başlığı"]').setValue('Etkin oturumları incele');
    await $('[aria-label="Adım 1 SQL"]').setValue("SELECT pid, usename, state\nFROM pg_stat_activity\nWHERE datname = current_database();");
    await $('button=Adım ekle').click();
    await $('[aria-label="Adım 2 başlığı"]').setValue('Bekleyen kilitleri incele');
    await $('[aria-label="Adım 2 SQL"]').setValue('SELECT pid, locktype, mode\nFROM pg_locks\nWHERE NOT granted;');
    await $('button=Belgeyi kaydet').click();await $('[role="status"]').waitForDisplayed();
    book=(await invoke('vault_info')).records.find(r=>r.kind==='databaseRunbook');assert.ok(book);
    assert.equal(await $('button=Editöre yükle · adım 1').isEnabled(),false);
    await browser.execute(()=>{document.querySelector('.db-library > section').scrollTop=0;});
    await browser.saveScreenshot(path.resolve('artifacts/database-workspace/sql-library-personal.png'));
  });
  it('reopens encrypted documents, preserves backup links and rejects orphan connections',async()=>{
    await invoke('vault_lock');await invoke('vault_open',{path:vaultPath,password});
    const loaded=await invoke('database_request',{request:{op:'libraryLoad',recordId:book.id}});assert.deepEqual(loaded.data,book.data);
    const contents=await fs.readFile(vaultPath);assert.equal(contents.includes(Buffer.from('pg_stat_activity')),false);
    await assert.rejects(()=>invoke('records_save',{records:[{...book,id:crypto.randomUUID(),data:{...book.data,connectionId:crypto.randomUUID()}}]}),/bağlantısı/);
    await assert.rejects(()=>invoke('records_delete',{ids:[connection.id]}),/referenced/);
    const backup=path.join(work,'library.ttbackup');await invoke('backup_create',{path:backup,password,includeProfiles:false,ids:[book.id]});
    const preview=await invoke('backup_preview',{path:backup,password});const rows=preview.vaults[0].records;
    assert.equal(rows.length,2);assert.equal(rows.find(r=>r.kind==='databaseRunbook').data.connectionId,rows.find(r=>r.kind==='databaseConnection').id);
  });
  after(async()=>{await invoke('vault_lock');});
});
