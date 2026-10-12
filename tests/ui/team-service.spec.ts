import { test, expect, Page } from "@playwright/test";

test("Team owner confirms a version once and sees the persisted decision",async({page})=>{
 await signIn(page);await page.locator('.main-nav').getByRole('button',{name:'Team',exact:true}).click();
 await page.getByRole('button',{name:'Sürüm politikası',exact:true}).click();
 const panel=page.getByRole('region',{name:'Team sürüm politikası'});
 await panel.getByLabel('Zorunlu istemci sürümü').selectOption('0.4.2');
 await panel.getByRole('button',{name:'Etkiyi önizle'}).click();
 await panel.getByRole('button',{name:'Onayla ve tüm takıma bildir'}).click();
 await expect(panel.getByRole('status')).toContainText('Sürüm politikası kaydedildi');
 expect(await page.evaluate(()=>(window as any).policy)).toMatchObject({revision:1,selectedVersion:'0.4.2'});
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.kind==='setClientPolicy'))).toHaveLength(1);
});

test("leader previews a selected Team release and must confirm before notifying",async({page})=>{
  await signIn(page);
  await page.locator('.main-nav').getByRole('button',{name:'Team',exact:true}).click();
  await page.getByRole('button',{name:'Sürüm politikası',exact:true}).click();
  const panel=page.getByRole('region',{name:'Team sürüm politikası'});
  await panel.getByLabel('Zorunlu istemci sürümü').selectOption('0.4.2');
  await panel.getByRole('button',{name:'Etkiyi önizle'}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.kind==='setClientPolicy'))).toHaveLength(0);
  await expect(panel.getByRole('region',{name:'Sürüm zorunluluğu önizlemesi'})).toContainText('0.4.2');
  await page.screenshot({path:'artifacts/team-v2/team-update-policy-browser.png'});
  await page.evaluate(()=>{(window as any).policyConflict=true;});
  await panel.getByRole('button',{name:'Onayla ve tüm takıma bildir'}).click();
  await expect(panel.getByRole('alert')).toContainText('Başka bir yönetici değiştirdi');
  expect(await page.evaluate(()=>(window as any).policy)).toBeUndefined();
});

test("mandatory update is shown before Team login and cannot be skipped", async ({page}) => {
  await page.addInitScript(()=>{(window as any).requiredPolicy={required:true,minimumClientVersion:"0.4.2",apiVersion:1};});
  await page.goto("/tests/ui/team-service-harness.html");
  await page.getByLabel("Servis adresi").fill("https://team.example.invalid");
  await page.getByLabel("Takım kimliği").fill("00000000-0000-0000-0000-000000000001");
  const gate=page.getByRole("region",{name:"Zorunlu Team güncellemesi"});
  await expect(gate).toContainText("0.4.2");
  await expect(page.getByRole("button",{name:"Giriş yap",exact:true})).toHaveCount(0);
  await expect(gate).toContainText("gönderilmemiş değişikliklerin korunur");
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==="team_v2_login"||c.command==="update_install"))).toHaveLength(0);
  await page.screenshot({path:"artifacts/team-v2/mandatory-update-browser.png"});
});

test("running Team closes its workspace on policy change but preserves journal data", async ({page}) => {
  await signIn(page);
  await page.evaluate(()=>{
    (window as any).journal={pending:2,conflicts:0,quarantined:0,editRemainingMs:3600};
    (window as any).requiredPolicy={required:true,minimumClientVersion:"0.4.2",apiVersion:1};
    window.dispatchEvent(new Event("focus"));
  });
  await expect(page.getByRole("heading",{name:"Team için güncelleme gerekli"})).toBeVisible();
  await expect(page.locator(".ts-workspace")).toHaveCount(0);
  expect(await page.evaluate(()=>(window as any).journal.pending)).toBe(2);
  expect(await page.evaluate(()=>(window as any).calls.some((c:any)=>c.command==="native-policy-logout"))).toBe(true);
});

test("Team SFTP retains folders while excluding hosts without connect permission",async({page})=>{
 await page.addInitScript(()=>{(window as any).sftpFixture=true;});
 await signIn(page);await page.locator('.main-nav').getByRole('button',{name:'SFTP',exact:true}).click();
 await page.locator('[data-file-pane="1"] .sftp-host-trigger').click();
 const d=page.getByRole('dialog');await d.getByRole('button',{name:'Üretim',exact:true}).click();
 await expect(d.getByRole('button',{name:'Connect to İzinli SFTP'})).toBeVisible();
 await expect(d).not.toContainText('Laboratuvar');
 await d.getByRole('textbox',{name:'Search host names'}).fill('Laboratuvar');
 await expect(d.getByRole('button',{name:'Connect to Laboratuvar'})).toHaveCount(0);
});

test("resource move previews access changes and stages keys before applying",async({page})=>{
 await signIn(page);await page.locator('.group-main').filter({hasText:'Üretim'}).click();
 await page.locator('.record-card').click({button:'right'});await page.getByRole('menuitem',{name:'Taşı',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'Kaynağı taşı'});
 await dialog.getByRole('button',{name:'Taşıma ve erişim önizlemesi'}).click();
 await expect(dialog).toContainText('Erişim yok');await expect(dialog.getByRole('button',{name:'Onayla ve taşı'})).toBeDisabled();
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.kind==='finishMove').length)).toBe(0);
 await dialog.getByRole('button',{name:'Sonraki anahtarları hazırla'}).click();
 await page.screenshot({path:'artifacts/team-v2/move-preview.png'});
 await page.evaluate(()=>((window as any).moveFailure=true));await dialog.getByRole('button',{name:'Onayla ve taşı'}).click();
 await expect(dialog.getByRole('alert')).toContainText('preview_changed');
 await dialog.getByRole('button',{name:'Vazgeç'}).click();await expect(dialog).toHaveCount(0);
});

