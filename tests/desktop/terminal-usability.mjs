import { browser, $, $$, expect } from '@wdio/globals';
import { doubleClick } from './interaction.mjs';
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
const root=process.cwd(), work=path.join(root,'.lab','terminal-ui-'+Date.now());
const record=(kind,data)=>({id:crypto.randomUUID(),kind,data,updatedAt:Date.now()});
async function invoke(command,args={}) {
  const r=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);
  if(r.error)throw Error(r.error);return r.value;
}
async function refresh(){await browser.executeAsync(done=>window.__TAURI__.event.emit('vault-changed').then(()=>done(true)));}
const output=id=>browser.execute(id=>window.__output[id]??'',id);
const focusedId=()=>$('.terminal-pane.focused').getAttribute('data-session-id');
const clipboardRead=()=>invoke('plugin:clipboard-manager|read_text');
const clipboardWrite=text=>invoke('plugin:clipboard-manager|write_text',{text});
// The embedded driver does not map Insert. Dispatch complete DOM key events,
// then assert the bytes received by the real SSH process and system clipboard.
async function key(key,code,keyCode,modifiers={}) {
  await browser.execute((key,code,keyCode,modifiers)=>{
    const target=document.activeElement;
    for(const type of ['keydown','keyup'])target.dispatchEvent(new KeyboardEvent(type,{key,code,keyCode,which:keyCode,bubbles:true,cancelable:true,...modifiers}));
  },key,code,keyCode,modifiers);
}
async function paste(text){await clipboardWrite(text);await browser.execute(()=>document.querySelector('.terminal-pane.focused .xterm-helper-textarea').focus());await key('Insert','Insert',45,{shiftKey:true});await browser.pause(150);}
async function command(text){await paste(text);await key('Enter','Enter',13);}
const shot=name=>browser.saveScreenshot(path.join(root,'artifacts',`terminal-ui-${name}.png`));
let ssh,host,id;
describe('Native Windows terminal usability',()=>{
  afterEach(async function(){
    if(this.currentTest.state==='failed'&&id)console.log('Lab SSH output:',JSON.stringify((await output(id)).slice(-3000)));
  });
  before(async()=>{
    await fs.mkdir(work,{recursive:true});
    ssh=JSON.parse(await fs.readFile(process.env.TERMTERM_SSH_FIXTURE??path.join(root,'.lab/ssh.json'),'utf8'));
    await browser.setTimeout({script:120000});
    await browser.executeAsync(done=>{
      window.__output={};window.__metrics={};
      Promise.all([
        window.__TAURI__.event.listen('session-event',({payload:e})=>{if(e.kind==='data')window.__output[e.id]=(window.__output[e.id]??'')+atob(e.detail);}),
        window.__TAURI__.event.listen('session-metrics',({payload:e})=>window.__metrics[e.sessionId]=e),
      ]).then(()=>done(true));
    });
    await invoke('vault_create',{path:path.join(work,'test.ttvault'),name:'Terminal design lab',password:'Disposable-ui-test-vault-2026'});
    host=record('host',{label:'Linux lab · SSH',address:'127.0.0.1',port:22222,username:ssh.username,password:ssh.password,protocol:'ssh'});
    const known=record('knownHost',{label:'Lab server',address:'[127.0.0.1]:22222',publicKey:ssh.publicKey});
    await invoke('records_save',{records:[host,known]});await refresh();
    await $('h1=Hosts').waitForDisplayed();
  });
  it('opens SSH in the shared top row and receives real CPU/memory/disk samples',async()=>{
    await doubleClick($('.group-main*=Ungrouped'));await doubleClick($('.record-card'));
    await $('.terminal-status=Connected').waitForDisplayed({timeout:45000});id=await focusedId();
    await expect($$('.top-session-tab')).toBeElementsArrayOfSize(1);
    await command("printf '\\033[2J\\033[H\\033[1;32mtermterm@linux-lab\\033[0m:~/projects $ status\\n\\033[32m✓ SSH connected\\033[0m  \\033[36m✓ Secure vault\\033[0m\\n\\033[33mCPU / memory / mounted disks are live\\033[0m\\nCOPY_THIS_TEXT Terminal clipboard\\nTürkçe: ğüşöçıİ  —  Unicode ready\\n'; for i in 31 32 33 34 35 36 91 92 93 94 95 96; do printf '\\033[%sm ANSI %s \\033[0m' \"$i\" \"$i\"; done; printf '\\n' ");
    await browser.waitUntil(async()=>await browser.execute(id=>window.__metrics[id]?.memory?.total>0 && window.__metrics[id]?.cpuPercent!=null,id),{timeout:45000});
    await shot('single');
  });
  it('copies a mouse selection to a separate Windows text editor control',async()=>{
    await command("printf '\\033[2J\\033[HCOPY_THIS_TEXT\\n'");
    await browser.waitUntil(async()=>(await output(id)).includes('COPY_THIS_TEXT'),{timeout:10000});
    await browser.pause(250);
    await clipboardWrite('BEFORE_SELECTION');
    // Embedded WDIO omits pointer events and click counts. Use a complete mouse
    // sequence inside the real WebView; the browser suite uses trusted input.
    await browser.execute(()=>{
      const surface=document.querySelector('.terminal-pane.focused .terminal-surface');
      const b=surface.querySelector('.xterm-screen').getBoundingClientRect();
      const target=document.elementFromPoint(b.x+2,b.y+9);
      target.dispatchEvent(new PointerEvent('pointerdown',{button:0,bubbles:true}));
      target.dispatchEvent(new MouseEvent('mousedown',{button:0,buttons:1,detail:1,clientX:b.x+2,clientY:b.y+9,bubbles:true,cancelable:true}));
      document.dispatchEvent(new MouseEvent('mousemove',{button:0,buttons:1,clientX:b.x+180,clientY:b.y+9,bubbles:true}));
      document.dispatchEvent(new MouseEvent('mouseup',{button:0,buttons:0,clientX:b.x+180,clientY:b.y+9,bubbles:true}));
      window.dispatchEvent(new PointerEvent('pointerup',{button:0,bubbles:true}));
    });
    await browser.waitUntil(async()=>(await clipboardRead()).includes('COPY_THIS_TEXT'),{timeout:10000});
    const ps='Add-Type -AssemblyName System.Windows.Forms; $edit = New-Object System.Windows.Forms.TextBox; $null=$edit.Handle; $edit.Multiline=$true; $edit.Paste(); [Console]::Write($edit.Text); $edit.Dispose()';
    const external=execFileSync('powershell.exe',['-NoProfile','-STA','-Command',ps],{encoding:'utf8',windowsHide:true});
    expect(external).toContain('COPY_THIS_TEXT');
  });
  it('accepts external-editor copy with right click and Shift+Insert in shell and editor',async()=>{
    const text="printf 'EXTERNAL_PASTE_OK\\n'";
    const ps=`Add-Type -AssemblyName System.Windows.Forms; $edit = New-Object System.Windows.Forms.TextBox; $null=$edit.Handle; $edit.Text=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('${Buffer.from(text).toString('base64')}')); $edit.SelectAll(); $edit.Copy(); $edit.Dispose()`;
    execFileSync('powershell.exe',['-NoProfile','-STA','-Command',ps],{windowsHide:true});
    expect(await clipboardRead()).toBe(text);
    // Embedded WDIO emits down/up/click but omits the browser contextmenu event.
    // Playwright separately exercises the complete trusted right-click sequence.
    await browser.execute(()=>document.querySelector('.terminal-pane.focused .terminal-surface').dispatchEvent(new MouseEvent('contextmenu',{button:2,bubbles:true,cancelable:true})));
    await browser.pause(150);await key('Enter','Enter',13);
    await browser.waitUntil(async()=>(await output(id)).includes('EXTERNAL_PASTE_OK\r\n'),{timeout:10000});
    await command("printf 'SHIFT_INSERT_OK\\n'");
    await browser.waitUntil(async()=>(await output(id)).includes('SHIFT_INSERT_OK\r\n'),{timeout:10000});
    await command("rm -f /tmp/termterm-ui-clipboard.txt; vi /tmp/termterm-ui-clipboard.txt");await browser.waitUntil(async()=>(await output(id)).includes('[New]'),{timeout:15000});await invoke('session_input',{id,data:'i'});await browser.waitUntil(async()=>(await output(id)).includes('-- INSERT --'),{timeout:10000});
    await paste('Türkçe: ğüşöçıİ\nSecond editor line');await key('Escape','Escape',27);await browser.pause(150);await invoke('session_input',{id,data:':wq\r'});await browser.pause(300);
    await command("python3 -c \"from pathlib import Path; print('EDITOR_HEX='+Path('/tmp/termterm-ui-clipboard.txt').read_bytes().hex())\"");
    await browser.waitUntil(async()=>(await output(id)).includes(Buffer.from('Türkçe: ğüşöçıİ\nSecond editor line').toString('hex')),{timeout:10000});
  });
  it('preserves shell Control keys, function keys and UTF-8 bytes over real SSH',async()=>{
    const script="import os,tty,termios,select; old=termios.tcgetattr(0); tty.setraw(0); os.write(1,b'RAW_READY\\r\\n'); data=b''\nwhile True:\n r,_,_=select.select([0],[],[],2)\n if not r: break\n data+=os.read(0,4096)\ntermios.tcsetattr(0,termios.TCSANOW,old); print('KEY_HEX='+data.hex())";
    await command(`python3 -c "import base64; exec(base64.b64decode('${Buffer.from(script).toString('base64')}'))"`);
    await browser.waitUntil(async()=>(await output(id)).includes('RAW_READY\r\n'),{timeout:10000});
    for(const ch of ['c','d','z','l','k','n'])await key(ch,'Key'+ch.toUpperCase(),ch.toUpperCase().charCodeAt(0),{ctrlKey:true});
    await key('F1','F1',112);await key('F12','F12',123);await key('ArrowUp','ArrowUp',38);
    await paste('ğüşöçıİ');
    const bytes=Buffer.concat([Buffer.from([3,4,26,12,11,14]),Buffer.from('\x1bOP\x1b[24~\x1b[Ağüşöçıİ')]).toString('hex');
    await browser.waitUntil(async()=>(await output(id)).includes('KEY_HEX='+bytes),{timeout:12000});
  });
  it('keeps SSH alive across Vault/SFTP tabs and shows split and update screens',async()=>{
    await $('.top-tabs').$('button=Vault').click();await $('.top-tabs').$('button=SFTP').click();await $('.sftp-page').waitForDisplayed();
    await $('.session-tab-label').click();expect(await focusedId()).toBe(id);
    await command("printf '\\033[2J\\033[H\\033[36mSSH WORKSPACE\\033[0m\\n\\n\\033[32mConnected to Linux lab\\033[0m\\nClipboard, Unicode and function keys: verified\\n\\nStats use a separate SSH channel.\\n' ");
    await $('button[title="New local terminal"]').click();await browser.waitUntil(async()=>(await $$('.top-session-tab')).length===2,{timeout:10000});
    const local=await focusedId();await browser.waitUntil(async()=>(await output(local)).includes('PS '),{timeout:45000});
    await command("Clear-Host; Write-Output 'WINDOWS LOCAL TERMINAL'; Write-Output 'Clipboard and live resource monitor ready'");
    await $('button[title="Split terminals"]').click();
    await browser.waitUntil(async()=>await browser.execute(id=>window.__metrics[id]?.memory?.total>0 && window.__metrics[id]?.cpuPercent!=null,local),{timeout:45000});
    await browser.waitUntil(async()=>(await output(local)).includes('ready\r\n'),{timeout:15000});
    await shot('split');
    await $('button.updates-nav').click();await $('h2=Application updates').waitForDisplayed();await shot('updates');
    await $('.settings-nav').$('button=General').click();await shot('appearance');
    await $('.session-tab-label').click();
    await $('button[title="Focus terminal"]').click();
  });
  it('runs sixteen real SSH panels with Stats and broadcast, then cleans up',async()=>{
    while((await $$('.top-session-tab')).length)await $('.tab-close').click();
    const workspace=record('workspace',{label:'Sixteen panel check',hostIds:Array(16).fill(host.id),layout:'split'});
    await invoke('records_save',{records:[workspace]});await refresh();
    await $('button=Workspaces').click();await $('h3=Sixteen panel check').waitForDisplayed();
    await $('button=Open').click();
    await browser.waitUntil(async()=>(await $$('.terminal-status=Connected')).length===16,{timeout:60000});
    const ids=await browser.execute(()=>[...document.querySelectorAll('.terminal-pane')].map(el=>el.dataset.sessionId));
    await browser.waitUntil(async()=>await browser.execute(ids=>ids.every(id=>window.__metrics[id]?.memory?.total>0 && window.__metrics[id]?.cpuPercent!=null),ids),{timeout:45000});
    await $('button[title="Broadcast input to all connected terminals"]').click();
    await command("printf 'BROADCAST_COMPLETE\\n'");
    await browser.waitUntil(async()=>await browser.execute(ids=>ids.every(id=>(window.__output[id]??'').includes('BROADCAST_COMPLETE\r\n')),ids),{timeout:15000});
    await $('button[title="Broadcast input to all connected terminals"]').click();
    const initial=await browser.execute(ids=>ids.map(id=>window.__output[id]),ids);
    await browser.pause(2500);
    expect(await browser.execute(ids=>ids.map(id=>window.__output[id]),ids)).toEqual(initial);
    await invoke('vault_lock');
    await browser.pause(1500);
    const stopped=await browser.execute(()=>JSON.stringify(window.__metrics));
    await browser.pause(2500);expect(await browser.execute(()=>JSON.stringify(window.__metrics))).toBe(stopped);
  });
});
