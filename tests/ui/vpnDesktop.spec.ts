import { expect, test, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";

// This suite verifies the actual panel against a mocked IPC boundary; no tunnel is opened.
async function setup(page: Page, available = true, deferStatus = false, ssoBeforeReply = false, diagnostics = ssoBeforeReply) {
  await page.addInitScript(
    ({ available, deferStatus, ssoBeforeReply }) => {
      const callbacks = new Map<number, (event: unknown) => void>();
      const listeners = new Map<number, { event: string; callback: number }>();
      let sequence = 0;
      let generation = 0;
      const statusWaiters: ((value: unknown) => void)[] = [];
      const availability = {available,platform:"Windows",reason:available ? "IPC test double: no real tunnel" : "Windows motoru, imzalı sürücü ve yetkili ağ hizmeti hazır değil.",labOnly:available,maxConnections:3,authTestAvailable:ssoBeforeReply};
      const profiles = ["OpenVPN", "Fortinet", "GlobalProtect", "Extra"].map(
        (label, i) => ({
          id: `p${i}`,
          version: 1,
          label,
          family: i === 0 ? "openVpn" : i === 2 ? "globalProtect" : "fortinet",
          source: i === 0 ? "test.ovpn" : "manual",
          server: i ? "https://vpn.example" : "",
          groupId: "Lab",
          username: "",
          hasPassword: i === 1,
          hasConfig: i === 0,
          realm: "",
          remember: i === 1,
        }),
      );
      const sessions: any[] = [];
      const calls: { command: string; args: any }[] = [];
      const emit = (session: any) => {
        const index = sessions.findIndex(
          (item) => item.profileId === session.profileId,
        );
        if (index >= 0) sessions[index] = session;
        else sessions.push(session);
        for (const listener of listeners.values())
          if (listener.event === "vpn-event")
            callbacks.get(listener.callback)?.({ payload: session });
      };
      Object.assign(window, {
        isTauri: true,
        vpnTest: { drop: (paths: string[], position: {x:number;y:number}) => { for (const listener of listeners.values()) if(listener.event === "tauri://drag-drop") callbacks.get(listener.callback)?.({payload:{paths,position}}); }, importResult: null, calls, emit, sessions, listeners, profiles, availability, finishStatus: () => statusWaiters.splice(0).forEach(resolve => resolve(availability)) },
        __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
        __TAURI_INTERNALS__: {
          transformCallback: (callback: (value: unknown) => void) => {
            const id = ++sequence;
            callbacks.set(id, callback);
            return id;
          },
          unregisterCallback: (id: number) => callbacks.delete(id),
          invoke: async (command: string, args: any = {}) => {
            calls.push({ command, args });
            if (command === "plugin:event|listen") {
              const id = ++sequence;
              listeners.set(id, { event: args.event, callback: args.handler });
              return id;
            }
            if (command === "plugin:event|unlisten") {
              listeners.delete(args.eventId);
              return;
            }
            if (command === "app_info")
              return {
                version: "0.4.3-dev.1",
                defaultVaultPath: "disposable",
                home: "disposable",
              };
            if (command === "vault_info")
              return {
                id: "vpn-test",
                name: "Disposable IPC mock",
                path: "disposable",
                deviceId: "test",
                records: [],
              };
            if (command === "team_status") return null;
            if (command === "authenticator") {
              if (args.action.kind !== "list") throw Error("This fixture only lists disposable TOTP codes");
              const now = Math.floor(Date.now() / 1000);
              return { context: "vpn-test:authenticator", serverTime: now, entries: [{ id: "rfc6238", revision: 1, label: "disposable@example.test", issuer: "Public RFC 6238 example", algorithm: "SHA1", digits: 8, period: 30, code: "07081804", validUntil: now + 30, visibility: "private", permissions: ["connect"], canRemove: false, canUpdate: false }] };
            }
            if (command === "plugin:clipboard-manager|write_text") { (window as any).vpnTest.clipboard = args.text; return; }
            if (command === "vpn_status")
              return deferStatus ? new Promise(resolve => statusWaiters.push(resolve)) : availability;
            if (command === "vpn_profiles") return profiles;
            if (command === "vpn_sessions") return sessions;
            if (["vpn_connect","vpn_sso_test_start"].includes(command)) {
              const session = {
                id: `s${++generation}`,
                vaultId: "vpn-test",
                profileId: args.profileId,
                generation,
                state: "connecting",
                authOnly: command === "vpn_sso_test_start",
              };
              emit(session);
              if(ssoBeforeReply)emit({...session,state:"challenge",challenge:{id:1,kind:"sso",provider:"globalProtect",fields:[]}});
              return session;
            }
            if (command === "vpn_disconnect") {
              const session = sessions.find((item) => item.id === args.id);
              if ((window as any).vpnTest.disconnectHook) return (window as any).vpnTest.disconnectHook(session);
              emit({ ...session, state: "disconnected", challenge: undefined });
              return;
            }
            if (["vpn_answer","vpn_sso_open","vpn_sso_close"].includes(command)) return;
            if (command === "vpn_profile_save") {
              if (Object.hasOwn(args.profile, "config"))
                throw new Error("Unsupported VPN field: config");
              if (
                !args.profile.server.startsWith("https:") &&
                args.profile.family !== "openVpn"
              )
                throw new Error("HTTPS adresi gerekli");
              const previous = profiles.find((item) => item.id === args.profile.id);
              const saved = {
                ...args.profile,
                id: args.profile.id || `p${profiles.length}`,
                hasConfig: !!args.importToken || !!previous?.hasConfig,
                hasPassword: args.profile.remember && (Object.hasOwn(args.profile, "password") ? !!args.profile.password : !!previous?.hasPassword),
              };
              delete saved.password;
              delete saved.config;
              const previousIndex = profiles.findIndex((item) => item.id === saved.id);
              if (previousIndex >= 0) profiles[previousIndex] = saved;
              else profiles.push(saved);
              return saved;
            }
            if (command === "vpn_profile_delete") {
              profiles.splice(
                profiles.findIndex((item) => item.id === args.id),
                1,
              );
              return;
            }
            if (command === "plugin:dialog|open")
              return (window as any).vpnTest.importPath || "/disposable/test.ovpn";
            if (command === "vpn_import")
              return (window as any).vpnTest.importResult || {
                token: "opaque-test-token",
                warnings: [],
                missingFiles: [],
                blocked: false,
              };
            throw new Error(`Unmocked command: ${command}`);
          },
        },
      });
    },
    { available, deferStatus, ssoBeforeReply },
  );
  await page.goto(diagnostics ? "/?vpnDiagnostics" : "/");
  await page.getByRole("button", { name: "VPN", exact: true }).click();
  await expect(page.locator(".vpn-card")).toHaveCount(4);
}

test("normal VPN navigation follows SFTP and hides diagnostics", async ({page}) => {
  await setup(page);
  const labels = await page.locator(".main-nav > button").allTextContents();
  expect(labels.indexOf("VPN")).toBe(labels.indexOf("SFTP") + 1);
  await expect(page.getByRole("button", {name:"Arayüz demosu"})).toHaveCount(0);
  await expect(page.getByRole("button", {name:"SSO girişini test et (tünelsiz)"})).toHaveCount(0);
  await expect(page.locator(".vpn-workspace")).not.toContainText("Geliştirme");
});

for (const outcome of ["disconnected", "failed"] as const) test(`disconnect ACK preserves pending state until the actual ${outcome} event and keeps the other VPN`, async ({page}) => {
  await setup(page);
  for (let i=0;i<2;i++) await page.locator(".vpn-card").nth(i).getByRole("button",{name:"Bağlan",exact:true}).click();
  await page.evaluate(() => {
    const t=(window as any).vpnTest;
    for (const session of [...t.sessions]) t.emit({...session,state:"connected"});
    t.disconnectHook=(session:any)=>{t.emit({...session,state:"disconnecting",error:"cleanup_pending: windows_interface_unregister_pending"});};
  });
  const first=page.locator(".vpn-card").nth(0);
  const second=page.locator(".vpn-card").nth(1);
  await first.getByRole("button",{name:"Bağlantıyı kes",exact:true}).evaluate((button:HTMLButtonElement)=>{button.click();button.click();});
  await expect(first.getByRole("status")).toHaveText("Kapatılıyor");
  await expect(first.getByRole("button",{name:"Bağlantıyı kes",exact:true})).toBeDisabled();
  await expect(second.getByRole("status")).toHaveText("Bağlı");
  expect(await page.evaluate(()=>(window as any).vpnTest.calls.filter((call:any)=>call.command==="vpn_disconnect").length)).toBe(1);
  await page.evaluate(outcome=>{
    const t=(window as any).vpnTest;
    const session=t.sessions.find((session:any)=>session.profileId==="p0");
    t.emit({...session,state:outcome,error:outcome==="failed"?"cleanup_required: routes_retained_until_quiescence":undefined});
    if(outcome==="failed") t.emit({...session,state:"disconnected",error:undefined});
  },outcome);
  await expect(first.getByRole("status")).toHaveText(outcome==="failed"?"Bağlantı başarısız":"Bağlı değil");
  if(outcome==="failed") await expect(first.getByRole("alert")).toContainText("cleanup_required: routes_retained_until_quiescence");
  await expect(second.getByRole("status")).toHaveText("Bağlı");
});

for (const purpose of ["otp", "secret"]) test(`pending OTP challenge survives Authenticator handoff and manual code submission (${purpose})`, async ({ page }) => {
  await setup(page);
  await page.locator(".vpn-card").nth(1).getByRole("button", { name: "Bağlan", exact: true }).click();
  const otpLabel = purpose === "otp" ? "Tek kullanımlık kod" : "Challenge:";
  await page.evaluate(({ purpose, otpLabel }) => {
    const t = (window as any).vpnTest;
    t.emit({ ...t.sessions[0], state: "challenge", challenge: { id: 12, message: "Disposable OTP challenge", fields: [
      { name: "username", label: "Kullanıcı adı", purpose: "username", secret: false },
      { name: "otp", label: otpLabel, purpose, secret: true },
    ] } });
  }, { purpose, otpLabel });
  await page.getByRole("button", { name: "Yanıtla", exact: true }).click();
  const challenge = page.getByRole("dialog", { name: "VPN kimlik doğrulama" });
  await challenge.getByLabel(/^Kullanıcı adı/).fill("disposable-user");
  await challenge.getByLabel(otpLabel).fill("12");
  const identity = await page.evaluate(() => { const s = (window as any).vpnTest.sessions[0]; return { id: s.id, generation: s.generation, challengeId: s.challenge.id }; });
  await challenge.getByRole("button", { name: "Authenticator’ı aç", exact: true }).click();
  await expect(challenge).not.toBeVisible();
  await page.getByRole("button", { name: "disposable@example.test kodunu kopyala" }).click();
  await expect(page.getByRole("status")).toHaveText("Kod panoya kopyalandı.");
  await mkdir("artifacts/authenticator-ui", { recursive: true });
  await page.evaluate(() => {
    const label = document.createElement("div");
    label.textContent = "IPC MOCK · Disposable Authenticator / VPN preview";
    label.style.cssText = "position:fixed;bottom:12px;right:12px;z-index:9999;padding:8px 12px;background:#141a21;color:#dde4ec;border:1px solid #617f9c;border-radius:8px;pointer-events:none";
    document.body.append(label);
  });
  await page.screenshot({ path: "artifacts/authenticator-ui/mock-app-vpn-handoff.png" });
  expect(await page.evaluate(() => (window as any).vpnTest.calls.filter((c: any) => ["vpn_answer", "vpn_disconnect"].includes(c.command)))).toEqual([]);
  await page.getByRole("button", { name: "VPN", exact: true }).click();
  await expect(challenge).toBeVisible();
  await expect(challenge.getByLabel(/^Kullanıcı adı/)).toHaveValue("disposable-user");
  await expect(challenge.getByLabel(otpLabel)).toHaveValue("12");
  expect(await page.evaluate(() => { const s = (window as any).vpnTest.sessions[0]; return { id: s.id, generation: s.generation, challengeId: s.challenge.id }; })).toEqual(identity);
  const copied = await page.evaluate(() => (window as any).vpnTest.clipboard);
  expect(copied).toBe("07081804");
  await challenge.getByLabel(otpLabel).fill(copied);
  await challenge.getByRole("button", { name: "Gönder", exact: true }).click();
  await expect(challenge).not.toBeVisible();
  expect(await page.evaluate(() => (window as any).vpnTest.calls.filter((c: any) => c.command === "vpn_answer").map((c: any) => c.args))).toEqual([{ ...identity, values: { username: "disposable-user", otp: "07081804" } }]);
  expect(await page.evaluate(() => (window as any).vpnTest.calls.some((c: any) => c.command === "vpn_disconnect"))).toBe(false);
});

for(const command of ["vpn_connect","vpn_sso_test_start"])test(`${command} keeps an SSO event delivered before its initial reply`,async({page})=>{
  await setup(page,true,false,true);
  const card=page.locator(".vpn-card").nth(2);
  await card.getByRole("button",{name:command==="vpn_connect" ? "Bağlan" : "SSO girişini test et (tünelsiz)",exact:true}).click();
  await expect(card.getByRole("status")).toHaveText("Kimlik doğrulama bekliyor");
  await expect(card.getByRole("button",{name:"Yanıtla",exact:true})).toBeVisible();
});

test("profiles remain editable while a read-only runtime preflight is pending", async ({page}) => {
  await setup(page,true,true);
  await expect(page.locator(".vpn-preview-note")).toContainText("hizmeti kontrol ediliyor");
  await expect(page.getByRole("button",{name:"Bağlan",exact:true}).first()).toBeDisabled();
  await page.locator(".vpn-card").first().getByRole("button",{name:"Düzenle",exact:true}).click();
  const dialog = page.getByRole("dialog",{name:"VPN profilini kaydet"});
  await dialog.getByLabel("Profil adı").fill("Edited during preflight");
  await dialog.getByRole("button",{name:"Kaydet",exact:true}).click();
  await expect(page.locator(".vpn-card").filter({hasText:"Edited during preflight"})).toHaveCount(1);
  await page.evaluate(()=>(window as any).vpnTest.finishStatus());
  await expect(page.locator(".vpn-preview-note")).toContainText("IPC test double");
  await expect(page.getByRole("button",{name:"Bağlan",exact:true}).first()).toBeEnabled();
});

for (const platform of ["Windows","linux"]) test(`available ${platform} banner reflects the platform and guarded development readiness`, async ({page}) => {
  await setup(page,true,true);
  await page.evaluate((platform) => {
    const t=(window as any).vpnTest;t.availability.platform=platform;t.availability.reason="";t.finishStatus();
  },platform);
  const banner=page.locator(".vpn-preview-note");
  await expect(banner).toContainText(`${platform}: VPN bağlantıları hazır.`);
  if (platform === "linux") await expect(banner).toContainText("yalıtılmış Linux laboratuvarında");
  else await expect(banner).not.toContainText("Linux laboratuvarında");
  await expect(page.getByRole("button",{name:"Bağlan",exact:true}).first()).toBeEnabled();
});

test("portal and gateway SSO are separate explicit stages, auth-only success and failure remain visible", async ({page}) => {
  await setup(page,true,false,true);
  const card=page.locator(".vpn-card").nth(2);
  await card.getByRole("button",{name:"SSO girişini test et (tünelsiz)",exact:true}).click();
  const stage = async (state: string, challenge?: any, extra = {}) => page.evaluate(({state,challenge,extra}) => {
    const t=(window as any).vpnTest;
    t.emit({...t.sessions[0],state,challenge,...extra});
  },{state,challenge,extra});
  await stage("challenge",{id:1,kind:"sso",provider:"globalProtect",stage:"portal",fields:[]});
  await expect(card.getByRole("status")).toHaveText("Portal SSO girişi bekleniyor");
  await card.getByRole("button",{name:"Yanıtla",exact:true}).click();
  const dialog=page.getByRole("dialog",{name:"VPN kimlik doğrulama"});
  await expect(dialog).toContainText("Bu giriş testi tünel açmaz");
  await stage("connecting",undefined,{authStage:"portal"});
  await expect(dialog).toBeHidden();
  await expect(card.getByRole("status")).toHaveText("Portal girişi doğrulanıyor · tünel açılmayacak");
  await stage("challenge",{id:2,message:"Please select GlobalProtect gateway.",fields:[{name:"gateway",label:"GATEWAY",secret:false,purpose:"choice",value:"gateway.example",choices:[{value:"gateway.example",label:"Disposable gateway"}]}]});
  await expect(card.getByRole("status")).toHaveText("Portal girişi tamamlandı · gateway seçimi bekleniyor");
  await card.getByRole("button",{name:"Yanıtla",exact:true}).click();
  await expect(dialog.getByLabel("GATEWAY")).toHaveValue("gateway.example");
  await dialog.getByRole("button",{name:"Gönder",exact:true}).click();
  await stage("challenge",{id:3,kind:"sso",provider:"globalProtect",stage:"gateway",fields:[]},{authStage:"gateway"});
  await expect(card.getByRole("status")).toHaveText("Gateway SSO girişi bekleniyor");
  // A new gateway login requires an explicit action; the portal pane is never reopened automatically.
  await expect(dialog).toBeHidden();
  await card.getByRole("button",{name:"Yanıtla",exact:true}).click();
  await expect(dialog).toContainText("Seçilen gateway için");
  await stage("challenge",{id:1,kind:"sso",provider:"globalProtect",stage:"portal",fields:[]});
  await expect(card.getByRole("status")).toHaveText("Gateway SSO girişi bekleniyor");
  await stage("connecting",undefined,{authStage:"gateway"});
  await expect(dialog).toBeHidden();
  await expect(card.getByRole("status")).toHaveText("Gateway girişi doğrulanıyor · tünel açılmayacak");
  await stage("failed",undefined,{error:"Gateway SSO giriş testi tamamlanamadı; tünel açılmadı."});
  await stage("disconnected",undefined,{error:undefined});
  await expect(card.getByRole("status")).toHaveText("Bağlantı başarısız");
  await expect(card.getByRole("alert")).toContainText("Gateway SSO giriş testi tamamlanamadı");
  await expect(card.getByRole("button",{name:"SSO girişini test et (tünelsiz)",exact:true})).toBeEnabled();
  await card.getByRole("button",{name:"SSO girişini test et (tünelsiz)",exact:true}).click();
  await expect(card.getByRole("alert")).toHaveCount(0);
  await stage("disconnected",undefined,{authenticationVerified:true});
  await expect(card.getByRole("status")).toHaveText("Giriş doğrulandı · tünel açılmadı");
  await expect(page.locator(".vpn-overview")).toContainText("0/ en fazla 3");
});

test("SSO uses a native pane with only scoped identity, cancels cleanly and cannot submit a form answer", async ({page}) => {
  await setup(page);
  await page.locator(".vpn-card").nth(2).getByRole("button",{name:"Bağlan",exact:true}).click();
  await page.evaluate(() => {
    const t = (window as any).vpnTest;
    t.emit({...t.sessions[0],state:"challenge",challenge:{id:9,kind:"sso",provider:"globalProtect",fields:[]}});
  });
  await page.getByRole("button",{name:"Yanıtla",exact:true}).click();
  const dialog = page.getByRole("dialog",{name:"VPN kimlik doğrulama"});
  await expect(dialog.getByRole("region",{name:"Kurumsal SSO oturum açma"})).toBeVisible();
  await expect(dialog.locator("input")).toHaveCount(0);
  await expect(dialog.getByRole("button",{name:"Gönder",exact:true})).toHaveCount(0);
  await expect.poll(() => page.evaluate(()=>(window as any).vpnTest.calls.filter((c:any)=>c.command === "vpn_sso_open").length)).toBeGreaterThan(0);
  const calls = await page.evaluate(()=>(window as any).vpnTest.calls.filter((c:any)=>c.command === "vpn_sso_open"));
  for (const {args} of calls) {
    expect(Object.keys(args).sort()).toEqual(["bounds","challengeId","generation","id"]);
    expect(args.challengeId).toBe(9);
    expect(args.generation).toBe(1);
  }
  await page.locator(".modal-shade").click({position:{x:4,y:4}});
  await expect(dialog).toBeVisible();
  expect(await page.evaluate(()=>(window as any).vpnTest.calls.some((c:any)=>c.command === "vpn_sso_close"))).toBe(false);
  await dialog.getByRole("button",{name:"Bağlantıyı iptal et"}).click();
  await expect(dialog).toBeHidden();
  await expect.poll(() => page.evaluate(()=>(window as any).vpnTest.calls.filter((c:any)=>c.command === "vpn_sso_close").length)).toBe(1);
  expect(await page.evaluate(()=>(window as any).vpnTest.calls.some((c:any)=>c.command === "vpn_answer"))).toBe(false);
  await page.evaluate(() => {
    const t = (window as any).vpnTest;
    t.emit({...t.sessions[0],state:"challenge",challenge:{id:9,kind:"sso",provider:"globalProtect",fields:[]}});
  });
  await expect(page.locator(".vpn-card").nth(2).getByRole("status")).toHaveText("Bağlı değil");
});

test("unavailable desktop is explicit and demo remains separate", async ({
  page,
}) => {
  await setup(page, false, false, false, true);
  await expect(page.locator(".vpn-preview-note")).toContainText(
    "Windows: Windows motoru, imzalı sürücü ve yetkili ağ hizmeti hazır değil.",
  );
  await expect(page.locator(".vpn-preview-note")).not.toContainText("VPN bağlantıları hazır.");
  await expect(page.locator(".vpn-preview-note")).not.toContainText("Linux laboratuvarında");
  await expect(
    page.getByRole("button", { name: "Bağlan", exact: true }).first(),
  ).toBeDisabled();
  await page.screenshot({
    path: "artifacts/vpn-ui/06-desktop-panel-ipc-mocked.png",
  });
  await page.getByRole("button", { name: "Arayüz demosu" }).click();
  await expect(page.getByText("Etkileşimli arayüz önizlemesi.")).toBeVisible();
  await page.getByRole("button", { name: "Masaüstü VPN paneline dön" }).click();
  await expect(page.locator(".vpn-card")).toHaveCount(4);
  expect(
    await page.evaluate(
      () =>
        [...(window as any).vpnTest.listeners.values()].filter(
          (item: any) => item.event === "vpn-event",
        ).length,
    ),
  ).toBe(1);
});

test("three sessions remain independent, old vault and generation events are ignored, OTP is ephemeral", async ({
  page,
}) => {
  await setup(page);
  for (let i = 0; i < 3; i++)
    await page
      .locator(".vpn-card")
      .nth(i)
      .getByRole("button", { name: "Bağlan", exact: true })
      .click();
  await expect(
    page
      .locator(".vpn-card")
      .nth(3)
      .getByRole("button", { name: "Bağlan", exact: true }),
  ).toBeDisabled();
  await page.evaluate(() => {
    const t = (window as any).vpnTest;
    for (const session of [...t.sessions])
      t.emit({ ...session, state: "connected" });
    t.emit({
      ...t.sessions[0],
      vaultId: "other-vault",
      generation: 999,
      state: "failed",
    });
    t.emit({
      ...t.sessions[1],
      state: "challenge",
      challenge: {
        id: 7,
        message: "OTP required",
        fields: [
          {
            name: "otp",
            label: "Tek kullanımlık kod",
            purpose: "otp",
            secret: true,
          },
        ],
      },
    });
  });
  await expect(page.locator(".vpn-card").nth(0).getByRole("status")).toHaveText(
    "Bağlı",
  );
  await page.getByRole("button", { name: "Yanıtla" }).click();
  const dialog = page.getByRole("dialog", { name: "VPN kimlik doğrulama" });
  await dialog.getByLabel("Tek kullanımlık kod").fill("123456");
  await expect(dialog.locator('input[type="password"]')).toHaveCount(1);
  await dialog.getByRole("button", { name: "Gönder" }).dblclick();
  await expect(dialog).not.toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).vpnTest.calls.filter(
          (call: any) => call.command === "vpn_answer",
        ).length,
    ),
  ).toBe(1);
  expect(
    await page.evaluate(() =>
      (window as any).vpnTest.calls.some(
        (call: any) => call.command === "vpn_profile_save",
      ),
    ),
  ).toBe(false);
  await page
    .locator(".vpn-card")
    .nth(1)
    .getByRole("button", { name: "Bağlantıyı kes" })
    .click();
  await page.evaluate(() => {
    const t = (window as any).vpnTest;
    t.emit({ ...t.sessions[1], state: "connected" });
  });
  await expect(page.locator(".vpn-card").nth(1).getByRole("status")).toHaveText(
    "Bağlı değil",
  );
  await expect(page.locator(".vpn-card").nth(2).getByRole("status")).toHaveText(
    "Bağlı",
  );
});