test("personal username archive requires preview and refuses stale restore",async({page})=>{
  await signIn(page);
  await page.getByRole('button',{name:'Kimliklerim',exact:true}).click();
  await page.getByRole('button',{name:'Varsayılan bağlantı profilim'}).click();
  await page.getByRole('button',{name:'Önceki kullanıcı adı sürümleri'}).click();
  await expect(page.getByRole('button',{name:'Sürüm 0 · Bu kimlikle açılamıyor'})).toBeDisabled();
  await page.getByRole('button',{name:'Sürüm 1 · Önizle'}).click();
  await expect(page.getByRole('region',{name:'Profil geri yükleme önizlemesi'})).toContainText('onceki_kullanici');
  expect(await page.evaluate(()=> (window as any).calls.filter((c:any)=>c.kind==='restoreProfile').length)).toBe(0);
  await page.evaluate(()=>((window as any).profileConflict=true));
  await page.getByRole('button',{name:'Yeni sürüm olarak geri yükle'}).click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(1);
  await page.evaluate(()=>((window as any).profileConflict=false));
  await page.getByRole('button',{name:'Yeni sürüm olarak geri yükle'}).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});
test("unreadable legacy profile needs explicit replacement consent", async ({
  page,
}) => {
  await signIn(page);
  await page.evaluate(() => ((window as any).brokenDefault = true));
  await page.getByRole("button", { name: "Kimliklerim", exact: true }).click();
  await page
    .getByRole("button", { name: "Varsayılan bağlantı profilim" })
    .click();
  await expect(
    page.getByRole("button", { name: "Profili kaydet" }),
  ).toBeDisabled();
  await page
    .getByLabel(
      "Kullanıcı adını yeniden girerek profilimi güncellemek istiyorum",
    )
    .check();
  await page.getByLabel("Profil kullanıcı adı").fill("new_personal_user");
  await page.getByRole("button", { name: "Profili kaydet" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "saveProfile")
          .length,
    ),
  ).toBe(1);
});
test("recipient recovery requires explicit verification and fails closed on bad ciphertext", async ({
  page,
}) => {
  await signIn(page);
  await page.getByRole("button", { name: "Kimliklerim", exact: true }).click();
  await page
    .getByRole("button", { name: "Kurtarmayı doğrula", exact: true })
    .click();
  await page.getByRole("button", { name: "Bu kurtarmayı doğrula" }).click();
  await expect(page.getByRole("dialog")).toContainText("Recipient");
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "recoveryStep")
          .length,
    ),
  ).toBe(0);
  await page.evaluate(() => ((window as any).recoveryFailure = true));
  await page.getByRole("button", { name: "Açılabildiğini doğrula" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Recovery ciphertext invalid",
  );
  await expect(page.getByRole("status")).not.toContainText("tamamlandı");
  await page.evaluate(() => ((window as any).recoveryFailure = false));
  await page.getByRole("button", { name: "Açılabildiğini doğrula" }).click();
  await expect(page.getByRole("dialog").getByRole("status")).toContainText(
    "Kurtarma doğrulandı ve tamamlandı.",
  );
  await page.getByRole("button", { name: "Kapat", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
});
test("audit retention failures stay visible and retry reports actual counts", async ({
  page,
}) => {
  await signIn(page);
  await page
    .locator(".main-nav")
    .getByRole("button", { name: "Team", exact: true })
    .click();
  await page
    .getByRole("button", { name: "İşlem geçmişi", exact: true })
    .click();
  await expect(
    page.getByRole("cell", { name: "Ortak kimlik oluşturuldu" }),
  ).toBeVisible();
  await page.evaluate(() => ((window as any).maintenanceFailure = true));
  await page
    .getByRole("button", { name: "Saklama süresi dolanları temizle" })
    .click();
  await expect(page.getByRole("alert")).toContainText(
    "Maintenance unavailable",
  );
  await page.evaluate(() => ((window as any).maintenanceFailure = false));
  await page
    .getByRole("button", { name: "Saklama süresi dolanları temizle" })
    .click();
  await expect(page.getByRole("status")).toContainText(
    "8 işlem kaydı, 2 süresi dolmuş",
  );
});
test("sharing requires an explicit recipient preview and preserves the private identity", async ({
  page,
}) => {
  await signIn(page);
  await page.getByRole("button", { name: "Kimliklerim", exact: true }).click();
  await page.locator('[data-identity-id="private-key"]').click({button:"right"});
  await page.getByRole("menuitem", {name:"Ortak kimlik olarak paylaş"}).click();
  await page.getByLabel("Paylaşım kapsamı").selectOption("folder");
  await page.getByLabel("Member connect").check();
  await page.getByRole("button", { name: "Paylaşımı önizle" }).click();
  await expect(
    page.getByRole("cell", { name: "Owner", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "finishShare")
          .length,
    ),
  ).toBe(0);
  await page.getByRole("button", { name: "Vazgeç", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "cancelShare")
          .length,
    ),
  ).toBe(1);
  await page.locator('[data-identity-id="private-key"]').click({button:"right"});
  await page.getByRole("menuitem", {name:"Ortak kimlik olarak paylaş"}).click();
  await page.getByLabel("Paylaşım kapsamı").selectOption("folder");
  await page.getByLabel("Member connect").check();
  await page.getByRole("button", { name: "Paylaşımı önizle" }).click();
  await page.getByRole("button", { name: "Ortak kopyayı oluştur" }).click();
  await expect(
    page.getByText("Private test key", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Shared test key", { exact: true }),
  ).toBeVisible();
  await page.locator('[data-identity-id="shared-key"]').click({button:"right"});
  const shared = page.getByRole("menu");
  await expect(
    shared.getByRole("menuitem", { name: "Varsayılan yap" }),
  ).toBeEnabled();
  await expect(
    shared.getByRole("menuitem", { name: "Sırrı göster" }),
  ).toHaveCount(0);
  await expect(shared.getByRole("menuitem", { name: "Dışa aktar" })).toHaveCount(
    0,
  );
});
test("shared identity updates require preview, show stale errors and allow cancellation", async ({
  page,
}) => {
  await signIn(page);
  await page.evaluate(() => {
    (window as any).shared = true;
    (window as any).canUpdate = true;
  });
  await page.getByRole("button", { name: "Kimliklerim", exact: true }).click();
  await page.locator('[data-identity-id="shared-key"]').click({button:"right"});
  await page.getByRole("menuitem", { name: "Alıcıları / kimliği güncelle" }).click();
  await expect(page.getByLabel("Member connect")).toBeChecked();
  await page.getByLabel("Yeni sürümün içeriği").selectOption("private-key");
  await page.getByLabel("Member reveal").check();
  await page.getByRole("button", { name: "Paylaşımı önizle" }).click();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter(
          (c: any) => c.kind === "finishSharedUpdate",
        ).length,
    ),
  ).toBe(0);
  await page.evaluate(() => {
    (window as any).updateFailure = true;
  });
  await page.getByRole("button", { name: "Yeni sürümü kaydet" }).click();
  await expect(page.getByRole("alert")).toContainText("preview_changed");
  await page.getByRole("button", { name: "Vazgeç", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter(
          (c: any) => c.kind === "cancelSharedUpdate",
        ).length,
    ),
  ).toBe(1);
  await expect(
    page.getByText("Private test key", { exact: true }),
  ).toBeVisible();
});
test.setTimeout(60000);
test("leader pin cleanup needs explicit preview and confirmation", async ({
  page,
}) => {
  await signIn(page);
  await page
    .locator(".main-nav")
    .getByRole("button", { name: "Team", exact: true })
    .click();
  await page
    .getByRole("button", { name: "İşlem geçmişi", exact: true })
    .click();
  await page.getByRole("button", { name: "Korunan sürümleri yönet" }).click();
  await page
    .getByRole("checkbox", { name: "Old member Laboratuvar sürüm 1" })
    .check();
  await page.getByRole("button", { name: "Kaldırmayı önizle" }).click();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "finishPinRelease")
          .length,
    ),
  ).toBe(0);
  await page.getByRole("button", { name: "Korumaları kaldır" }).click();
  await expect(page.getByRole("status")).toContainText(
    "1 koruma kaldırıldı, 1 eski sürüm temizlendi",
  );
  await expect(page.getByRole("checkbox")).toHaveCount(0);
  await page.getByRole("button", { name: "Kapat", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
});
async function signIn(page: Page) {
  await page.goto("/tests/ui/team-service-harness.html");
  await page.getByLabel("Servis adresi").fill("https://team.example.invalid");
  await page
    .getByLabel("Takım kimliği")
    .fill("00000000-0000-0000-0000-000000000001");
  await page.getByLabel("Kullanıcı adı", { exact: true }).fill("leader");
  await page
    .getByLabel("Parola", { exact: true })
    .fill("Disposable-Only-Password-123!");
  await page.getByRole("button", { name: "Giriş yap", exact: true }).click();
  await expect(page.locator(".ts-workspace")).toBeVisible();
}
test("trusted right-click and select events open a permission preview without granting access", async ({
  page,
}) => {
  await signIn(page);
  await expect(
    page.locator(".main-nav").getByRole("button", { name: /Hosts/ }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: /Laboratuvar/ })).toHaveCount(
    0,
  );
  await page
    .locator(".group-main")
    .filter({ hasText: "Üretim" })
    .click({ button: "right" });
  await page.getByRole("menuitem", { name: "Üye izinleri" }).click();
  await page
    .getByRole("combobox", { name: "Üye", exact: true })
    .selectOption("user");
  await page
    .getByRole("combobox", { name: "Düzenle", exact: true })
    .selectOption("allow");
  await page.getByRole("button", { name: "Değişikliği önizle" }).click();
  await expect(
    page.getByRole("cell", { name: "Laboratuvar", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "finishAccess")
          .length,
    ),
  ).toBe(0);
  await page.getByRole("button", { name: "İptal", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.locator(".group-main").filter({ hasText: "Üretim" }).click();
  await expect(page.getByRole("button", { name: /Laboratuvar/ })).toBeVisible();
});
test("busy creation cannot duplicate a request and logout suppresses its late result", async ({
  page,
}) => {
  await signIn(page);
  await page
    .locator(".main-nav")
    .getByRole("button", { name: "Team", exact: true })
    .click();
  await page.getByRole("button", { name: "Üyeler ve yetkiler" }).click();
  await page
    .getByRole("button", { name: "Kullanıcı oluştur", exact: true })
    .click();
  await page.getByLabel("Görünen ad").fill("Geçici üye");
  await page.getByLabel("Kullanıcı adı", { exact: true }).fill("new.member");
  await page.evaluate(() => {
    (window as any).hold = "createMember";
  });
  await page.getByRole("button", { name: "Kaydet", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Kaydet", exact: true }),
  ).toBeDisabled();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "createMember")
          .length,
    ),
  ).toBe(1);
  // Running dialogs expose logout without interacting through the modal overlay.
  await page
    .getByRole("button", {
      name: "İşlemi beklemeden oturumu kapat",
      exact: true,
    })
    .click();
  await page.evaluate(() => (window as any).release());
  await expect(page.locator("#root")).toHaveText("Oturum kapandı");
  await expect(page.getByRole("dialog", { name: "Geçici parola" })).toHaveCount(
    0,
  );
});

