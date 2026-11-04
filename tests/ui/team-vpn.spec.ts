import {test,expect} from "@playwright/test";
import {mkdir} from "node:fs/promises";
test.beforeEach(async({page})=>{await page.goto("/tests/ui/team-vpn-harness.html");await expect(page.getByRole("heading",{name:"VPN",exact:true})).toBeVisible();});
test("Team uses normal panel and enforces separate view/connect/edit rights",async({page})=>{
  const observer=page.locator(".vpn-card").filter({hasText:"Shared view only"});
  await expect(observer.getByRole("button",{name:"Bağlan",exact:true})).toBeDisabled();
  await expect(observer.getByRole("button",{name:"Düzenle",exact:true})).toBeDisabled();
  await expect(observer.getByRole("button",{name:"Yapılandırmayı gör"})).toHaveCount(0);
  const shared=page.locator(".vpn-card").filter({hasText:"Shared connect only"});
  await expect(shared.getByRole("button",{name:"Bağlan",exact:true})).toBeEnabled();
  await expect(shared.getByRole("button",{name:"Düzenle",exact:true})).toBeDisabled();
  await expect(page.locator('input[type="password"]')).toHaveCount(0);
  await mkdir("artifacts/vpn-ui",{recursive:true});await page.screenshot({path:"artifacts/vpn-ui/team-vpn-permissions.png",fullPage:true});
});
test("sharing previews an independent copy and all five rights",async({page})=>{
  await page.locator(".vpn-card").filter({hasText:"Private office"}).getByRole("button",{name:"Bağımsız kopya paylaş"}).click();
  const dialog=page.getByRole("dialog",{name:"Ortak kimlik paylaşımı"});
  await expect(dialog.getByText(/bağımsız bir ortak kopya/)).toBeVisible();
  await dialog.getByRole("combobox").selectOption("folder");
  await dialog.getByRole("checkbox",{name:"Member view"}).check();await dialog.getByRole("checkbox",{name:"Member edit"}).check();
  await dialog.getByRole("button",{name:"Paylaşımı önizle"}).click();
  const share=await page.evaluate(()=>(window as any).fixture.share);
  expect(share.recipients).toEqual([{accountId:"member",view:true,edit:true,connect:false,reveal:false,export:false}]);
  await dialog.getByRole("button",{name:"Ortak kopyayı oluştur"}).click();
  expect(await page.evaluate(()=>(window as any).fixture.profiles[0].username)).toBe("my-account");
});
test("personal authentication is separate and disabling remember clears the password",async({page})=>{
  await page.locator(".vpn-card").filter({hasText:"Private office"}).getByRole("button",{name:"Kişisel giriş"}).click();
  const dialog=page.getByRole("dialog",{name:"Kişisel VPN girişi"});
  await dialog.getByRole("checkbox",{name:"Parolayı hatırla"}).uncheck();
  await dialog.getByRole("button",{name:"Kaydet",exact:true}).click();
  expect(await page.evaluate(()=>(window as any).fixture.authentication.authentication)).toEqual({username:"my-account",password:"",remember:false});
  await expect(page.locator('input[type="password"]')).toHaveCount(0);
});
test("account switch clears credentials and old-generation VPN events",async({page})=>{
  await page.locator(".vpn-card").filter({hasText:"Private office"}).getByRole("button",{name:"Kişisel giriş"}).click();
  await page.getByLabel("Yeni parola (boş bırakılırsa korunur)").fill("disposable-password-input");
  await page.evaluate(()=>(window as any).fixture.switchAccount());
  await expect(page.getByRole("dialog",{name:"Kişisel VPN girişi"})).toHaveCount(0);
  await expect(page.getByRole("heading",{name:"Account B Private office",exact:true})).toBeVisible();
  await page.evaluate(()=>(window as any).fixture.emit({id:"old",vaultId:"team-a:vpn:1",profileId:"private",generation:99,state:"connected"}));
  await expect(page.locator(".vpn-card").filter({hasText:"Account B Private office"}).getByRole("button",{name:"Bağlan",exact:true})).toBeVisible();
});
test("WireGuard reaches native readiness without faking completion",async({page})=>{
  const card=page.locator(".vpn-card").filter({hasText:"WireGuard office"});await card.getByRole("button",{name:"Bağlan",exact:true}).click();
  expect(await page.evaluate(()=>(window as any).fixture.calls.some((x:any)=>x.command==="vpn_connect"&&x.args.profileId==="wireguard"))).toBe(true);
  await expect(card.getByText("Bağlanıyor",{exact:true})).toBeVisible();
});
test("offline cache states its limits and disables online-only secret actions",async({page})=>{
  await page.goto("/tests/ui/team-vpn-harness.html?offline=1");
  await expect(page.getByText(/Çevrimdışı VPN önbelleği açık/)).toBeVisible();
  const card=page.locator(".vpn-card").filter({hasText:"Private office"});
  await expect(card.getByRole("button",{name:"Bağımsız kopya paylaş"})).toBeDisabled();
  await expect(card.getByRole("button",{name:"Yapılandırmayı gör"})).toBeDisabled();
  await expect(card.getByRole("button",{name:"Dışa aktar"})).toBeDisabled();
});

async function openWorkspace(page:import("@playwright/test").Page,role="operator"){
  await page.goto("/tests/ui/team-vpn-harness.html?workspace=1&role="+role);
  await page.getByRole("button",{name:"Kayıtlı hesabımla devam et"}).click();
  await expect(page.getByRole("button",{name:"Lab host host.example.test",exact:true})).toBeVisible();
}
async function openBinding(page:import("@playwright/test").Page,kind="host"){
  if(kind==="host")await page.getByRole("button",{name:"Lab host host.example.test",exact:true}).click({button:"right"});
  else await page.getByRole("button",{name:"Operations işlemleri"}).click();
  await page.getByRole("menuitem",{name:"Kişisel VPN bağlantım",exact:true}).click();
  const dialog=page.getByRole("dialog",{name:"Kişisel VPN bağlantısı",exact:true});
  await expect(dialog.getByRole("combobox",{name:"Kişisel VPN bağlantısı",exact:true})).toBeVisible();
  return dialog;
}