test("native import uses an opaque token and saves no config or OTP", async ({
  page,
}) => {
  await setup(page);
  await page.getByRole("button", { name: "Yeni VPN", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "VPN profilini kaydet" });
  await dialog.getByLabel("VPN türü").selectOption("openVpn");
  await dialog.getByLabel("Profil adı").fill("Imported test");
  await dialog.getByRole("button", { name: ".ovpn / .conf içe aktar" }).click();
  await expect(dialog.getByText("test.ovpn", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "Kaydet", exact: true }).click();
  await expect(page.locator(".vpn-card")).toHaveCount(5);
  const saved = await page.evaluate(
    () =>
      (window as any).vpnTest.calls.find(
        (call: any) => call.command === "vpn_profile_save",
      ).args,
  );
  expect(saved.importToken).toBe("opaque-test-token");
  expect(saved.profile).not.toHaveProperty("config");
  expect(saved.profile).not.toHaveProperty("otp");
  await page
    .getByRole("button", { name: "Imported test profilini sil" })
    .click();
  await expect(page.locator(".vpn-card")).toHaveCount(4);
});

for (const mode of ["certificate", "encryptedKey", "account", "staticOtp"]) test(`OpenVPN import shows ${mode} authentication and the optional DNS limitation`, async ({page}) => {
  await setup(page);
  await page.getByRole("button", {name:"Yeni VPN",exact:true}).click();
  const dialog = page.getByRole("dialog", {name:"VPN profilini kaydet"});
  await dialog.getByLabel("VPN türü").selectOption("openVpn");
  await dialog.getByLabel("Profil adı").fill(`Public ${mode}`);
  await page.evaluate(mode => {
    (window as any).vpnTest.importResult = {token:"auth-token",family:"openVpn",blocked:false,missingFiles:[],
      warnings:["Satır 19: İsteğe bağlı block-outside-dns uygulanmadı; DNS sızıntısı engelleme desteği yok."],
      authentication:{certificate:mode!=="account",usernamePassword:["account","staticOtp"].includes(mode),privateKeyPassword:mode==="encryptedKey",staticChallenge:mode==="staticOtp"}};
  }, mode);
  await dialog.getByRole("button", {name:".ovpn / .conf içe aktar"}).click();
  await expect(dialog.getByRole("alert")).toHaveCount(0);
  await expect(dialog.getByRole("status")).toContainText("DNS sızıntısı engelleme desteği yok");
  if (mode === "certificate") await expect(dialog).toContainText("Sertifika/anahtar ile parolasız giriş.");
  if (mode === "encryptedKey") await expect(dialog).toContainText("Özel anahtar parolası bağlantıda sorulur ve kaydedilmez.");
  if (["certificate","encryptedKey"].includes(mode)) {
    await expect(dialog.getByLabel("Kullanıcı adı", {exact:true})).toHaveCount(0);
    await expect(dialog.getByLabel("Parolayı şifreli kasada hatırla")).toHaveCount(0);
  } else await expect(dialog.getByLabel("Kullanıcı adı", {exact:true})).toBeVisible();
  if (mode === "staticOtp") {
    await expect(dialog).toContainText("bu formun yanıtlarından hesap parolası kaydedilmez veya otomatik doldurulmaz");
    await expect(dialog.getByLabel("Parolayı şifreli kasada hatırla")).toHaveCount(0);
    await expect(dialog.locator('input[type="password"]')).toHaveCount(0);
  }
  await dialog.getByRole("button", {name:"Kaydet",exact:true}).click();
  await expect(dialog).toBeHidden();
  const profile = await page.evaluate(() => (window as any).vpnTest.calls.find((call:any) => call.command === "vpn_profile_save").args.profile);
  expect(profile).not.toHaveProperty("authentication");
  expect(profile).not.toHaveProperty("otp");
  if (mode !== "account") expect(profile).not.toHaveProperty("password");
});

