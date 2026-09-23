// Real isolated Team PostgreSQL + Windows native IPC, SSH chain and SFTP.
// Never opens a personal/production vault. Credentials remain in ignored .lab.
import {browser,$,expect} from '@wdio/globals';
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
const root=process.cwd(), password='Disposable-Team-native-1234';
const suffix=Date.now().toString(36),work=path.join(root,'.lab','team-'+suffix);
let profile,ssh,teamId,vaultId,owner,operator,group,host,jump;
const rec=(kind,data)=>({id:crypto.randomUUID(),kind,data,updatedAt:Date.now()});
async function invoke(command,args={}){
 const result=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);
 if(result.error)throw Error(result.error);return result.value;
}
const request=(action,body={})=>invoke('team_request',{action,body});
async function login(account,register=false){return invoke('team_auth',{profile,login:account.username,email:account.username+'@example.invalid',password,register});}
async function openVault(){return invoke('team_open_vault',{vaultId,name:'İstanbul Altyapısı',teamId});}
async function permission(userId,recordId,permission,effect='allow'){
 const overview=await request('overview');
 return request('acl_set',{teamId,vaultId,userId,recordId,permission,effect,aclRevision:overview.teams.find(t=>t.id===teamId).aclRevision});
}
async function events(){await browser.executeAsync(done=>{
 window.__teamEvents={};Promise.all([
 window.__TAURI__.event.listen('session-event',({payload:e})=>{let state=window.__teamEvents[e.id]??={text:'',status:''};if(e.kind==='data')state.text+=atob(e.detail);else state.status=e.kind;}),
 window.__TAURI__.event.listen('session-prompt',({payload:p})=>{if(p.kind==='hostKey'&&p.detail.address.includes('127.0.0.1'))void window.__TAURI__.core.invoke('prompt_answer',{id:p.id,answers:['trust']});})]).then(()=>done());
});}
describe('TermTerm Team native acceptance',()=>{
 before(async()=>{
  await fs.mkdir(work,{recursive:true});await browser.setTimeout({script:120000});
  const info=await invoke('app_info');if(!info.defaultVaultPath.includes('teamdev.e2e'))throw Error('Refusing non-isolated application identity');
  profile=JSON.parse(await fs.readFile(path.join(root,'.lab/team-windows.json'),'utf8'));profile.database='termterm_team_e2e';
  ssh=JSON.parse(await fs.readFile(path.join(root,'.lab/ssh.json'),'utf8'));
  owner={username:'owner_'+suffix};operator={username:'operator_'+suffix};
  const initial=await login(operator,true);operator.id=initial.user.id;await invoke('team_logout');
  const status=await login(owner,true);owner.id=status.user.id;
 });
 it('creates a real team, encrypted shared vault and host chain',async()=>{
  teamId=(await request('create_team',{name:'İstanbul Platform Ekibi'})).id;
  vaultId=(await request('create_vault',{teamId,name:'İstanbul Altyapısı'})).id;
  await openVault();
  group=rec('group',{label:'Üretim',groupId:''});
  jump=rec('host',{label:'Lab Jump',address:ssh.address,port:ssh.port,username:ssh.username,privateKey:ssh.privateKey,protocol:'ssh',groupId:''});
  host=rec('host',{label:'Linux Lab · Chain',address:ssh.address,port:ssh.port,username:ssh.username,privateKey:ssh.privateKey,protocol:'ssh',groupId:group.id,chain:[jump.id]});
  await invoke('records_save',{records:[group,jump,host]});await request('sync');
  const vault=await invoke('vault_info');expect(vault.records.filter(r=>r.kind==='host')).toHaveLength(2);
  await request('member_set',{teamId,userId:operator.id,role:'operator',active:true});
  for(const p of ['read','connect'])await permission(operator.id,host.id,p);
  await browser.refresh();await $('.team-workspace').waitForDisplayed({timeout:30000});
  await browser.saveScreenshot(path.join(root,'artifacts','team-vaults.png'));
  await $('button=Üyeler ve yetkiler').click();await $('.team-matrix').waitForDisplayed();
  await browser.saveScreenshot(path.join(root,'artifacts','team-permissions.png'));
 });
 it('operator sees only allowed host, cannot export/edit/jump independently, can use SSH chain and SFTP',async()=>{
  await invoke('team_logout');await login(operator);await openVault();
  let vault=await invoke('vault_info');const hosts=vault.records.filter(r=>r.kind==='host');expect(hosts.map(h=>h.id)).toEqual([host.id]);expect(hosts[0].data.privateKey).toBeUndefined();
  expect(vault.records.find(r=>r.id===group.id).data._teamPathOnly).toBe(true);
  await expect(invoke('export_preview',{format:'csv',secrets:false})).rejects.toThrow();
  await expect(invoke('records_save',{records:[{...host,data:{...host.data,label:'Forbidden'}}]})).rejects.toThrow();
  await events();
  const denied=await invoke('session_start',{hostId:jump.id,shell:null});
  await browser.waitUntil(async()=>await browser.execute(id=>window.__teamEvents[id]?.status==='closed',denied));
  const session=await invoke('session_start',{hostId:host.id,shell:null});
  await browser.waitUntil(async()=>await browser.execute(id=>window.__teamEvents[id]?.status==='connected',session),{timeout:45000});
  await invoke('session_input',{id:session,data:"printf '__TEAM_CHAIN_OK__\\n'\r"});
  await browser.waitUntil(async()=>await browser.execute(id=>window.__teamEvents[id]?.text.includes('__TEAM_CHAIN_OK__'),session));
  const sftpId=crypto.randomUUID();await invoke('sftp_connect',{id:sftpId,hostId:host.id});
  const entries=await invoke('file_list',{endpoint:{connection:sftpId,path:ssh.home}});expect(entries.some(e=>e.name==='welcome.txt')).toBe(true);
  const output=path.join(work,'from-lab.txt');
  await invoke('file_transfer',{id:crypto.randomUUID(),source:{connection:sftpId,path:ssh.home+'/welcome.txt'},dest:{connection:'local',path:output},overwrite:false});
  expect(await fs.readFile(output,'utf8')).toContain('TermTerm SFTP integration fixture');
  await invoke('sftp_disconnect',{id:sftpId});await invoke('session_input',{id:session,close:true});
 });
 it('keeps versions and demonstrates temporary use / current view',async()=>{
  await invoke('team_logout');await login(owner);await openVault();
  for(let n=0;n<12;n++){let r=(await invoke('vault_info')).records.find(r=>r.id===host.id);r.data.label='Linux Lab · r'+(n+2);await invoke('records_save',{records:[r]});await request('sync');}
  const history=await invoke('team_history',{recordId:host.id});expect(history).toHaveLength(11);
  await invoke('team_checkout',{recordId:host.id,revision:history.at(-1).revision,permanent:false});
  await invoke('team_checkout',{recordId:host.id,revision:null,permanent:false});
  await browser.refresh();await $('.team-workspace').waitForDisplayed({timeout:30000});
  await $('button=İşlem geçmişi').click();await $('.team-audit').waitForDisplayed();await browser.saveScreenshot(path.join(root,'artifacts','team-audit.png'));
  await fs.writeFile(path.join(work,'demo.json'),JSON.stringify({owner,operator,teamId,vaultId,password},null,2));
  console.log('Team native acceptance evidence saved to ignored lab directory.');
 });
});
