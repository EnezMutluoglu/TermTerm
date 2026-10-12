import {execFileSync} from 'node:child_process';
import {homedir} from 'node:os';
import path from 'node:path';
import {config as base} from './wdio.conf.mjs';
process.env.TERMTERM_LOCALE = 'tr'; // Team acceptance selectors use the default Turkish UI.
export const config={...base,
  before:async function(){
    await base.before();
    // This suite has one window. Native title differs from the HTML title;
    // explicitly select it instead of repeated title-based focus recovery.
    await browser.switchToWindow(await browser.getWindowHandle());
  },
  onPrepare:()=>{
    // The second-client writer must use the same API/version as the desktop.
    const cargo=process.platform==='win32'?path.join(homedir(),'.cargo/bin/cargo.exe'):'cargo';
    execFileSync(cargo,['build','--locked','--manifest-path','team-client/Cargo.toml','--example','native_test_writer'],{windowsHide:true,stdio:'inherit'});
  },
  specs:['./tests/desktop/team-v2.mjs'],
  afterTest:async function(test,context,{passed}) {
    // Never dump DOM/IPC arguments: temporary credentials can be visible.
    if(!passed) {
      console.error('Native Team v2 test failed; private diagnostic saved.');
      await browser.saveScreenshot(path.resolve('.lab/team-v2-http/native-failure.png'));
      console.error('UI state', await browser.execute(() => ({busy:!!document.querySelector('main[aria-busy="true"]'), dialogs:[...document.querySelectorAll('[role="dialog"]')].map(e=>e.getAttribute('aria-label')), identities:[...document.querySelectorAll('button')].filter(e=>e.textContent.trim()==='Kimliklerim').map(e=>({disabled:e.disabled,rect:{x:e.getBoundingClientRect().x,y:e.getBoundingClientRect().y}}))})));
    }
  }};