test("OpenVPN key unlock, account login and OTP remain separate masked transient answers", async ({page}) => {
  await setup(page);
  await page.locator(".vpn-card").first().getByRole("button", {name:"Bağlan",exact:true}).click();
  const stage = async (id:number, fields:unknown[]) => page.evaluate(({id,fields}) => {
    const t = (window as any).vpnTest;t.emit({...t.sessions[0],state:"challenge",challenge:{id,fields}});
  }, {id,fields});
  const dialog = page.getByRole("dialog", {name:"VPN kimlik doğrulama"});
  await stage(1,[{name:"privateKeyPassword",purpose:"keyPassword",label:"Özel anahtar parolası",secret:true}]);
  await page.getByRole("button", {name:"Yanıtla",exact:true}).click();
  await expect(dialog.locator('input[type="password"]')).toHaveCount(1);
  await dialog.getByLabel("Özel anahtar parolası").fill("Disposable key password");
  await dialog.getByRole("button", {name:"Gönder",exact:true}).click();
  await expect(dialog).toBeHidden();
  await stage(2,[{name:"username",purpose:"username",label:"Kullanıcı adı",secret:false},{name:"password",purpose:"password",label:"Parola",secret:true}]);
  await page.getByRole("button", {name:"Yanıtla",exact:true}).click();
  await dialog.getByLabel(/^Kullanıcı adı/).fill("public-user");
  await dialog.getByLabel(/^Parola/).fill("Disposable account password");
  await dialog.getByRole("button", {name:"Gönder",exact:true}).click();
  await expect(dialog).toBeHidden();
  await stage(3,[{name:"response",purpose:"otp",label:"Tek kullanımlık kod",secret:true}]);
  await page.getByRole("button", {name:"Yanıtla",exact:true}).click();
  await expect(dialog.locator('input[type="password"]')).toHaveCount(1);
  await expect(dialog.getByLabel("Tek kullanımlık kod")).toHaveValue("");
  await dialog.getByLabel("Tek kullanımlık kod").fill("123456");
  await dialog.getByRole("button", {name:"Gönder",exact:true}).click();
  const calls = await page.evaluate(() => (window as any).vpnTest.calls);
  expect(calls.filter((call:any) => call.command === "vpn_answer").map((call:any) => call.args.values)).toEqual([
    {privateKeyPassword:"Disposable key password"},{username:"public-user",password:"Disposable account password"},{response:"123456"},
  ]);
  expect(calls.some((call:any) => call.command === "vpn_profile_save")).toBe(false);
});

