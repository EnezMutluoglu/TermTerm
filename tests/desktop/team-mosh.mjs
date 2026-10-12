// Real Team SSH chain bootstrap + bundled Windows Mosh client + WSL UDP server.
// The loopback UDP relay is a test-only accommodation for WSL NAT.
import {browser,$,$$} from '@wdio/globals';
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import dgram from 'node:dgram';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {proxy} from './proxy-fixtures.mjs';
async function invoke(command,args={}) {
  const r=await browser.executeAsync((c,a,done)=>window.__TAURI__.core.invoke(c,a).then(value=>done({value}),()=>done({failed:true})),command,args);
  if(r.failed)throw Error('Native Mosh operation rejected');return r.value;
}
const action=action=>invoke('team_v2_action',{action});
describe('Native Team Mosh',()=>{
  for (const proxyKind of ['http','socks5']) it(`uses saved ${proxyKind} auth and per-hop identity with real Mosh UDP`,async()=>{
    await browser.setTimeout({script:60000});
    await browser.refresh();
    const fixture=JSON.parse(await fs.readFile('.lab/team-v2-http/windows-native.json','utf8').catch(()=>fs.readFile('.lab/team-v2-http/offline-native-case.json','utf8')));
    const ssh=JSON.parse(await fs.readFile('.lab/team-v2-http/ssh-native.json','utf8'));
    const {stdout}=await promisify(execFile)('wsl.exe',['-d','Ubuntu-24.04','--','hostname','-I'],{windowsHide:true});
    const ip=stdout.trim().split(/\s+/).find(v=>/^(172\.|192\.168\.|10\.)/.test(v));assert(ip);
    await $('button*=Team hesabı').click();
    const fields=await $$('.ts-login input');
    await fields[0].setValue(fixture.profile.serviceUrl);await fields[1].setValue(fixture.profile.teamId);await fields[2].setValue('leader');
    await $('.ts-login summary').click();await $('.ts-login textarea').setValue(fixture.profile.caPem);
    await fields[3].setValue('Native-Team-Lab-Password-2026!');await $('button=Giriş yap').click();
    await $('.ts-workspace').waitForExist({timeout:30000});
    const server=await proxy(proxyKind,ssh.port,ssh.host);
    const endpoint={kind:proxyKind,host:'127.0.0.1',port:server.port,authenticate:true};
    const vault=(await action({kind:'vaults'}))[0].id,root=crypto.randomUUID(),hop=crypto.randomUUID(),key=crypto.randomUUID();
    await $('.vault-selector').click();await (await $$('.vault-menu button'))[0].click();
    await action({kind:'saveCredential',id:key,expectedRevision:0,credentialKind:'ssh_key',label:'Disposable Mosh key',secret:await fs.readFile('tests/ssh-keys/rsa2048.pkcs1','utf8'),passphrase:null});
    for(const [id,protocol] of [[hop,'ssh'],[root,'mosh']]) {
      await action({kind:'saveResource',vaultId:vault,id,expectedRevision:0,clientAt:new Date().toISOString(),content:{kind:'host',parentId:null,name:protocol==='mosh'?'Team Mosh':'Mosh jump',address:ssh.host,port:ssh.port,protocol,usernamePolicy:'shared_required',username:ssh.username,deleted:false,proxy:id===hop?endpoint:null}});
      await action({kind:'saveProfile',selection:{hostId:id,expectedRevision:0,method:'credential',credentialId:key,username:null}});
    }
    const plan=await action({kind:'prepareChain',id:root,hops:[hop]});
    await action({kind:'finishChain',planId:plan.planId,cancel:false});
    await action({kind:'saveProxy',root,hostId:hop,credentialId:null,revision:0,secret:{endpoint,username:'test',password:'proxy-pass'}});
    const document=crypto.randomUUID();
    await action({kind:'saveResource',vaultId:vault,id:document,expectedRevision:0,clientAt:new Date().toISOString(),content:{kind:'workspace',parentId:null,name:`Mosh workspace ${document}`,address:null,port:null,protocol:null,usernamePolicy:null,username:null,deleted:false,document:{type:'workspace',layout:'tabs',entries:[{hostId:root,startupCommand:"printf '\\124\\124\\137\\115\\117\\123\\110\\137\\104\\117\\103\\137\\117\\113\\n'",statsEnabled:false}]}}});
    const row=await action({kind:'resource',id:document});
    await browser.executeAsync(done=>{
      window.__mosh=[];
      Promise.all([
        window.__TAURI__.event.listen('session-event',e=>{
          window.__mosh.push(e.payload);
          // The mounted terminal handles terminal queries and input modes.
        }),
        window.__TAURI__.event.listen('session-prompt',e=>{
          if(e.payload.kind==='hostKey'&&e.payload.detail.address==='[127.0.0.1]:41843')window.__TAURI__.core.invoke('prompt_answer',{id:e.payload.id,answers:['trust']});
        }),
      ]).then(()=>done());
    });
    let session,relay,uplink; let outbound=0,inbound=0;
    try {
      await $('button=Komutlar / Workspaces').click();await $('button[aria-label="Yenile"]').click();
      await $('.ts-document-list').$(`button*=Mosh workspace ${document}`).click();
      await $('button=Önizlenen bağlantıları aç').click();
      await browser.waitUntil(async()=>{session=await browser.execute(()=>window.__mosh.find(e=>e.kind==='status')?.id);return !!session;},{timeout:20000});
      let port;
      await browser.waitUntil(async()=>{
        const status=await browser.execute(id=>window.__mosh.filter(e=>e.id===id&&e.kind==='status').map(e=>e.detail),session);
        for(const text of status){const match=String(text).match(/127\.0\.0\.1:(\d+) .*UDP/);if(match)port=Number(match[1]);}
        return Boolean(port);
      },{timeout:30000,timeoutMsg:'Mosh SSH bootstrap did not provide a UDP endpoint'});
      relay=dgram.createSocket('udp4');uplink=dgram.createSocket('udp4');let peer;
      await new Promise((resolve,reject)=>{uplink.once('error',reject);uplink.connect(port,ip,resolve);});
      uplink.on('message',message=>{inbound++;if(peer)relay.send(message,peer.port,'127.0.0.1');});
      relay.on('message',(message,remote)=>{
        if(remote.address==='127.0.0.1'){outbound++;peer=remote;uplink.send(message);}
      });
      await new Promise((resolve,reject)=>{relay.once('error',reject);relay.bind(port,'127.0.0.1',resolve);});
      await browser.waitUntil(async()=>await browser.execute(id=>window.__mosh.some(e=>e.id===id&&e.kind==='connected'),session),{timeout:15000});
      await browser.waitUntil(async()=>await browser.execute(id=>window.__mosh.filter(e=>e.id===id&&e.kind==='data').map(e=>atob(e.detail)).join('').includes('TT_MOSH_DOC_OK'),session),{timeout:25000,timeoutMsg:'Workspace startup command did not execute over Mosh'});
      await invoke('session_input',{id:session,cols:150,rows:42});
      await invoke('session_input',{id:session,data:"printf '__TEAM_MOSH_%s__\\n' 'OK'\n"});
      await browser.waitUntil(async()=>await browser.execute(id=>window.__mosh.filter(e=>e.id===id&&e.kind==='data').map(e=>atob(e.detail)).join('').includes('__TEAM_MOSH_OK__'),session),{timeout:25000,timeoutMsg:'Mosh UDP did not exchange real shell output'});
      // Mosh's documented escape exits the remote server before local teardown.
      await invoke('session_input',{id:session,data:'\n\x1e.'});
      await browser.waitUntil(async()=>await browser.execute(id=>window.__mosh.some(e=>e.id===id&&e.kind==='closed'),session),{timeout:15000});
    } catch(e) {
      const diagnostics=await browser.execute(id=>window.__mosh.filter(e=>e.id===id&&(e.kind==='error'||e.kind==='data')).map(e=>e.kind==='data'?atob(e.detail):e.detail).join('').slice(-1800),session);
      console.error('Disposable Mosh diagnostic', {outbound,inbound,diagnostics});throw e;
    } finally {if(session)await invoke('session_input',{id:session,close:true}).catch(()=>{});relay?.close();uplink?.close();server.close();await $('button[aria-label="Team oturumunu kapat"]').click();await $('button=Kişisel kasa / geri').click().catch(()=>{});}
  });
});
