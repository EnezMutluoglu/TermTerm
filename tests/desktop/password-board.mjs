// Real IPC, encrypted files and OS clipboard. Disposable credentials only.
import { browser, $, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import assert from "node:assert/strict";
const root = process.cwd(), password = "Disposable-board-password-040";
const work = path.join(root, ".lab", `password-board-${Date.now()}`);
async function invoke(command, args = {}) {
  const r = await browser.executeAsync((command, args, done) => window.__TAURI__.core.invoke(command, args).then(value => done({value}), error => done({error:String(error)})), command, args);
  if (r.error) throw Error(r.error); return r.value;
}
const board = (action = {kind:"list"}, context) => invoke("password_board", {action, context});
let profile, vaultPath, generated, scope;
describe("Password board", () => {
  before(async () => {
    await browser.setTimeout({script:120000});
    assert.ok((await invoke("app_info")).defaultVaultPath.includes("teamdev.e2e"));
    await fs.mkdir(work, {recursive:true});
    await invoke("vault_lock");
    vaultPath = path.join(work,"board.ttvault");
    await invoke("vault_create", {path:vaultPath,name:"Parola panosu testi",password});
    await browser.refresh();
    await $("button=Parola üretici").waitForDisplayed();
  });
  it("generates selected classes, masks, copies through OS clipboard, and reopens encrypted history", async () => {
    await $("button=Parola üretici").click();
    await $("button=Parola üret ve kaydet").waitForEnabled();
    await $('[aria-label="Karakter sayısı"]').setValue("21");
    await $("button=Parola üret ve kaydet").click();
    await $(".password-history-entry").waitForDisplayed();
    const b=await board(); scope=b.context; generated=b.entries[0].password;
    assert.equal(generated.length,21);
    assert.ok(/^[a-zA-Z0-9.,;:!?]+$/.test(generated));
    assert.ok(/[.,;:!?]/.test(generated));
    assert.equal(await $(".password-entry-value code").getText(),"••••••••••••••••");
    const previous=await invoke("plugin:clipboard-manager|read_text");
    try {
      await $('[aria-label="Parolayı kopyala"]').click();
      await browser.waitUntil(async () => (await invoke("plugin:clipboard-manager|read_text"))===generated);
    } finally { await invoke("plugin:clipboard-manager|write_text",{text:previous??""}); }
    await $('[aria-label="Parolayı göster"]').click();
    assert.equal(await $(".password-entry-value code").getText(),generated);
    await $('[aria-label="Parolayı gizle"]').click();
    await browser.saveScreenshot(path.join(root,"artifacts/password-board-040.png"));
    assert.ok(!(await fs.readFile(vaultPath)).includes(Buffer.from(generated)));
    await invoke("vault_lock");
    await assert.rejects(() => board());
    await invoke("vault_open",{path:vaultPath,password});
    assert.equal((await board()).entries[0].password,generated);
    assert.equal((await invoke("vault_info")).records.length,0);
  });
  it("keeps 50 entries, rejects invalid/stale commands, deletes and separates vaults",async () => {
    const options=(await board()).options;
    await assert.rejects(() => board({kind:"generate",options:{...options,length:0}},scope));
    await assert.rejects(() => board({kind:"generate",options},"other-vault"));
    for(let i=0;i<50;i++) await board({kind:"generate",options},scope);
    let b=await board();assert.equal(b.entries.length,50);assert.ok(!b.entries.some(e=>e.password===generated));
    await board({kind:"delete",id:b.entries[0].id},scope);assert.equal((await board()).entries.length,49);
    await invoke("vault_lock");
    await invoke("vault_create",{path:path.join(work,"other.ttvault"),name:"Other",password});
    assert.equal((await board()).entries.length,0);
    await assert.rejects(() => board({kind:"clear"},scope));
    await invoke("vault_lock");await invoke("vault_open",{path:vaultPath,password});
    await browser.refresh();await $("button=Parola üretici").waitForDisplayed();await $("button=Parola üretici").click();
    await $("button=Geçmişi temizle").waitForEnabled();await $("button=Geçmişi temizle").click();
    await $("button=Tümünü sil").click();
    await browser.waitUntil(async ()=>(await board()).entries.length===0);
    await invoke("vault_lock");
  });
  it("stores account-private Team history locally, persists across login, never creates shared records",async () => {
    profile=JSON.parse(await fs.readFile((process.env.TERMTERM_TEAM_PROFILE ?? path.join(root,".lab/team-windows.json")),"utf8"));profile.database="termterm_team_e2e";
    const name=`board_${Date.now()}`;
    const login=(name,register)=>invoke("team_auth",{profile,login:name,email:`${name}@example.invalid`,password,register});
    await login(name,true);
    let b=await board();assert.equal(b.entries.length,0);
    b=await board({kind:"generate",options:b.options},b.context);
    const secret=b.entries[0].password, teamScope=b.context;
    const team=await invoke("team_request",{action:"create_team",body:{name:"Password privacy test"}});
    const vault=await invoke("team_request",{action:"create_vault",body:{teamId:team.id,name:"Empty shared vault"}});
    await invoke("team_open_vault",{teamId:team.id,vaultId:vault.id,name:"Empty shared vault"});
    await invoke("team_request",{action:"sync",body:{}});
    assert.equal((await invoke("vault_info")).records.length,0);
    await invoke("team_logout");await login(name+"_other",true);
    assert.equal((await board()).entries.length,0);
    await assert.rejects(()=>board({kind:"clear"},teamScope));
    await invoke("team_logout");await login(name,false);
    assert.equal((await board()).entries[0].password,secret);
    await board({kind:"clear"},teamScope);await invoke("team_logout");
  });
});
