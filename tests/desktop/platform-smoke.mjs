import { browser, $, $$, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import crypto from "node:crypto";
const directory = path.join(os.tmpdir(), "termterm-smoke-" + Date.now()),
  password = "Disposable-platform-test-password";
async function invoke(command, args = {}) {
  const r = await browser.executeAsync(
    (command, args, done) =>
      window.__TAURI__.core.invoke(command, args).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) }),
      ),
    command,
    args,
  );
  if (r.error) throw Error(r.error);
  return r.value;
}
describe("Native platform smoke", () => {
  before(async () => {
    await fs.mkdir(directory, { recursive: true });
    await browser.setTimeout({ script: 120000 });
  });
  it("creates an encrypted vault and runs the platform shell with live counters", async () => {
    const info = await invoke("app_info");
    expect(info.platform.os).toBe(
      process.platform === "win32"
        ? "windows"
        : process.platform === "darwin"
          ? "macos"
          : "linux",
    );
    await $("button*=Create local vault").click();
    await $('input[placeholder="Choose where to save your vault"]').setValue(
      path.join(directory, "portable.ttvault"),
    );
    const inputs = await $$("input[type=password]");
    await inputs[0].setValue(password);
    await inputs[1].setValue(password);
    await $("button=Create vault").click();
    await $("h1=Hosts").waitForDisplayed({ timeout: 120000 });
    await browser.executeAsync((done) => {
      window.__smoke = {};
      Promise.all([
        window.__TAURI__.event.listen(
          "session-metrics",
          ({ payload: s }) => (window.__smoke.metrics = s),
        ),
        window.__TAURI__.event.listen("session-event", ({ payload: e }) => {
          window.__smoke.events = [...(window.__smoke.events ?? []), {kind:e.kind,detail:e.kind==='data'?null:e.detail}].slice(-10);
          if (e.kind === "data")
            window.__smoke.output =
              (window.__smoke.output ?? "") + atob(e.detail);
          if (e.kind === "connected") window.__smoke.id = e.id;
        }),
      ]).then(() => done(true));
    });
    // Use the real terminal so xterm answers ConPTY's cursor-position queries.
    await $('button*=Local terminal').click();
    await browser.waitUntil(
      async () => {const state=await browser.execute(() => window.__smoke); const failure=state.events?.find(e=>e.kind==='error');if(failure)throw Error(String(failure.detail));return !!state.id;},
      {timeout:60000,timeoutMsg:'Local PTY did not connect'},
    );
    const id=await browser.execute(()=>window.__smoke.id);
    await invoke("session_metrics_set", { sessionId: id, enabled: true });
    await invoke("session_input", {
      id,
      data: "echo TERMTERM_PLATFORM_SHELL_OK\r",
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(() =>
          window.__smoke.output?.includes("TERMTERM_PLATFORM_SHELL_OK"),
        ),
    );
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () =>
            window.__smoke.metrics?.cpuPercent !== null &&
            window.__smoke.metrics?.memory?.total > 0 &&
            window.__smoke.metrics?.disks?.some((d) => d.root),
        ),
      { timeout: 30000 },
    );
    const record=(kind,data)=>({id:crypto.randomUUID(),kind,data,updatedAt:Date.now()});
    const outer=record('group',{label:'Outer folder'}),inner=record('group',{label:'Inner folder',groupId:outer.id});
    await invoke('records_save',{records:[outer,inner,record('host',{label:'Root host',address:'root.invalid'}),record('host',{label:'Outer host',address:'outer.invalid',groupId:outer.id}),record('host',{label:'Inner host',address:'inner.invalid',groupId:inner.id})]});
    await browser.executeAsync(done=>window.__TAURI__.event.emit('vault-changed').then(()=>done(true)));
    await $('.main-nav button:first-child').click();
    await browser.waitUntil(async()=>(await $$('.record-card')).length===1);expect(await $('.record-card').getText()).toContain('Root host');
    await $('.group-main*=Outer folder').click();expect(await $('.record-card').getText()).toContain('Outer host');expect(await $$('.record-card')).toHaveLength(1);
    await $('.group-main*=Inner folder').click();expect(await $('.record-card').getText()).toContain('Inner host');expect(await $('.breadcrumb').getText()).toContain('Outer folder');
    await $('.breadcrumb').$('button=Hosts').click();expect(await $('.record-card').getText()).toContain('Root host');
    await $('button=Settings').click();await $('button=General').click();await browser.execute(()=>{const label=Array.from(document.querySelectorAll('label')).find(e=>e.textContent.includes('Terminal color theme'));const select=label.querySelector('select');select.value='forest';select.dispatchEvent(new Event('change',{bubbles:true}));});await $('button=Save preferences').click();
    expect((await invoke('vault_info')).records.find(r=>r.kind==='settings').data.terminalTheme).toBe('forest');
    await $('button*=Local terminal').click();await $('.terminal-cell:last-child .terminal-pane').waitForDisplayed();expect(await browser.execute(()=>getComputedStyle(document.querySelector('.terminal-cell:last-child .terminal-surface')).backgroundColor)).toBe('rgb(16, 32, 28)');
    await fs.mkdir(path.resolve('artifacts'),{recursive:true});await browser.saveScreenshot(path.resolve('artifacts/native-forest-terminal.png'));
    await invoke("vault_copy", { path: path.join(directory, "copy.ttvault") });
    await invoke("vault_lock");
    await invoke("vault_open", {
      path: path.join(directory, "copy.ttvault"),
      password,
    });
    const v = await invoke("vault_info");
    expect(v.records).toBeDefined();
    await invoke("vault_lock");
  });
});