test("delegation requires an explicit preview and a separate apply step", async ({
  page,
}) => {
  await signIn(page);
  await page
    .locator(".group-main")
    .filter({ hasText: "Üretim" })
    .click({ button: "right" });
  await page
    .getByRole("menuitem", { name: "Yetki devri", exact: true })
    .click();
  await page
    .getByLabel("Kapsam yöneticisi", { exact: true })
    .selectOption("manager");
  await page.getByLabel("Görüntüleme izni verebilir").check();
  await page
    .getByRole("button", { name: "Değişikliği önizle", exact: true })
    .click();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "setDelegation")
          .length,
    ),
  ).toBe(0);
  await expect(page.getByRole("dialog", { name: "Yetki devri" })).toContainText(
    "Yetki devri yok",
  );
  await page.getByRole("button", { name: "Yetki devrini uygula" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.kind === "setDelegation")
          .length,
    ),
  ).toBe(1);
});

test("journal keeps pending counts on sync failure and clears only after receipt",async({page})=>{
  await page.clock.install();
  await signIn(page);
  await page.evaluate(()=>{(window as any).journal={pending:2,conflicts:0,quarantined:0,oldestPendingMs:1791302400000,editRemainingMs:3600000};(window as any).failSync=true;});
  await page.clock.fastForward(15000);
  await expect(page.locator(".ts-journal")).toContainText("2 bekleyen");
  await page.getByRole("button",{name:"Şimdi eşitle"}).click();
  await expect(page.locator(".ts-error")).toContainText("network_unavailable");
  await expect(page.locator(".ts-journal")).toContainText("2 bekleyen");
  await page.screenshot({path:"artifacts/team-v2/journal-pending.png"});
  await page.evaluate(()=>{(window as any).failSync=false;});
  await page.getByRole("button",{name:"Şimdi eşitle"}).click();
  await expect(page.locator(".ts-journal")).toHaveCount(0);
});

