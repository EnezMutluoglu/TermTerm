// Actual native child-view IPC isolation, with a disposable vault and no network authentication.
import {browser,expect} from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
const directory = path.resolve(".lab/vpn-native-sso");
let recentPath, previousRecent;
async function invoke(command,args={}) {
  const result=await browser.executeAsync((command,args,done)=>window.__TAURI__.core.invoke(command,args).then(value=>done({value}),error=>done({error:String(error)})),command,args);
  if(result.error)throw Error(result.error);
  return result.value;
}
describe("Native VPN SSO isolation",()=>{
  before(async()=>{
    await browser.setTimeout({script:120000});
    const info=await invoke("app_info");
    if(!info.defaultVaultPath.includes("local.termterm.desktop.vpndev"))throw Error("Refusing non-isolated application");
    recentPath=path.join(path.dirname(info.defaultVaultPath),"recent.json");
    previousRecent=await fs.readFile(recentPath).catch(e=>{if(e.code!=="ENOENT")throw e;return null;});
    await fs.mkdir(directory,{recursive:true});
    await invoke("vault_create",{path:path.join(directory,`isolation-${Date.now()}.ttvault`),name:"Disposable SSO isolation",password:"Disposable-SSO-fixture-password"});
  });
  after(async()=>{
    await invoke("vault_lock").catch(()=>{});
    if(recentPath){
      if(previousRecent===null)await fs.unlink(recentPath).catch(e=>{if(e.code!=="ENOENT")throw e;});
      else await fs.writeFile(recentPath,previousRecent);
    }
  });
  it("denies actual application and core commands from the auth child, then closes it",async()=>{
    const before=await invoke("vault_info");
    const report=await invoke("vpn_sso_test_isolation",{bounds:{x:100,y:120,width:800,height:440}});
    expect(report.checks).toEqual(["vault_info","plugin:window|inner_size"].map(command=>({command,denied:true,error:expect.stringMatching(/^(?:vault_info|window\.inner_size) not allowed(?:[. ]|$)|^VPN authentication views cannot use application IPC$/)})));
    expect(report.viewClosed).toBe(true);
    expect(report.productionWebMessagesDisabled).toBe(true);
    expect(report.fixtureTransport).toBe("tauriPostMessage");
    expect((await invoke("vault_info")).id).toBe(before.id);
    expect(await invoke("vpn_sessions")).toHaveLength(0);
    const status=await invoke("vpn_status");
    expect(typeof status.available).toBe("boolean");
    if (status.available) expect(status.bundle?.complete).toBe(true);
    else expect(status.reason).toEqual(expect.stringMatching(/\S/));
    if (process.env.TERMTERM_VPN_EXPECT_BUNDLE === "1") {
      expect(status.bundle?.complete).toBe(true); expect(status.bundle.filesChecked).toBeGreaterThan(8);
    }
    expect(status.authTestAvailable).toBe(true);
    await fs.mkdir(path.resolve("artifacts/vpn-ui"),{recursive:true});
    await fs.writeFile(path.resolve("artifacts/vpn-ui/native-sso-isolation.json"),JSON.stringify({completed:true,realNativeChild:true,networkAuthentication:false,applicationIdentity:"local.termterm.desktop.vpndev",checks:report.checks,productionWebMessagesDisabled:report.productionWebMessagesDisabled,fixtureTransport:report.fixtureTransport,productionTransportCommandsTested:false,disposableVaultPreserved:true,windowsRuntimeAvailable:status.available,remoteHttpsHeaderDeliveryTested:false},null,2));
  });
});
