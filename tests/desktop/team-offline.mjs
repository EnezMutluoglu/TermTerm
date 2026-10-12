// Three separate native application processes: warm / offline / resume.
// Stop only the disposable TLS fixture between warm and offline; keep SSH up.
import {browser,$,$$,expect} from '@wdio/globals';
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import path from 'node:path';
const phase=process.env.TERMTERM_TEAM_OFFLINE_PHASE;
const file='.lab/team-v2-http/offline-native-case.json';
const password='Native-Team-Lab-Password-2026!';
let data;
async function invoke(command,args={}){
  const result=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),()=>done({failed:true})),command,args);
  if(result.failed)throw Error('Native offline test operation rejected');return result.value;
}
const action=action=>invoke('team_v2_action',{action});
async function login(){
  await $('button*=Team hesabı').click();
  const inputs=await $$('.ts-login input');
  await inputs[0].setValue(data.profile.serviceUrl);await inputs[1].setValue(data.profile.teamId);
  await inputs[2].setValue('leader');await inputs[3].setValue(password);
  await $('.ts-login summary').click();await $('.ts-login textarea').setValue(data.profile.caPem);
  await $('button=Giriş yap').click();await $('.ts-workspace').waitForDisplayed({timeout:30000});
}
async function connect(){
  await browser.executeAsync(done=>{
    window.__offlineEvents=[];
    Promise.all([
      window.__TAURI__.event.listen('session-event',e=>window.__offlineEvents.push(e.payload)),
      window.__TAURI__.event.listen('session-prompt',e=>{
        if(e.payload.kind==='hostKey'&&e.payload.detail.address==='[127.0.0.1]:41843')
          window.__TAURI__.core.invoke('prompt_answer',{id:e.payload.id,answers:['trust']});
      }),
    ]).then(()=>done());
  });
  const document=await action({kind:'resource',id:data.document});
  const opened=await invoke('team_v2_document_start',{documentId:data.document,targetId:null,expectedRevision:document.revision,localGeneration:document.localGeneration});
  assert.equal(opened.sessions.length,1);assert.equal(opened.sessions[0].statsEnabled,false);
  const session=opened.sessions[0].id;
  await browser.waitUntil(async()=>await browser.execute(id=>window.__offlineEvents.some(e=>e.id===id&&e.kind==='connected'),session),{timeout:25000});
  await browser.waitUntil(async()=>await browser.execute(id=>window.__offlineEvents.filter(e=>e.id===id&&e.kind==='data').map(e=>atob(e.detail)).join('').includes('TT_OFFLINE_DOC_OK'),session),{timeout:15000});
  await invoke('session_input',{id:session,data:"printf '__OFFLINE_%s__\\n' 'CONNECTED'\n"});
  await browser.waitUntil(async()=>await browser.execute(id=>window.__offlineEvents.filter(e=>e.id===id&&e.kind==='data').map(e=>atob(e.detail)).join('').includes('__OFFLINE_CONNECTED__'),session),{timeout:15000});
  await invoke('session_input',{id:session,close:true});
}
describe(`Native offline cache: ${phase}`,()=>{
  before(async()=>{
    assert(['warm','offline','resume'].includes(phase));
    await browser.setTimeout({script:60000});
    if(phase==='warm'){
      const fixture=JSON.parse(await fs.readFile('.lab/team-v2-http/windows-native.json','utf8'));
      data={profile:fixture.profile,vault:crypto.randomUUID(),host:crypto.randomUUID(),created:crypto.randomUUID(),document:crypto.randomUUID()};
    }else data=JSON.parse(await fs.readFile(file,'utf8'));
  });
  it('preserves the native password-protected cache and verifies real SSH',async()=>{
    await login();
    if(phase==='warm'){
      const ssh=JSON.parse(await fs.readFile('.lab/team-v2-http/ssh-native.json','utf8'));
      await action({kind:'createRecovery',path:path.resolve('.lab/team-v2-http/offline-'+crypto.randomUUID()+'.ttrecovery'),password:'Recovery-Offline-Lab-2026!',reauthentication:password});
      await action({kind:'createVault',id:data.vault,name:'Çevrimdışı kabul kasası'});
      data.content={kind:'host',parentId:null,name:'Çevrimdışı SSH',address:ssh.host,protocol:'ssh',port:ssh.port,usernamePolicy:'shared_required',username:ssh.username,deleted:false};
      await action({kind:'saveResource',vaultId:data.vault,id:data.host,expectedRevision:0,clientAt:new Date().toISOString(),content:data.content});
      const key=crypto.randomUUID();
      await action({kind:'saveCredential',id:key,expectedRevision:0,credentialKind:'ssh_key',label:'Disposable offline key',secret:await fs.readFile('tests/ssh-keys/rsa2048.pkcs1','utf8'),passphrase:null});
      await action({kind:'saveProfile',selection:{hostId:data.host,expectedRevision:0,method:'credential',credentialId:key,username:null}});
      data.documentContent={kind:'workspace',parentId:null,name:'Çevrimdışı workspace',address:null,protocol:null,port:null,usernamePolicy:null,username:null,deleted:false,document:{type:'workspace',layout:'tabs',entries:[{hostId:data.host,startupCommand:"printf '\\124\\124\\137\\117\\106\\106\\114\\111\\116\\105\\137\\104\\117\\103\\137\\117\\113\\n'",statsEnabled:false}]}};
      await action({kind:'saveResource',vaultId:data.vault,id:data.document,expectedRevision:0,clientAt:new Date().toISOString(),content:data.documentContent});
      await action({kind:'resource',id:data.document});
      await action({kind:'resource',id:data.host});await action({kind:'vaults'});await action({kind:'historySelections'});
      await action({kind:'resources',vaultId:data.vault,after:null});
      await connect();await fs.writeFile(file,JSON.stringify(data));return;
    }
    if(phase==='offline'){
      assert.equal((await action({kind:'overview'})).offline,true);
      await connect();
      const cached=await action({kind:'resource',id:data.host});
      const changed=cached.content.name==='Çevrimdışı kayıt korundu'?{status:'queued'}:await action({kind:'saveResource',vaultId:data.vault,id:data.host,expectedRevision:1,localGeneration:cached.localGeneration,clientAt:new Date().toISOString(),content:{...data.content,name:'Çevrimdışı kayıt korundu'}});
      assert.equal(changed.status,'queued');
      await action({kind:'saveResource',vaultId:data.vault,id:data.created,expectedRevision:0,clientAt:new Date().toISOString(),content:{...data.content,name:'Çevrimdışı yeni kayıt'}});
      const document=await action({kind:'resource',id:data.document});
      assert.equal((await action({kind:'saveResource',vaultId:data.vault,id:data.document,expectedRevision:1,localGeneration:document.localGeneration,clientAt:new Date().toISOString(),content:{...data.documentContent,name:'Workspace değişikliği korundu'}})).status,'queued');
      assert.equal((await action({kind:'journalStatus'})).pending,3);
      await $('.vault-selector').click();await $('button=Çevrimdışı kabul kasası').click();
      await $('.record-card').waitForDisplayed({timeout:60000});
      await browser.saveScreenshot('artifacts/team-v2/windows-offline.png');return;
    }
    // Background synchronization may have already delivered the operation.
    for(let i=0;i<4&&(await action({kind:'journalStatus'})).pending;i++)await action({kind:'syncJournal'});
    const row=await action({kind:'resource',id:data.host});
    assert.equal(row.content.name,'Çevrimdışı kayıt korundu');assert.equal(row.revision,2);
    assert.equal((await action({kind:'journalStatus'})).pending,0);
    const created=await action({kind:'serverResource',id:data.created});
    assert.equal(created.revision,1);assert.equal(created.content.name,'Çevrimdışı yeni kayıt');
    const document=await action({kind:'serverResource',id:data.document});
    assert.equal(document.revision,2);assert.equal(document.content.name,'Workspace değişikliği korundu');
    assert.deepEqual(document.content.document,data.documentContent.document);
    await connect();
  });
});