test("Team uses the normal host workspace and keeps administration behind Team navigation", async ({
  page,
}) => {
  await signIn(page);
  await expect(page.locator(".topbar .brand")).toHaveText("TermTerm");
  await page.keyboard.press("Control+Tab");
  await expect(page.locator(".top-tabs button.active")).toHaveText("SFTP");
  await page.keyboard.press("Control+Shift+Tab");
  await expect(page.locator(".top-tabs button.active")).toHaveText("Kasa");
  await expect(
    page
      .locator(".top-tabs")
      .getByRole("button", { name: "Kasa", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Kullanıcı oluştur", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.locator(".group-main").filter({ hasText: "Üretim" }),
  ).toContainText("1 erişilebilir host");
  await expect(page.locator(".record-card")).toHaveCount(0);
  await page.screenshot({
    path: "artifacts/team-v2/unified-hosts-folders.png",
  });
  await page.locator(".group-main").filter({ hasText: "Üretim" }).click();
  await expect(page.locator(".ts-details")).toHaveCount(0);
  await expect(page.locator(".record-card")).toHaveCount(1);
  await page
    .getByLabel("Bu klasörde ara", { exact: true })
    .fill("does-not-exist");
  await expect(page.locator(".record-card")).toHaveCount(0);
  await page.getByLabel("Bu klasörde ara", { exact: true }).fill("Laboratuvar");
  await page.locator(".record-card").click();
  await expect(page.locator(".ts-details")).toContainText("127.0.0.1");
  await page.getByRole("button", { name: "Bilgileri kapat" }).click();
  await page.screenshot({ path: "artifacts/team-v2/unified-hosts-inside.png" });
  await page
    .locator(".main-nav")
    .getByRole("button", { name: "Team", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Kullanıcı oluştur", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".record-card")).toHaveCount(0);
  await page.screenshot({
    path: "artifacts/team-v2/unified-team-administration.png",
  });
  await page
    .locator(".main-nav")
    .getByRole("button", { name: /Hosts/ })
    .click();
  await expect(page.locator(".group-card")).toBeVisible();
});


test("chain uses explicit preview and keeps failed changes reviewable",async({page})=>{
  await signIn(page);
  await page.evaluate(()=>{(window as any).chainFixture=true;});
  await page.getByRole("button",{name:"Yenile",exact:true}).click();
  await page.locator(".group-main").filter({hasText:"Üretim"}).click();
  await page.locator(".record-card").click({button:"right"});
  await page.getByRole("menuitem",{name:"Host chain",exact:true}).click();
  const dialog=page.getByRole("dialog",{name:"Host chain",exact:true});
  await dialog.getByLabel("Jump host",{exact:true}).selectOption("jump");
  await dialog.getByRole("button",{name:"Sıraya ekle"}).click();
  await dialog.getByRole("button",{name:"Erişim önizlemesi"}).click();
  await expect(dialog).toContainText("1 jump host · 2 yetkili kullanıcı");
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.kind==="finishChain").length)).toBe(0);
  await page.screenshot({path:"artifacts/team-v2/chain-preview.png"});
  await page.evaluate(()=>{(window as any).chainFailure=true;});
  await dialog.getByRole("button",{name:"Onayla ve kaydet"}).click();
  await expect(dialog.getByRole("alert")).toContainText("revision_conflict");
  await dialog.getByRole("button",{name:"İptal",exact:true}).click();
  await expect(dialog).toHaveCount(0);
});

test("offline workspace explains cached access and does not fetch leader management",async({page})=>{
  await page.addInitScript(()=>{(window as any).offlineFixture=true;});
  await signIn(page);
  await expect(page.getByText(/Çevrimdışı önbellek açık/)).toBeVisible();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>["members","delegations"].includes(c.kind)).length)).toBe(0);
  await expect(page.locator(".group-card")).toBeVisible();
  await page.locator(".group-main").filter({hasText:"Üretim"}).click();
  await expect(page.locator(".record-card")).toBeVisible();
  await page.screenshot({path:"artifacts/team-v2/offline-ui.png"});
  await page.evaluate(()=>{(window as any).offlineFixture=false;});
  await page.getByRole("button",{name:"Bağlantıyı yeniden doğrula",exact:true}).click();
  await expect(page.getByText(/Çevrimdışı önbellek açık/)).toHaveCount(0);
});