for(const kind of ["host","folder"]){
  test(`connect-only operator saves account-scoped ${kind} VPN without shared edits`,async({page})=>{
    await openWorkspace(page);
    const dialog=await openBinding(page,kind);
    await expect(dialog.getByRole("combobox",{name:"Ortak VPN bağlantısı"})).toHaveCount(0);
    await expect(dialog.getByRole("option",{name:"Shared view only"})).toHaveCount(0);
    await dialog.getByRole("combobox").selectOption("private");
    await dialog.getByRole("button",{name:"VPN ayarını kaydet"}).click();
    await expect.poll(()=>page.evaluate((resource)=>(window as any).fixture.bindings["account-a:"+resource+":true"]?.revision,kind)).toBe(1);
    const actions=await page.evaluate(()=>(window as any).fixture.calls.filter((c:any)=>c.command==="team_v2_action"&&["bind","binding"].includes(c.args.action.action?.kind)).map((c:any)=>c.args.action.action));
    expect(actions.every((a:any)=>a.personal===true)).toBe(true);
    expect(actions.find((a:any)=>a.kind==="bind")).toEqual({kind:"bind",resourceId:kind,expectedRevision:0,vpnId:"private",personal:true});
  });
}

test("observer has no host or folder VPN binding entry; actual edit permission enables shared binding",async({page})=>{
  await openWorkspace(page,"observer");
  await page.getByRole("button",{name:"Lab host host.example.test",exact:true}).click({button:"right"});
  await expect(page.getByRole("menuitem",{name:"Kişisel VPN bağlantım",exact:true})).toHaveCount(0);
  await page.getByRole("menuitem",{name:"Bilgiler",exact:true}).click();
  await page.getByRole("button",{name:"Operations işlemleri"}).click();
  await expect(page.getByRole("menuitem",{name:"Kişisel VPN bağlantım",exact:true})).toHaveCount(0);
  await openWorkspace(page,"editor");
  const dialog=await openBinding(page);
  await expect(dialog.getByRole("combobox",{name:"Ortak VPN bağlantısı"})).toBeVisible();
  await expect(dialog.getByRole("combobox",{name:"Ortak VPN bağlantısı"}).getByRole("option",{name:"Private office"})).toHaveCount(0);
});

test("stale personal binding reports a revision conflict and preserves the newer selection",async({page})=>{
  await openWorkspace(page);
  const dialog=await openBinding(page);
  await dialog.getByRole("combobox").selectOption("private");
  await page.evaluate(()=>(window as any).fixture.bindings["account-a:host:true"]={vpnId:"shared",revision:1});
  await dialog.getByRole("button",{name:"VPN ayarını kaydet"}).click();
  await expect(dialog.getByRole("alert")).toContainText("revision_conflict");
  expect(await page.evaluate(()=>(window as any).fixture.bindings["account-a:host:true"])).toEqual({vpnId:"shared",revision:1});
  await dialog.getByRole("button",{name:"Kapat",exact:true}).click();
  const reopened=await openBinding(page);
  await expect(reopened.getByRole("combobox")).toHaveValue("shared");
  await reopened.getByRole("combobox").selectOption("private");
  await reopened.getByRole("button",{name:"VPN ayarını kaydet"}).click();
  await expect.poll(()=>page.evaluate(()=>(window as any).fixture.bindings["account-a:host:true"])).toEqual({vpnId:"private",revision:2});
});

test("resource refresh, account change, logout and required update discard binding dialogs",async({page})=>{
  await openWorkspace(page);
  await openBinding(page);
  await page.getByRole("button",{name:"Yenile",exact:true}).evaluate((button:HTMLButtonElement)=>button.click());
  await expect(page.getByRole("dialog",{name:"Kişisel VPN bağlantısı",exact:true})).toHaveCount(0);
  await openBinding(page,"folder");
  await page.evaluate(()=>{(window as any).fixture.accountId="account-b";(window as any).fixture.scope="team-b:vpn:2";});
  await page.getByRole("button",{name:"Yenile",exact:true}).evaluate((button:HTMLButtonElement)=>button.click());
  await expect(page.getByRole("dialog",{name:"Kişisel VPN bağlantısı",exact:true})).toHaveCount(0);
  await openBinding(page);
  await page.evaluate(()=>{(window as any).fixture.required=true;window.dispatchEvent(new Event("team-update-required"));});
  await expect(page.getByRole("heading",{name:"Team için güncelleme gerekli"})).toBeVisible();
  await expect(page.getByRole("dialog",{name:"Kişisel VPN bağlantısı",exact:true})).toHaveCount(0);
  await page.evaluate(()=>{(window as any).fixture.required=false;window.dispatchEvent(new Event("team-update-required"));});
  await page.getByRole("button",{name:"Kayıtlı hesabımla devam et"}).click();
  await expect(page.getByRole("dialog",{name:"Kişisel VPN bağlantısı",exact:true})).toHaveCount(0);
  await openBinding(page);
  await page.getByRole("button",{name:"Team oturumunu kapat",exact:true}).evaluate((button:HTMLButtonElement)=>button.click());
  await expect(page.getByText("Team session closed",{exact:true})).toBeVisible();
  await expect(page.getByRole("dialog",{name:"Kişisel VPN bağlantısı",exact:true})).toHaveCount(0);
});
