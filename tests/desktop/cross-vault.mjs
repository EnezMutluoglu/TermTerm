import {browser,expect} from '@wdio/globals';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';

// Input is a disposable fixture produced by platform-smoke, never a personal vault.
const password='Disposable-platform-test-password';
async function invoke(command,args={}){
  const result=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);
  if(result.error)throw Error(result.error);return result.value;
}
describe('Password-only vault portability',()=>{
  it('opens a copied Windows fixture, edits it, and restores its backup',async()=>{
    if(!process.env.TERMTERM_PORTABLE_FIXTURE)throw Error('Set TERMTERM_PORTABLE_FIXTURE to platform-smoke copy.ttvault');
    await browser.setTimeout({script:120000});
    const dir=await fs.mkdtemp(path.join(os.tmpdir(),'termterm-portability-'));
    const file=path.join(dir,'foreign.ttvault');
    await fs.copyFile(process.env.TERMTERM_PORTABLE_FIXTURE,file);
    const before=await invoke('vault_open',{path:file,password});
    expect(before.records.filter(r=>r.kind==='host')).toHaveLength(3);
    expect(before.records.find(r=>r.kind==='settings').data.terminalTheme).toBe('forest');
    const outer=before.records.find(r=>r.data.label==='Outer folder');
    const inner=before.records.find(r=>r.data.label==='Inner folder');
    expect(inner.data.groupId).toBe(outer.id);
    expect(before.records.find(r=>r.data.label==='Inner host').data.groupId).toBe(inner.id);
    await invoke('records_save',{records:[{id:crypto.randomUUID(),kind:'snippet',updatedAt:Date.now(),data:{label:'Linux → Windows Türkçe',command:'echo Şişli'}}]});
    const backup=path.join(dir,'edited.ttbackup');
    await invoke('backup_create',{path:backup,password,includeProfiles:false,ids:[]});
    await invoke('vault_lock');
    const restored=await invoke('backup_restore',{source:backup,password,path:path.join(dir,'restored.ttvault'),newPassword:password,index:0});
    expect(restored.records.find(r=>r.data.label==='Linux → Windows Türkçe').data.command).toBe('echo Şişli');
    const restoredInner=restored.records.find(r=>r.data.label==='Inner folder');
    expect(restored.records.find(r=>r.data.label==='Inner host').data.groupId).toBe(restoredInner.id);
    expect(restored.id).not.toBe(before.id);
    await invoke('vault_copy',{path:path.join(dir,'return-copy.ttvault')});
    await invoke('vault_lock');
    console.log('Portable fixture verified on',process.platform,'in',dir);
  });
});