test("offline creation candidate stays separate until an explicit reviewed decision",async({page})=>{
  await page.addInitScript(()=>{(window as any).candidateFixture=true;});
  await signIn(page);
  await page.locator(".main-nav").getByRole("button",{name:"Team",exact:true}).click();
  await page.getByRole("button",{name:"Bekleyen kararlar",exact:true}).click();
  await page.getByRole("button",{name:/Çevrimdışı yeni makine/}).click();
  const dialog=page.getByRole("dialog",{name:"Çakışmayı karşılaştır"});
  await expect(dialog).toContainText("Onaylanana kadar ortak Hosts listesine eklenmez");
  expect(await page.evaluate(()=>(window as any).decisionApplied)).toBeUndefined();
  await page.screenshot({path:"artifacts/team-v2/creation-decision.png"});
  await page.evaluate(()=>{(window as any).decisionFailure=true;});
  await dialog.getByRole("button",{name:"Yeni kaydı onayla"}).click();
  await expect(dialog.getByRole("alert")).toContainText("revision_conflict");
  expect(await page.evaluate(()=>(window as any).decisionApplied)).toBeUndefined();
  await page.evaluate(()=>{(window as any).decisionFailure=false;});
  await dialog.getByRole("button",{name:"Yeni kaydı reddet"}).click();
  await expect(dialog).toHaveCount(0);
  expect(await page.evaluate(()=>(window as any).decisionChoice)).toBe("reject");
});

test("disabled account draft handoff never enters the workspace",async({page})=>{
  await page.goto("/tests/ui/team-service-harness.html");
  await page.getByLabel("Servis adresi").fill("https://team.example.invalid");
  await page.getByLabel("Takım kimliği").fill("00000000-0000-0000-0000-000000000001");
  await page.getByLabel("Kullanıcı adı",{exact:true}).fill("disabled-user");
  await page.getByText("Hesabım kapatıldı; bekleyen taslaklarım var",{exact:true}).click();
  const submit=page.getByRole("button",{name:"Bekleyen taslakları ilet",exact:true});
  await expect(submit).toBeDisabled();
  await page.getByLabel("Parola",{exact:true}).fill("Disposable-Cache-Password-123!");
  await submit.click();
  await expect(page.getByRole("status")).toContainText("1 işlem teslim edildi");
  await expect(page.getByRole("status")).toContainText("Oturum açılmadı");
  await expect(page.locator(".ts-workspace")).toHaveCount(0);
  await expect(page.getByLabel("Parola",{exact:true})).toHaveValue("");
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==="team_v2_login").length)).toBe(0);
  await page.evaluate(()=>{(window as any).deliveryFailure=true;});
  await page.getByLabel("Parola",{exact:true}).fill("Disposable-Cache-Password-123!");
  await submit.click();
  await expect(page.getByRole("alert")).toBeVisible();
  await expect(page.locator(".ts-workspace")).toHaveCount(0);
});


test("agent selection requires a specific public fingerprint and shows unavailable agent", async ({page}) => {
  await signIn(page);
  await page.getByRole("button",{name:"Kimliklerim",exact:true}).click();
  await page.getByRole("button",{name:"Özel kimlik ekle",exact:true}).click();
  const dialog=page.getByRole("dialog");
  await dialog.getByLabel("Yöntem",{exact:true}).selectOption("agent");
  await dialog.getByRole("button",{name:"Agent anahtarlarını listele"}).click();
  await expect(dialog.getByLabel("İmzalama anahtarı",{exact:true}).locator("option")).toHaveCount(3);
  await expect(dialog.getByLabel("İmzalama anahtarı",{exact:true})).toHaveValue("");
  await dialog.getByLabel("İmzalama anahtarı",{exact:true}).selectOption("SHA256:"+"A".repeat(43));
  await expect(dialog.getByRole("option",{name:/Sertifika/})).toHaveCount(1);
  await dialog.getByLabel("İmzalama anahtarı",{exact:true}).selectOption("CERT-SHA256:"+"B".repeat(43));
  await expect(dialog.getByLabel("İmzalama anahtarı",{exact:true})).toHaveValue("CERT-SHA256:"+"B".repeat(43));
  await page.evaluate(()=>{(window as any).agentFailure=true;});
  await dialog.getByRole("button",{name:"Agent anahtarlarını listele"}).click();
  await expect(dialog.getByRole("alert")).toContainText("unavailable");
  await expect(dialog.getByLabel("İmzalama anahtarı",{exact:true})).toHaveValue("");
});


test("rotation never retires the old key before verified staging",async({page})=>{
  await signIn(page);
  await page.locator(".main-nav").getByRole("button",{name:"Team",exact:true}).click();
  await page.getByRole("button",{name:"Kurtarma anahtarını değiştir",exact:true}).click();
  const d=page.getByRole("dialog",{name:"Kurtarma anahtarını değiştir"});
  await d.getByLabel("Eski kurtarma dosyası",{exact:true}).fill("C:/test/old.ttrecovery");
  await d.getByLabel("Yeni kurtarma dosyası",{exact:true}).fill("C:/test/new.ttrecovery");
  await d.getByLabel("Eski dosyanın parolası",{exact:true}).fill("Disposable-Old-123!");
  await d.getByLabel("Yeni dosyanın parolası",{exact:true}).fill("Disposable-New-123!");
  await d.getByLabel("Team hesap parolan",{exact:true}).fill("Disposable-Team-123!");
  await d.getByLabel("Değişim gerekçesi",{exact:true}).fill("Şirket anahtarı değişim testi");
  await d.getByRole("button",{name:"Dosyaları doğrula ve aç"}).click();
  const finish=d.getByRole("button",{name:"Eski anahtarı emekliye ayır ve bitir"});
  await expect(finish).toBeDisabled();
  await page.evaluate(()=>{(window as any).rotationFailure=true;});
  await d.getByRole("button",{name:"Sonraki parçayı doğrula"}).click();
  await expect(d.getByRole("alert")).toContainText("invalid");await expect(finish).toBeDisabled();
  await page.evaluate(()=>{(window as any).rotationFailure=false;});
  await d.getByRole("button",{name:"Sonraki parçayı doğrula"}).click();
  await expect(finish).toBeEnabled();
  expect(await page.evaluate(()=>(window as any).calls.filter((x:any)=>x.kind==="rotationFinish").length)).toBe(0);
  await page.screenshot({path:"artifacts/team-v2/rotation-preview.png"});
  await finish.click();await expect(d.getByRole("status")).toContainText("etkinleştirildi");
});