for (const mode of ["certificate", "staticOtp"]) for (const family of ["fortinet", "globalProtect"]) test(`changing imported ${mode} to ${family} restores account controls and preserves the entered password`, async ({page}) => {
  await setup(page);
  await page.getByRole("button", {name:"Yeni VPN",exact:true}).click();
  const dialog = page.getByRole("dialog", {name:"VPN profilini kaydet"});
  await dialog.getByLabel("VPN türü").selectOption("openVpn");
  await dialog.getByLabel("Profil adı").fill("Family transition");
  await page.evaluate(mode => {
    (window as any).vpnTest.importResult = {token:"auth-token",family:"openVpn",blocked:false,missingFiles:[],warnings:[],
      authentication:{certificate:true,usernamePassword:mode==="staticOtp",privateKeyPassword:false,staticChallenge:mode==="staticOtp"}};
  }, mode);
  await dialog.getByRole("button", {name:".ovpn / .conf içe aktar"}).click();
  await dialog.getByLabel("VPN türü").selectOption(family);
  await expect(dialog.getByLabel("Kullanıcı adı", {exact:true})).toBeVisible();
  await expect(dialog.getByLabel("Parolayı şifreli kasada hatırla")).toBeChecked();
  await dialog.getByLabel(family === "fortinet" ? "VPN sunucusu" : "Portal / gateway adresi").fill("https://vpn.example");
  await dialog.getByLabel("Kullanıcı adı", {exact:true}).fill("public-user");
  await dialog.getByLabel("Parola", {exact:true}).fill("Disposable transition password");
  await dialog.getByRole("button", {name:"Kaydet",exact:true}).click();
  await expect(dialog).toBeHidden();
  const args = await page.evaluate(() => (window as any).vpnTest.calls.find((call:any) => call.command === "vpn_profile_save").args);
  expect(args.profile.family).toBe(family);
  expect(args.profile.password).toBe("Disposable transition password");
  expect(args.profile).not.toHaveProperty("authentication");
  expect(args.importToken).toBeUndefined();
});

