// Focused regression using the disposable account/route from the native Team suite.
import {browser} from '@wdio/globals';
import fs from 'node:fs/promises';
import https from 'node:https';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
let fixture;
async function invoke(command,args={}) {
 const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),()=>done({failed:true})),command,args);
 if(r.failed)throw Error('Lab IPC rejected');return r.value;
}
const action=a=>invoke('team_v2_action',{action:a});
async function http(endpoint,body,token){
 return await new Promise((resolve,reject)=>{const req=https.request(fixture.profile.serviceUrl+endpoint,{method:'POST',ca:fixture.profile.caPem,headers:{'content-type':'application/json','x-termterm-client-version':'0.4.1','x-termterm-api-version':'1',...(token?{authorization:'Bearer '+token}:{})}},res=>{
 let data='';res.on('data',c=>data+=c);res.on('end',()=>res.statusCode===200?resolve(JSON.parse(data)):reject(Error('Lab HTTP status '+res.statusCode)));
 });req.on('error',reject);req.end(JSON.stringify(body));});
}
describe('Focused live Team SFTP revocation',()=>{
 it('closes an existing SFTP transport after a connect deny',async function(){
  this.timeout(90000);
  fixture=JSON.parse(await fs.readFile('.lab/team-v2-http/windows-native.json','utf8'));
  const password='Native-Team-Lab-Password-2026!';
  await invoke('team_v2_login',{profile:fixture.profile,username:'native.user',password});
  const me=await action({kind:'overview'}),vaults=await action({kind:'vaults'});
  const rows=await action({kind:'resources',vaultId:vaults[0].id,after:null});
  const host=rows.records.find(r=>r.content?.kind==='host' && r.content.port===41843).id;
  const admin=await http('/v1/auth/login',{teamId:fixture.profile.teamId,username:'leader',password,deviceId:crypto.randomUUID(),deviceName:'Focused revocation'});
  async function permit(effect){const p=await http('/v1/access/plans',{accountId:me.accountId,resourceId:host,changes:[{permission:'connect',effect}]},admin.accessToken);assert.equal(p.requiredKeys,0);await http(`/v1/access/plans/${p.id}/finish`,{cancel:false},admin.accessToken);}
  let id=crypto.randomUUID(),terminal,tunnelId=crypto.randomUUID();
  try {
   await invoke('team_v2_logout');
   await invoke('team_v2_login',{profile:fixture.profile,username:'leader',password});
   const grant=await action({kind:'prepareAccess',accountId:me.accountId,resourceId:host,changes:[{permission:'connect',effect:'allow'}]});
   let after=0;do {const step=await action({kind:'stageAccess',planId:grant.id,after});after=step.nextAfter;}while(after!==null);
   await action({kind:'finishAccess',planId:grant.id,cancel:false});
   await invoke('team_v2_logout');
   await invoke('team_v2_login',{profile:fixture.profile,username:'native.user',password});
   await browser.executeAsync(done=>{window.__revocationEvents=[];Promise.all(['session-event','tunnel-event','sftp-closed'].map(type=>window.__TAURI__.event.listen(type,e=>window.__revocationEvents.push({type,...e.payload})))).then(()=>done());});
   terminal=await invoke('team_v2_start',{hostId:host});
   await browser.waitUntil(()=>browser.execute(id=>window.__revocationEvents.some(e=>e.id===id && e.kind==='connected'),terminal),{timeout:20000});
   await invoke('team_v2_tunnel',{id:tunnelId,tunnel:{hostId:host,mode:'local',bindAddress:'127.0.0.1',bindPort:0,targetAddress:'127.0.0.1',targetPort:41841}});
   const home=await invoke('team_v2_sftp',{id,hostId:host});
   assert(Array.isArray(await invoke('file_list',{endpoint:{connection:id,path:home}})));
   await permit('deny');
   await assert.rejects(invoke('team_v2_sftp',{id:crypto.randomUUID(),hostId:host}));
   const start=Date.now();
   await browser.waitUntil(async()=>{try{await invoke('file_list',{endpoint:{connection:id,path:home}});return false;}catch{return true;}},{timeout:25000,interval:500,timeoutMsg:'Existing SFTP must be revoked'});
   await browser.waitUntil(()=>browser.execute((id,tunnel)=>window.__revocationEvents.some(e=>e.id===id && e.kind==='closed') && window.__revocationEvents.some(e=>e.id===tunnel && e.status==='stopped'),terminal,tunnelId),{timeout:25000});
   assert(await browser.execute(id=>window.__revocationEvents.some(e=>e.type==='sftp-closed' && e.id===id),id));
   console.info('SSH, SFTP and tunnel revoked after milliseconds: '+(Date.now()-start));
  } finally {
   if(terminal)await invoke('session_input',{id:terminal,close:true}).catch(()=>{});
   await invoke('tunnel_stop',{id:tunnelId}).catch(()=>{});
   await invoke('sftp_disconnect',{id}).catch(()=>{});
   await invoke('team_v2_logout').catch(()=>{});
   await http('/v1/auth/logout',{},admin.accessToken);
  }
 });
});