test("shared recovery previews current recipients and preserves a stale decision",async({page})=>{
  await page.addInitScript(()=>{(window as any).shared=true;});
  await signIn(page);
  await page.getByRole("button",{name:"Kimliklerim",exact:true}).click();
  await page.locator(".main-nav").getByRole("button",{name:"Team",exact:true}).click();
  await page.getByRole("button",{name:"Ortak kimliği kurtar",exact:true}).click();
  await page.getByRole("dialog",{name:"Kurtarılacak ortak kimlik"}).getByRole("button",{name:"Shared test key",exact:true}).click();
  const d=page.getByRole("dialog",{name:"Ortak kimliği kurtar"});
  await d.getByLabel("Şirket kurtarma dosyası",{exact:true}).fill("C:/test/company.ttrecovery");
  await d.getByLabel("Dosya parolası",{exact:true}).fill("Disposable-File-123!");
  await d.getByLabel("Team hesap parolan",{exact:true}).fill("Disposable-Team-123!");
  await d.getByLabel("Kurtarma gerekçesi",{exact:true}).fill("Şirket ortak anahtarını kurtarma testi");
  await d.getByRole("button",{name:"Dosyayı aç ve alıcıları önizle"}).click();
  await expect(d.getByRole("table")).toContainText("Operatör");
  await expect(d.getByRole("row").filter({hasText:"Operatör"})).toContainText("Hayır");
  expect(await page.evaluate(()=>(window as any).calls.filter((x:any)=>x.kind==="sharedRecoveryFinish").length)).toBe(0);
  await page.evaluate(()=>{(window as any).sharedRecoveryFailure=true;});
  await d.getByRole("button",{name:"Yeni revizyonla kurtarmayı uygula"}).click();
  await expect(d.getByRole("alert")).toContainText("preview_changed");
  await expect(d.getByRole("table")).toBeVisible();
  await d.getByRole("button",{name:"İptal",exact:true}).click();
  await expect(d).toHaveCount(0);
});


test("Team documents save typed workspace preferences and preserve failed drafts",async({page})=>{
 await page.addInitScript(()=>{(window as any).documentFixture=true;});await signIn(page);
 await page.getByRole('button',{name:'Komutlar / Workspaces',exact:true}).click();
 await page.getByRole('button',{name:'Çalışma alanı',exact:true}).click();
 const d=page.getByRole('dialog',{name:'Team belge düzenle'});
 await d.getByLabel('Ad',{exact:true}).fill('İstanbul gözlem');await expect(d.getByRole('button',{name:'Kaydet',exact:true})).toBeDisabled();
 await d.getByLabel('Çalışma alanına host ekle').selectOption('allowed');await d.getByLabel('Çalışma alanı görünümü').selectOption('grid');
 await d.getByLabel('Başlangıç komutu').fill("printf 'İstanbul'\n");await d.getByLabel('Kaynak göstergesi').uncheck();
 await page.evaluate(()=>((window as any).documentFailure=true));await d.getByRole('button',{name:'Kaydet',exact:true}).click();await expect(d.getByRole('alert')).toContainText('permission_denied');
 await expect(d.getByLabel('Başlangıç komutu')).toHaveValue("printf 'İstanbul'\n");
 await page.evaluate(()=>((window as any).documentFailure=false));await d.getByRole('button',{name:'Kaydet',exact:true}).click();await expect(d).toHaveCount(0);
 const saved=await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.kind==='saveResource').map((c:any)=>c.action));
 expect(saved[0].id).toBe(saved[1].id);expect(saved[1].content.document).toEqual({type:'workspace',layout:'grid',entries:[{hostId:'allowed',startupCommand:"printf 'İstanbul'\n",statsEnabled:false}]});
 await page.locator('.ts-document-list').getByRole('button').filter({hasText:'İstanbul gözlem'}).click();await expect(page.locator('.ts-document-details')).toContainText('İzinli hedef');
 await page.screenshot({path:'artifacts/team-v2/documents-ui.png'});
});

test("Team snippets keep selection separate from execution and edit via context menu",async({page})=>{
 await signIn(page);await page.getByRole('button',{name:'Komutlar / Workspaces',exact:true}).click();
 await page.getByRole('button',{name:'Komut paketi',exact:true}).click();const d=page.getByRole('dialog',{name:'Team belge düzenle'});
 await d.getByLabel('Ad',{exact:true}).fill('Disk kontrolü');await d.getByLabel('Komut metni').fill('df -h');await d.getByRole('button',{name:'Kaydet',exact:true}).click();await expect(d).toHaveCount(0);
 const card=page.locator('.ts-document-list').getByRole('button').filter({hasText:'Disk kontrolü'});await card.click();
 await expect(page.getByRole('button',{name:'Önizlenen bağlantıları aç'})).toBeDisabled();
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='team_v2_document_start').length)).toBe(0);
 await card.click({button:'right'});await page.getByRole('menuitem',{name:'Düzenle',exact:true}).click();await expect(d.getByLabel('Komut metni')).toHaveValue('df -h');
});