test("editing metadata preserves hidden config and remembered password until explicitly cleared", async ({ page }) => {
  await setup(page);
  const card = (label: string) => page.locator(".vpn-card").filter({ has: page.getByRole("button", { name: `${label} profilini sil`, exact: true }) });
  const dialog = page.getByRole("dialog", { name: "VPN profilini kaydet" });
  await card("OpenVPN").getByRole("button", { name: "Düzenle", exact: true }).click();
  await dialog.getByLabel("Profil adı").fill("OpenVPN metadata");
  await dialog.getByRole("button", { name: "Kaydet", exact: true }).click();
  await expect(dialog).toBeHidden();
  await card("Fortinet").getByRole("button", { name: "Düzenle", exact: true }).click();
  await expect(dialog.getByLabel("Yeni parola (boş bırakılırsa mevcut korunur)")).toHaveValue("");
  await dialog.getByLabel("Profil adı").fill("Fortinet metadata");
  await dialog.getByRole("button", { name: "Kaydet", exact: true }).click();
  await expect(dialog).toBeHidden();
  let results = await page.evaluate(() => ({ calls: (window as any).vpnTest.calls.filter((c: any) => c.command === "vpn_profile_save"), profiles: (window as any).vpnTest.profiles }));
  for (const call of results.calls) {
    expect(call.args.profile).not.toHaveProperty("config");
    expect(call.args.profile).not.toHaveProperty("password");
  }
  expect(results.profiles.find((p: any) => p.id === "p0").hasConfig).toBe(true);
  expect(results.profiles.find((p: any) => p.id === "p1").hasPassword).toBe(true);
  await card("Fortinet metadata").getByRole("button", { name: "Düzenle", exact: true }).click();
  await dialog.getByRole("checkbox", { name: "Parolayı şifreli kasada hatırla" }).uncheck();
  await dialog.getByRole("button", { name: "Kaydet", exact: true }).click();
  await expect(dialog).toBeHidden();
  results = await page.evaluate(() => ({ calls: (window as any).vpnTest.calls.filter((c: any) => c.command === "vpn_profile_save"), profiles: (window as any).vpnTest.profiles }));
  expect(results.calls.at(-1).args.profile.password).toBe("");
  expect(results.profiles.find((p: any) => p.id === "p1").hasPassword).toBe(false);
});

