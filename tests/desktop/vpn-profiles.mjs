// Real Tauri IPC and UI. Only the separate vpndev application and disposable vault.
import { browser, $, $$, expect } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
const work = path.resolve(".lab/vpn-native-ui");
const vaultPath = path.join(work, `native-vpn-${Date.now()}.ttvault`);
const password = "Disposable-native-VPN-vault-password";
const importPath = path.resolve(".lab/vpn-desktop/desktop-1.ovpn");
let recentPath, previousRecent;
async function invoke(command, args = {}) {
  const result = await browser.executeAsync((command, args, done) =>
    window.__TAURI__.core.invoke(command, args).then(
      value => done({value}), error => done({error:String(error)})), command, args);
  if (result.error) throw Error(result.error); return result.value;
}
const field = label => $(`//label[contains(@class,'field')][span[starts-with(normalize-space(.),'${label}')]]//input`);
function metadataOnly(profile) {
  expect(Object.hasOwn(profile,"config")).toBe(false);
  expect(Object.hasOwn(profile,"password")).toBe(false);
}
async function editCard(label) {
  const cards = await $$(".vpn-card");
  for (const card of cards) if ((await card.getText()).includes(label)) { await card.$("button=Düzenle").click(); return; }
  throw Error("Missing VPN card: " + label);
}
describe("Real native VPN personal profile workflow", () => {
  before(async () => {
    await fs.mkdir(work,{recursive:true}); await browser.setTimeout({script:120000});
    const info = await invoke("app_info");
    if (!info.defaultVaultPath.includes("local.termterm.desktop.vpndev")) throw Error("Refusing non-VPN development identity");
    recentPath = path.join(path.dirname(info.defaultVaultPath),"recent.json");
    previousRecent = await fs.readFile(recentPath).catch(e => {if(e.code !== "ENOENT") throw e; return null;});
    if (await $('[title="Lock vault"]').isExisting()) await $('[title="Lock vault"]').click();
    await fs.unlink(recentPath).catch(e=>{if(e.code!=="ENOENT") throw e;});
    await browser.refresh(); await $("button*=Create local vault").waitForDisplayed({timeout:30000});
  });
  after(async () => {
    await invoke("vault_lock").catch(()=>{});
    if (recentPath) {
      if (previousRecent === null) await fs.unlink(recentPath).catch(e=>{if(e.code!=="ENOENT") throw e;});
      else await fs.writeFile(recentPath,previousRecent);
    }
  });
  it("imports, edits, locks and reopens without exposing native config or password", async () => {
    // Use a fresh filename even on repeat runs; never overwrite a user's vault.
    await fs.access(vaultPath).then(()=>{throw Error("Disposable test vault already exists; use a fresh work directory");},e=>{if(e.code!=="ENOENT") throw e;});
    await $("button*=Create local vault").click();
    await $('input[placeholder="Choose where to save your vault"]').setValue(vaultPath);
    const passwords = await $$("input[type=password]"); await passwords[0].setValue(password); await passwords[1].setValue(password);
    await $("button=Create vault").click(); await $("h1=Hosts").waitForDisplayed({timeout:120000});
    const imported = await invoke("vpn_import",{path:importPath});
    expect(imported.blocked).toBe(false); expect(imported.missingFiles).toHaveLength(0);
    expect(Object.hasOwn(imported,"config")).toBe(false); expect(typeof imported.token).toBe("string");
    const profile = await invoke("vpn_profile_save",{profile:{version:1,label:"Native imported OpenVPN",family:"openVpn",source:"disposable lab",server:"",groupId:null,username:"",realm:"",remember:false},importToken:imported.token});
    metadataOnly(profile); expect(profile.hasConfig).toBe(true);
    const fortinet = await invoke("vpn_profile_save",{profile:{version:1,label:"Native Fortinet metadata",family:"fortinet",source:"manual",server:"https://vpn.example.invalid",groupId:null,username:"fixture-user",realm:"",remember:true,password:"Disposable-profile-test-password"}});
    metadataOnly(fortinet); expect(fortinet.hasPassword).toBe(true);
    expect(await invoke("vpn_profiles")).toHaveLength(2);
    // App keeps hidden panels mounted. Recreate its view after this direct-IPC seed.
    const profileViewStarted = Date.now();
    await browser.refresh(); await $("h1=Hosts").waitForDisplayed({timeout:30000});
    await $("button=VPN · Geliştirme").click(); await browser.waitUntil(async()=>await $$(".vpn-card").then(v=>v.length===2));
    const profilesVisibleMs = Date.now() - profileViewStarted;
    await editCard("Native imported OpenVPN"); await field("Profil adı").setValue("Native imported edited");
    await $('[role="dialog"]').$("button=Kaydet").click();
    await browser.waitUntil(async()=>await $(".vpn-workspace").getText().then(v=>v.includes("Native imported edited")));
    await editCard("Native Fortinet metadata"); await field("Profil adı").setValue("Native Fortinet edited");
    await expect(field("Yeni parola (boş bırakılırsa mevcut korunur)")).toHaveValue("");
    await $('[role="dialog"]').$("button=Kaydet").click();
    await browser.waitUntil(async()=>await $(".vpn-workspace").getText().then(v=>v.includes("Native Fortinet edited")));
    const profiles = await invoke("vpn_profiles"); profiles.forEach(metadataOnly);
    expect(profiles.find(v=>v.id===profile.id).hasConfig).toBe(true); expect(profiles.find(v=>v.id===fortinet.id).hasPassword).toBe(true);
    expect((await invoke("vault_info")).records.some(v=>v.kind==="vpn")).toBe(false);
    const status = await invoke("vpn_status");
    expect(typeof status.available).toBe("boolean");
    if (status.available) expect(status.bundle?.complete).toBe(true);
    else expect(status.reason).toEqual(expect.stringMatching(/\S/));
    if (process.env.TERMTERM_VPN_EXPECT_BUNDLE === "1") {
      expect(status.bundle?.complete).toBe(true); expect(status.bundle.filesChecked).toBeGreaterThan(8);
    }
    const report = {completed:true,applicationIdentity:"local.termterm.desktop.vpndev",realNativeIpc:true,importOpaque:true,metadataOnly:true,profilesVisibleMs,uiEditsPreserveConfigAndPassword:true,windowsRuntimeAvailable:status.available,desktopBundleComplete:status.bundle?.complete ?? false,productionInstallationTouched:false};
    await fs.mkdir(path.resolve("artifacts/vpn-ui"),{recursive:true});
    await browser.saveScreenshot(path.resolve("artifacts/vpn-ui/native-profile-workflow.png"));
    await invoke("vault_lock"); await invoke("vault_open",{path:vaultPath,password});
    const reopened = await invoke("vpn_profiles"); reopened.forEach(metadataOnly);
    expect(reopened.find(v=>v.id===profile.id).label).toBe("Native imported edited");
    expect(reopened.find(v=>v.id===profile.id).hasConfig).toBe(true); expect(reopened.find(v=>v.id===fortinet.id).hasPassword).toBe(true);
    report.reopenedEncryptedProfiles = true;
    await fs.writeFile(path.resolve("artifacts/vpn-ui/native-profile-workflow.json"),JSON.stringify(report,null,2));
  });
});