test("legacy Team migration previews denies without applying or retaining passwords",async({page})=>{
  await page.goto("/tests/ui/team-service-harness.html?legacy-preview");
  const panel=page.getByRole("region",{name:"Eski Team geçişi"});
  await panel.getByRole("textbox",{name:"Geçiş arşivi",exact:true}).fill("C:/fixture.ttmigration");
  await panel.getByLabel("Arşiv parolası",{exact:true}).fill("Disposable-test-only");
  await page.evaluate(()=>{(window as any).migrationFailure=true;});
  await panel.getByRole("button",{name:"Salt okunur önizleme"}).click();
  await expect(panel.getByRole("alert")).toContainText("Wrong password");
  await expect(panel.getByLabel("Arşiv parolası",{exact:true})).toHaveValue("");
  await page.evaluate(()=>{(window as any).migrationFailure=false;(window as any).migrationHold=true;});
  await panel.getByLabel("Arşiv parolası",{exact:true}).fill("Disposable-test-only");
  await panel.getByRole("button",{name:"Salt okunur önizleme"}).click();
  await expect(panel.getByRole("button",{name:"İşleniyor…"})).toBeDisabled();
  await page.evaluate(()=>{(window as any).releaseMigration();});
  await expect(panel).toContainText("Eski İstanbul");await expect(panel).toContainText("proxy");
  await expect(panel.getByLabel("Arşiv parolası",{exact:true})).toHaveValue("");
  await panel.getByText("İzinler ve açık engeller (ilk 100)",{exact:true}).click();
  await expect(panel.getByRole("cell",{name:"deny",exact:true})).toBeVisible();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.kind==="saveResource"||c.kind==="createMember").length)).toBe(0);
  await page.screenshot({path:"artifacts/team-v2/migration-preview.png"});
  await panel.getByRole("textbox",{name:"Geçiş arşivi",exact:true}).fill("C:/another.ttmigration");
  await expect(panel).not.toContainText("Eski İstanbul");
});

test("legacy account mapping binds reviewed archive and target, clears password and refuses stale membership",async({page})=>{
  await page.goto("/tests/ui/team-service-harness.html?legacy-preview");
  const panel=page.getByRole("region",{name:"Eski Team geçişi"});
  await panel.getByRole("textbox",{name:"Geçiş arşivi",exact:true}).fill("C:/fixture.ttmigration");
  await panel.getByLabel("Arşiv parolası",{exact:true}).fill("Disposable-test-only");
  await panel.getByRole("button",{name:"Salt okunur önizleme"}).click();
  await panel.getByText("Kaynak ve hedef hesap eşlemesi",{exact:true}).click();
  await expect(panel.getByLabel("leader hedef hesabı")).toHaveValue("");
  await panel.getByLabel("leader hedef hesabı").selectOption("new-owner");
  await expect(panel).toContainText("Eşleme değişti; yeniden doğrulayın.");
  const pw=panel.getByLabel("Eşleme doğrulaması için arşiv parolası");
  await pw.fill("Disposable-test-only");
  await panel.getByRole("button",{name:"Hesap eşlemesini doğrula"}).click();
  await expect(panel).toContainText("Hesap eşlemesi doğrulandı. Veriler henüz aktarılmadı.");
  await expect(pw).toHaveValue("");
  expect(await page.evaluate(()=>(window as any).migrationArguments)).toMatchObject({
    mappings:[{sourceAccountId:"old-owner",targetAccountId:"new-owner"}],expectedSourceSha256:"abc123",expectedTargetFingerprint:"target123"
  });
  await page.evaluate(()=>{(window as any).migrationStale=true;});
  await pw.fill("Disposable-test-only");
  await panel.getByRole("button",{name:"Hesap eşlemesini doğrula"}).click();
  await expect(panel.getByRole("alert")).toContainText("Hedef üyeler/yetkiler değişti");
  await expect(pw).toHaveValue("");
  await expect(panel).not.toContainText("Hesap eşlemesi doğrulandı. Veriler henüz aktarılmadı.");
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.kind==="saveResource"||c.kind==="createMember").length)).toBe(0);
});


test("Team identities use ordinary browsing without fetching secrets or mixing management", async ({page}) => {
  await page.addInitScript(() => { (window as any).shared = true; });
  await signIn(page);
  await page.getByRole("button", {name:"Kimliklerim",exact:true}).click();
  const privateKey = page.locator('[data-identity-id="private-key"]');
  await privateKey.click();
  await expect(page.getByRole("complementary",{name:"Kimlik bilgileri"})).toContainText("Yalnız bana özel");
  await expect(privateKey.locator("button")).toHaveCount(0);
  await expect(page.getByRole("button",{name:"Ortak kimliği kurtar",exact:true})).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).calls.filter((c:any)=>["revealShared","exportShared","setDefault"].includes(c.kind)))).toHaveLength(0);
  await page.getByRole("button",{name:"Kimlik bilgilerini kapat"}).click();
  await page.getByLabel("Kimliklerde ara").fill("shared");
  await expect(privateKey).toHaveCount(0);
  await expect(page.locator('[data-identity-id="shared-key"]')).toBeVisible();
  await page.getByLabel("Kimliklerde ara").fill("");
  await page.getByLabel("Kimlik görünürlüğü").selectOption("private");
  await expect(page.locator('[data-identity-id="shared-key"]')).toHaveCount(0);
  await page.getByRole("button",{name:"Kimlik liste görünümü"}).click();
  await expect(page.locator(".team-identities .records-grid")).toHaveClass(/list/);
  await privateKey.focus();await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menuitem",{name:"Varsayılan yap"})).toBeEnabled();
  await page.keyboard.press("Escape");await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(privateKey).toBeFocused();
  await page.getByLabel("Kimlik görünürlüğü").selectOption("all");
  await page.locator('[data-identity-id="shared-key"]').click();
  await expect(page.getByRole("complementary",{name:"Kimlik bilgileri"})).toContainText("Sırrı görüntüleme: Yok");
  await page.screenshot({path:"artifacts/team-v2/identities-browser.png"});
});


test("late policy denial cannot block a different Team", async ({page}) => {
  await page.goto("/tests/ui/team-service-harness.html");
  const a="00000000-0000-0000-0000-000000000001", b="00000000-0000-0000-0000-000000000002";
  await page.evaluate(a=>{(window as any).holdPolicyTeam=a;},a);
  await page.getByLabel("Servis adresi").fill("https://team.example.invalid");
  await page.getByLabel("Takım kimliği").fill(a);
  await page.waitForFunction(()=>!!(window as any).rejectPolicy);
  await page.getByLabel("Takım kimliği").fill(b);
  await page.evaluate(()=>{(window as any).rejectPolicy(Error("team_update_required:0.4.2:1"));});
  await page.waitForTimeout(700);
  await expect(page.getByLabel("Takım kimliği")).toHaveValue(b);
  await expect(page.getByRole("heading",{name:"Team için güncelleme gerekli"})).toHaveCount(0);
});