for (const family of ["openVpn", "wireGuard"]) test(`native file drop detects ${family} without exposing configuration`, async ({page}) => {
  await setup(page);
  await page.evaluate(family => { (window as any).vpnTest.importResult = {family,token:"drop-token",warnings:[],missingFiles:[],blocked:false}; },family);
  const box=await page.locator(".vpn-workspace").boundingBox();
  await page.evaluate(({x,y}) => (window as any).vpnTest.drop(["/private/İstanbul.conf"], {x:x*devicePixelRatio,y:y*devicePixelRatio}), {x:box!.x+80,y:box!.y+80});
  const dialog=page.getByRole("dialog",{name:"VPN profilini kaydet"});
  await expect(dialog.getByLabel("VPN türü")).toHaveValue(family);
  await expect(dialog).toContainText("İstanbul.conf");
  await expect(dialog.getByLabel("Parolayı şifreli kasada hatırla")).toBeChecked();
  await dialog.getByRole("button",{name:"Vazgeç",exact:true}).click();
  expect(await page.evaluate(()=>(window as any).vpnTest.calls.some((c:any)=>c.command==="vpn_profile_save"))).toBe(false);
});
test("native drop rejects multiple files, wrong extensions and missing dependencies",async({page})=>{
  await setup(page);
  const box=await page.locator(".vpn-workspace").boundingBox();
  const drop=async(paths:string[])=>page.evaluate(({paths,x,y})=>(window as any).vpnTest.drop(paths,{x:x*devicePixelRatio,y:y*devicePixelRatio}),{paths,x:box!.x+80,y:box!.y+80});
  await drop(["a.ovpn","b.ovpn"]); await expect(page.getByRole("alert")).toContainText("Tek bir");
  await drop(["script.exe"]); await expect(page.getByRole("alert")).toContainText(".conf");
  await page.evaluate(()=>(window as any).vpnTest.importResult={token:"blocked",family:"openVpn",warnings:[],missingFiles:["missing-ca.pem"],blocked:false});
  await drop(["a.ovpn"]); await expect(page.getByRole("alert")).toContainText("missing-ca.pem");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

for (const development of [true, false]) test(`blocked native import names the selected file, exposes only supplied development diagnostics and clears feedback on retry (${development})`, async ({page}) => {
  await setup(page);
  await page.getByRole("button", {name:"Yeni VPN",exact:true}).click();
  const dialog = page.getByRole("dialog", {name:"VPN profilini kaydet"});
  await dialog.getByLabel("VPN türü").selectOption("openVpn");
  await page.evaluate(development => {
    const t = (window as any).vpnTest;
    t.importPath = "C:\\private-disposable\\İstanbul.ovpn";
    t.importResult = {token:"blocked",family:"openVpn",warnings:["Satır 7: script-security desteklenmiyor; yürütülmedi"],missingFiles:[],blocked:true,
      ...(development ? {diagnostics:{sha256:"2c79ce148e98d550274138c01503301630f32678ca6e2fdacbb58e852560feaa",byteLength:83}} : {})};
  }, development);
  await dialog.getByRole("button", {name:".ovpn / .conf içe aktar"}).click();
  const alert = dialog.getByRole("alert");
  await expect(alert).toContainText("İstanbul.ovpn: Dosya içe aktarılamadı.");
  await expect(alert).toContainText("Satır 7: script-security");
  await expect(alert).not.toContainText("private-disposable");
  if (development) await expect(alert).toContainText("Tanı: SHA-256 2c79ce148e98d550274138c01503301630f32678ca6e2fdacbb58e852560feaa; 83 bayt.");
  else await expect(alert).not.toContainText("SHA-256");
  await page.evaluate(() => { (window as any).vpnTest.importResult = null; });
  await dialog.getByRole("button", {name:".ovpn / .conf içe aktar"}).click();
  await expect(dialog.getByText("İstanbul.ovpn", {exact:true})).toBeVisible();
  await expect(alert).toHaveCount(0);
  await expect(dialog.getByRole("status")).toContainText("Yapılandırma güvenli biçimde hazırlandı.");
  expect(await page.evaluate(() => (window as any).vpnTest.calls.some((c:any) => c.command === "vpn_profile_save"))).toBe(false);
});

test("closing an import dialog discards a delayed native import result",async({page})=>{
  await setup(page);
  await page.getByRole("button",{name:"Yeni VPN",exact:true}).click();
  const dialog=page.getByRole("dialog",{name:"VPN profilini kaydet"});
  await dialog.getByLabel("VPN türü").selectOption("openVpn");
  await page.evaluate(()=>{const t=(window as any).vpnTest;t.importResult=new Promise(resolve=>t.finishImport=()=>resolve({family:"openVpn",token:"late",warnings:[],missingFiles:[],blocked:false}));});
  await dialog.getByRole("button",{name:".ovpn / .conf içe aktar"}).click();
  await dialog.locator(":scope > header button").click();
  await page.evaluate(()=>(window as any).vpnTest.finishImport());
  await expect(dialog).toHaveCount(0);
  expect(await page.evaluate(()=>(window as any).vpnTest.calls.some((c:any)=>c.command==="vpn_profile_save"))).toBe(false);
});