test("verified policy rollback returns to login, network failure does not",async({page})=>{
  await signIn(page);
  await page.evaluate(()=>{(window as any).requiredPolicy={required:true,minimumClientVersion:"0.4.2",apiVersion:1};window.dispatchEvent(new Event("focus"));});
  await expect(page.getByRole("heading",{name:"Team için güncelleme gerekli"})).toBeVisible();
  await page.evaluate(()=>{(window as any).policyNetworkFailure=true;window.dispatchEvent(new Event("focus"));});
  await expect(page.getByRole("heading",{name:"Team için güncelleme gerekli"})).toBeVisible();
  await page.evaluate(()=>{(window as any).policyNetworkFailure=false;(window as any).requiredPolicy={required:false};window.dispatchEvent(new Event("focus"));});
  await expect(page.getByRole("button",{name:"Giriş yap",exact:true})).toBeVisible();
  await expect(page.locator(".ts-workspace")).toHaveCount(0);
});

test("remembered Team restores fields and signs in without exposing a password",async({page})=>{
  await page.addInitScript(()=>{(window as any).savedLogin={profile:{serviceUrl:"https://team.example.invalid",teamId:"00000000-0000-0000-0000-000000000001",caPem:null},username:"leader",remembered:true};});
  await page.goto("/tests/ui/team-service-harness.html");
  await expect(page.getByLabel("Kullanıcı adı",{exact:true})).toHaveValue("leader");
  await expect(page.getByLabel("Parola",{exact:true})).toHaveValue("");
  await page.getByRole("button",{name:"Kayıtlı hesabımla devam et"}).click();
  await expect(page.locator(".ts-workspace")).toBeVisible();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==="team_v2_login"))).toHaveLength(0);
});

test("forgetting Team prevents remembered login without deleting the vault",async({page})=>{
  await page.addInitScript(()=>{(window as any).savedLogin={profile:{serviceUrl:"https://team.example.invalid",teamId:"00000000-0000-0000-0000-000000000001",caPem:null},username:"leader",remembered:true};});
  await page.goto("/tests/ui/team-service-harness.html");
  await page.getByRole("button",{name:"Bu cihazda beni unut"}).click();
  await expect(page.getByRole("button",{name:"Kayıtlı hesabımla devam et"})).toHaveCount(0);
  expect(await page.evaluate(()=>(window as any).savedLogin)).toBeNull();
});


test("personal proxy saves explicitly, clears password and allows forgetting",async({page})=>{
 await page.goto('/tests/ui/team-service-harness.html?proxy');
 await page.getByLabel('Kayıtlı proxy kullanıcı adı').fill('personal');
 await page.getByLabel('Kayıtlı proxy parolası').fill('Disposable-Proxy-UI!');
 await page.getByRole('button',{name:'Proxy kimliğini kaydet'}).click();
 await expect(page.getByLabel('Kayıtlı proxy parolası')).toHaveValue('');
 await expect(page.getByText('Kayıtlı proxy kimliği var.')).toBeVisible();
 await expect(page.getByRole('button',{name:'Proxy kimliğini kaydet'})).toBeDisabled();
 await page.getByRole('button',{name:'Proxy kimliğini unut'}).click();
 await expect(page.getByText('Kayıtlı proxy kimliği var.')).toHaveCount(0);
 await expect(page.getByRole('button',{name:'Proxy kimliğini unut'})).toBeDisabled();
});
test("personal proxy warns about changed endpoint and keeps failed input reviewable",async({page})=>{
 await page.addInitScript(()=>{(window as any).proxyStatus={credentialId:'id',revision:1,username:'personal',endpoint:{kind:'http',host:'old.invalid',port:8080,authenticate:true}};(window as any).proxyFailure=true;});
 await page.goto('/tests/ui/team-service-harness.html?proxy');
 await expect(page.getByText(/Proxy adresi veya türü değişmiş/)).toBeVisible();
 await page.getByLabel('Kayıtlı proxy parolası').fill('Disposable-Proxy-UI!');
 await page.getByRole('button',{name:'Proxy kimliğini kaydet'}).click();
 await expect(page.getByText('revision_conflict',{exact:true})).toBeVisible();
 await expect(page.getByLabel('Kayıtlı proxy parolası')).toHaveValue('Disposable-Proxy-UI!');
});
test("permanent historical route requires preview and does not hide a revision conflict",async({page})=>{
 await page.goto('/tests/ui/team-service-harness.html?history');
 await page.getByLabel('Geçmiş sürüm').selectOption('2');
 await page.getByRole('button',{name:'Yeni revizyon olarak geri yükle',exact:true}).click();
 await expect(page.getByLabel('Geri yükleme bağlantı önizlemesi')).toContainText('Historical jump');
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.kind==='finishChain'))).toHaveLength(0);
 await page.evaluate(()=>{(window as any).chainFailure=true;});
 await page.getByRole('button',{name:'Geri yüklemeyi uygula'}).click();
 await expect(page.getByRole('alert')).toContainText('revision_conflict');
 await page.getByRole('button',{name:'Yenile',exact:true}).click();
 await expect(page.getByRole('button',{name:'Geri yüklemeyi uygula'})).toHaveCount(0);
 await expect(page.getByLabel('Geri yükleme bağlantı önizlemesi')).toHaveCount(0);
 await page.evaluate(()=>{(window as any).chainFailure=false;});
 await page.getByRole('button',{name:'Yeni revizyon olarak geri yükle',exact:true}).click();
 await page.getByRole('button',{name:'Geri yüklemeyi uygula'}).click();
 await expect(page.getByRole('status')).toContainText('Sürüm 4 kaydedildi.');
});
