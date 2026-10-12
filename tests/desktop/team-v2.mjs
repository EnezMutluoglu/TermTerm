// Real Windows WebView + Tauri IPC + TLS service + PostgreSQL, disposable accounts.

import { browser, $, $$, expect } from "@wdio/globals";

import assert from "node:assert/strict";

import fs from "node:fs/promises";

import path from "node:path";

import crypto from "node:crypto";

import https from "node:https";
import http from "node:http";
import net from "node:net";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import { proxy } from "./proxy-fixtures.mjs";
import { doubleClick, contextMenu } from "./interaction.mjs";

const password = "Native-Team-Lab-Password-2026!";

let fixture, vault, folder, host, recoveryFile;

async function invoke(command, args = {}) {
  await browser.waitUntil(
    async () => !(await $('.ts-workspace main[aria-busy="true"]').isExisting()),
  );

  const reply = await browser.executeAsync(
    (command, args, done) =>
      window.__TAURI__.core.invoke(command, args).then(
        (value) => done({ value }),
        error => done({ failed: true, error: String(error) }),
      ),
    command,
    args,
  );

  if (reply.failed) {
    await fs.writeFile('.lab/team-v2-http/native-ipc-error.json',JSON.stringify({command,error:reply.error}));
    throw Error("Native Team call rejected; private diagnostic recorded");
  }
  return reply.value;
}

const action = (a) => invoke("team_v2_action", { action: a });

async function externalEdit(change) {
  const {vaultId,id,expectedRevision,content}=change;
  return await new Promise((resolve,reject)=>{
    const child=spawn(path.resolve("team-client/target/debug/examples/native_test_writer.exe"),[],{windowsHide:true,stdio:["pipe","pipe","pipe"]});
    let output="";child.stdout.on("data",chunk=>output+=chunk);child.stderr.resume();
    child.on("error",()=>reject(Error("Disposable writer unavailable")));
    child.on("exit",code=>{if(code!==0)reject(Error("Disposable writer rejected"));else {try{resolve(JSON.parse(output));}catch{reject(Error("Invalid writer receipt"));}}});
    child.stdin.end(JSON.stringify({profile:fixture.profile,username:"leader",password,vaultId,id,expectedRevision,content}));
  });
}

async function button(text) {
  if (text === "Ortak kasalar") text = "Hosts";
  if (
    [
      "Üyeler ve yetkiler",
      "Bekleyen kararlar",
      "Kurtarma anahtarı kur",
    ].includes(text)
  ) {
    const candidate = await $(`button=${text}`);
    if (!(await candidate.isExisting()) || !(await candidate.isDisplayed()))
      await $(".main-nav").$("button=Team").click();
  }
  if (
    text === "İstanbul ortak kasa" &&
    (await $(".vault-selector").isExisting()) &&
    (await $(".vault-selector").getText()).includes(text)
  )
    return;
  let el = await $(`button=${text}`);
  if (text === "Team oturumunu kapat")
    el = await $('button[aria-label="Team oturumunu kapat"]');
  else if (!(await el.isExisting())) el = await $(`button*=${text}`);
  await el.waitForClickable();
  await el.click();
}

async function selectValue(element, value) {
  await browser.execute(
    (el, value) => {
      el.value = value;
      el.dispatchEvent(new Event("change", { bubbles: true }));
    },
    element,
    value,
  );
}

async function loginFields(pw) {
  await browser.waitUntil(
    async () => (await $$(".ts-login input")).length >= 4,
  );
  const fields = await $$(".ts-login input");

  await fields[0].setValue(fixture.profile.serviceUrl);
  await fields[1].setValue(fixture.profile.teamId);
  await fields[2].setValue(fixture.username);

  await $(".ts-login summary").click();
  await $(".ts-login textarea").setValue(fixture.profile.caPem);
  await fields[3].setValue(pw);
  await button("Giriş yap");
  await browser.waitUntil(
    async () => {
      if (await $(".ts-workspace").isExisting()) return true;
      return (
        (await $(".ts-login h1").isExisting()) &&
        (await $(".ts-login h1").getText()) === "Hesabını etkinleştir"
      );
    },
    { timeout: 30000 },
  );
}

async function httpRequest(endpoint, body, token) {
  return await new Promise((resolve, reject) => {
    const req = https.request(
      fixture.profile.serviceUrl + endpoint,
      {
        method: "POST",
        ca: fixture.profile.caPem,
        headers: {
          "content-type": "application/json",
          "x-termterm-client-version": "0.4.1", "x-termterm-api-version": "1",
          ...(token ? { authorization: "Bearer " + token } : {}),
        },
      },
      (res) => {
        let data = "";
        res.on("data", (c) => (data += c));
        res.on("end", () => {
          if (res.statusCode !== 200)
            return reject(Error("Lab HTTP request failed " + res.statusCode));
          resolve(JSON.parse(data));
        });
      },
    );
    req.on("error", reject);
    req.end(JSON.stringify(body));
  });
}
async function readHttp(port) {
  return await new Promise((resolve, reject) => {
    const r = http.get(
      { host: "127.0.0.1", port, path: "/health", timeout: 5000 },
      (res) => {
        let body = "";
        res.on("data", (c) => (body += c));
        res.on("end", () => resolve(body));
      },
    );
    r.on("timeout", () => r.destroy(Error("Tunnel timeout")));
    r.on("error", reject);
  });
}
async function socksHttp(port, targetPort) {
  const socket = net.createConnection({ host: "127.0.0.1", port });
  socket.setTimeout(5000, () => socket.destroy(Error("SOCKS timeout")));
  const next = () =>
    new Promise((resolve, reject) => {
      socket.once("data", resolve);
      socket.once("error", reject);
    });
  try {
    await new Promise((resolve, reject) => {
      socket.once("connect", resolve);
      socket.once("error", reject);
    });
    let p = next();
    socket.write(Buffer.from([5, 1, 0]));
    assert.deepEqual([...(await p)], [5, 0]);
    p = next();
    socket.write(
      Buffer.from([
        5,
        1,
        0,
        1,
        127,
        0,
        0,
        1,
        targetPort >> 8,
        targetPort & 255,
      ]),
    );
    const reply = await p;
    assert.equal(reply[1], 0);
    const result = new Promise((resolve, reject) => {
      let text = "";
      socket.on("data", (c) => (text += c));
      socket.once("end", () => resolve(text));
      socket.once("error", reject);
    });
    socket.write(
      "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    return await result;
  } finally {
    socket.destroy();
  }
}

describe("Native Team Service workspace", () => {
  before(async () => {
    fixture = JSON.parse(
      await fs.readFile(".lab/team-v2-http/windows-native.json", "utf8"),
    );
    await fs.mkdir("artifacts/team-v2", { recursive: true });
  });

  after(async () => {
    await invoke("team_v2_logout").catch(() => {});
  });

  it("activates a leader and signs in inside the desktop window", async () => {
    await button("Team hesabı");
    await loginFields(fixture.password);

    await expect($(".ts-login h1")).toHaveText("Hesabını etkinleştir");

    const fields = await $$(".ts-login input");
    await fields[0].setValue(password);
    await fields[1].setValue(password);
    await button("Parolayı belirle");

    await expect($(".ts-login h1")).toHaveText("Team hesabına giriş");
    await loginFields(password);

    await expect($(".ts-workspace")).toBeDisplayed();

    assert.equal((await action({ kind: "overview" })).role, "owner");
  });

  it("remembers Team credentials in Windows protected storage without returning the password",async()=>{
    const session=await invoke("team_v2_login",{profile:fixture.profile,username:fixture.username,password,remember:true});
    assert.equal(session.activationRequired,false);assert.equal(session.rememberWarning,undefined);
    await invoke("team_v2_logout");
    const saved=await invoke("team_v2_saved_login");
    assert.equal(saved.remembered,true);assert.equal(saved.username,fixture.username);
    assert.equal(saved.profile.teamId,fixture.profile.teamId);
    assert(!JSON.stringify(saved).includes(password));
    assert.equal((await invoke("team_v2_login_remembered")).activationRequired,false);
    assert.equal((await action({kind:"overview"})).role,"owner");
    await invoke("team_v2_forget_login");
    assert.equal(await invoke("team_v2_saved_login"),null);
    await assert.rejects(invoke("team_v2_login_remembered"));
    // Forgetting local convenience credentials must not revoke the live account.
    assert.equal((await action({kind:"overview"})).role,"owner");
  });

  it("creates a vault and folder/host records, with folder navigation", async () => {
    await button("Kasa oluştur");
    await $(".ts-dialog input").setValue("İstanbul ortak kasa");
    await button("Kaydet");

    await button("İstanbul ortak kasa");

    await button("Klasör");
    await $(".ts-dialog input").setValue("Üretim");
    await button("Kaydet");

    await button("Üretim");
    await button("Host");

    const fields = await $$(".ts-dialog input");
    await fields[0].setValue("PostgreSQL laboratuvarı");
    await fields[1].setValue("127.0.0.1");
    await browser.saveScreenshot("artifacts/team-v2/windows-host-create.png");
    await button("Kaydet");

    await expect($(".record-card")).toBeDisplayed();
    await contextMenu(await $('.record-card'));
    await button('Düzenle');
    await $('.ts-dialog input').waitForDisplayed();
    await selectValue(await $('select[aria-label="Proxy türü"]'), 'http');
    await $('input[aria-label="Proxy adresi"]').setValue('proxy.example.internal');
    await browser.execute(el=>{const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set;setter.call(el,"8080");el.dispatchEvent(new Event("input",{bubbles:true}));},await $('input[aria-label="Proxy portu"]'));
    await $('.ts-dialog h2').click();
    await browser.execute(()=>document.querySelector('.ts-dialog').scrollTop=0);
    await browser.saveScreenshot("artifacts/team-v2/windows-host-edit.png");
    await $('.ts-dialog button[aria-label="Kapat"]').click();

    await browser.saveScreenshot(
      path.resolve("artifacts/team-v2/windows-folder.png"),
    );

    const vs = await action({ kind: "vaults" });
    vault = vs[0].id;

    const rows = (
      await action({ kind: "resources", vaultId: vault, after: null })
    ).records;

    folder = rows.find((r) => r.content.kind === "folder").id;
    host = rows.find((r) => r.content.kind === "host").id;

    await $(".ts-breadcrumb").$("button*=İstanbul").click();
    await expect($(".record-card")).not.toBeDisplayed();
  });

  it("leader-created member and native IPC have scoped access", async () => {
    await button("Üyeler ve yetkiler");
    await button("Kullanıcı oluştur");

    const fields = await $$(".ts-dialog input");
    await fields[0].setValue("Laboratuvar üyesi");
    await fields[1].setValue("native.user");
    await button("Kaydet");

    await $('input[aria-label="Geçici parola"]').waitForDisplayed();

    const temporaryPassword = await $(
      'input[aria-label="Geçici parola"]',
    ).getValue();
    await button("Kaydettim, kapat");

    const member = (await action({ kind: "members" })).find(
      (m) => m.username === "native.user",
    );

    const created = { accountId: member.accountId, temporaryPassword };

    assert.equal(created.temporaryPassword.length, 21);

    // A separate native session activates the member; no self-registration endpoint.

    await invoke("team_v2_logout");

    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "native.user",
      password: created.temporaryPassword,
    });

    await action({ kind: "activate", password });

    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "native.user",
      password,
    });

    assert.deepEqual(await action({ kind: "vaults" }), []);

    await assert.rejects(action({ kind: "members" }));

    await assert.rejects(
      action({ kind: "members", actorId: created.accountId }),
    );

    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "leader",
      password,
    });

    await button("Ortak kasalar");

    const plan = await action({
      kind: "prepareAccess",
      accountId: created.accountId,
      resourceId: folder,
      changes: [
        { permission: "read", effect: "allow" },
        { permission: "connect", effect: "allow" },
      ],
    });

    assert.equal(plan.affectedResources, 2);

    assert.equal(
      (await action({ kind: "accessImpact", planId: plan.id, after: null }))
        .records.length,
      2,
    );

    let after = 0;
    do {
      after = (await action({ kind: "stageAccess", planId: plan.id, after }))
        .nextAfter;
    } while (after !== null);

    await action({ kind: "finishAccess", planId: plan.id, cancel: false });

    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "native.user",
      password,
    });

    assert.equal(
      (await action({ kind: "vaults" }))[0].name,
      "İstanbul ortak kasa",
    );

    const rows = (
      await action({ kind: "resources", vaultId: vault, after: null })
    ).records;

    assert.equal(rows.find((r) => r.id === host).content.address, "127.0.0.1");

    assert(!JSON.stringify(rows).includes("accessToken"));

    await invoke("team_v2_logout");
    await assert.rejects(action({ kind: "vaults" }));
  });

  it("shows a real permission matrix without applying it until approved", async () => {
    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "leader",
      password,
    });

    await button("Ortak kasalar");

    await $('button[aria-label="Yenile"]').click();

    await browser.waitUntil(
      async () => !(await $(".ts-progress").isExisting()),
    );

    // Embedded WDIO omits contextmenu (same limitation documented in context-menu.mjs).

    await browser.execute(
      (el) => {
        const r = el.getBoundingClientRect();
        el.dispatchEvent(
          new MouseEvent("contextmenu", {
            bubbles: true,
            cancelable: true,
            button: 2,
            clientX: r.x + 35,
            clientY: r.y + 25,
          }),
        );
      },
      await $("button*=Üretim"),
    );

    await button("Üye izinleri");

    const members = await action({ kind: "members" });
    const account = members.find((m) => m.username === "native.user");

    await selectValue(await $(".ts-dialog select"), account.accountId);

    const selects = await $$(".ts-permissions select");
    await selectValue(selects[2], "allow");

    await button("Değişikliği önizle");
    await expect($(".ts-dialog table")).toBeDisplayed();

    await browser.saveScreenshot(
      path.resolve("artifacts/team-v2/windows-permissions.png"),
    );

    await button("İptal");
    await button("Üyeler ve yetkiler");

    await expect($(".ts-browse > table")).toBeDisplayed();

    await browser.saveScreenshot(
      path.resolve("artifacts/team-v2/windows-members.png"),
    );
  });

  it("creates an encrypted recovery file and saves a private password without exposing it in list IPC", async () => {
    const recovery = path.resolve(
      ".lab/team-v2-http/native-" + crypto.randomUUID() + ".ttrecovery",
    );

    recoveryFile = recovery;
    const result = await action({
      kind: "createRecovery",
      path: recovery,
      password: "Recovery-Native-Lab-2026!",
      reauthentication: password,
    });

    assert(result.keyId);
    const saved = await fs.readFile(recovery);

    await assert.rejects(
      action({
        kind: "createRecovery",
        path: recovery,
        password: "Recovery-Native-Lab-2026!",
        reauthentication: password,
      }),
    );

    assert(saved.equals(await fs.readFile(recovery)));

    await button("Kimliklerim");
    await button("Özel kimlik ekle");

    await $(".ts-dialog input").setValue("Kişisel laboratuvar parolası");
    await selectValue(await $(".ts-dialog select"), "password");

    await $('.ts-dialog input[type="password"]').waitForDisplayed();

    await $('.ts-dialog input[type="password"]').setValue(
      "Disposable-SSH-Password-31!",
    );
    await button("Kaydet");

    await expect($("h3*=Kişisel laboratuvar parolası")).toBeDisplayed();

    const rows = await action({ kind: "credentials" });
    assert.equal(rows.length, 1);
    assert(!JSON.stringify(rows).includes("Disposable-SSH-Password-31!"));

    await contextMenu($(`[data-identity-id="${rows[0].id}"]`));
    await button("Varsayılan yap");
    assert.equal(
      (await action({ kind: "profiles" })).default.credentialId,
      rows[0].id,
    );

    await browser.saveScreenshot(
      path.resolve("artifacts/team-v2/windows-identities.png"),
    );
  });


  for (const kind of ["http","socks5"]) it(`uses authenticated Team ${kind} proxy and historical chain routes`, async () => {
    const ssh=JSON.parse(await fs.readFile(".lab/team-v2-http/ssh-native.json","utf8"));
    const key=crypto.randomUUID();
    await action({kind:"saveCredential",id:key,expectedRevision:0,credentialKind:"ssh_key",label:"Disposable proxy test",secret:await fs.readFile("tests/ssh-keys/rsa2048.pkcs1","utf8"),passphrase:null});
    await browser.executeAsync(done=>{
      window.__proxyStops?.forEach(stop=>stop());
      window.__proxyEvents=[];window.__proxyPass="proxy-pass";window.__proxyPrompts=0;
      Promise.all([
        window.__TAURI__.event.listen("session-event",e=>window.__proxyEvents.push(e.payload)),
        window.__TAURI__.event.listen("tunnel-event",e=>window.__proxyEvents.push({...e.payload,kind:"tunnel"})),
        window.__TAURI__.event.listen("session-prompt",e=>{
          if(e.payload.kind==="hostKey"&&e.payload.detail.address==="[127.0.0.1]:41843")window.__TAURI__.core.invoke("prompt_answer",{id:e.payload.id,answers:["trust"]});
          if(e.payload.kind==="authentication" && e.payload.detail.prompts?.[0]?.prompt==="Proxy kullanıcı adı"){window.__proxyPrompts++;window.__TAURI__.core.invoke("prompt_answer",{id:e.payload.id,answers:["test",window.__proxyPass]});}
        })
      ]).then(stops=>{window.__proxyStops=stops;done();});
    });
    {
      const server=await proxy(kind,ssh.port,ssh.host), root=crypto.randomUUID(), hop=crypto.randomUUID();
      let session,sftp,tunnel,remote;
      try {
        for(const id of [hop,root]) {
          await action({kind:"saveResource",vaultId:vault,id,expectedRevision:0,clientAt:new Date().toISOString(),content:{kind:"host",parentId:null,name:`${kind} ${id===hop?"jump":"target"}`,address:ssh.host,port:ssh.port,protocol:"ssh",usernamePolicy:"shared_required",username:ssh.username,deleted:false,proxy:id===hop?{kind,host:"127.0.0.1",port:server.port,authenticate:true}:null}});
          await action({kind:"saveProfile",selection:{hostId:id,expectedRevision:0,method:"credential",credentialId:key,username:null}});
        }
        console.info("Team proxy prepare chain",kind);
        const chain=await action({kind:"prepareChain",id:root,hops:[hop]});
        await action({kind:"finishChain",planId:chain.planId,cancel:false});
        const chained=(await action({kind:"resource",id:root})).revision;
        const direct=await action({kind:"prepareChain",id:root,hops:[]});
        await action({kind:"finishChain",planId:direct.planId,cancel:false});
        console.info("Team proxy restore",kind);
        const current=(await action({kind:"resource",id:root})).revision;
        const restore=await action({kind:"prepareRestore",id:root,sourceRevision:chained,currentRevision:current});
        assert.equal(restore.hops.length,1);
        await action({kind:"finishChain",planId:restore.planId,cancel:false});
        assert.equal((await action({kind:"connectionProfiles",id:root})).length,2);
        console.info("Team proxy save proxy",kind);
        const endpoint={kind,host:"127.0.0.1",port:server.port,authenticate:true};
        await action({kind:"saveProxy",root,hostId:hop,credentialId:null,revision:0,secret:{endpoint,username:"test",password:"proxy-pass"}});
        const saved=await action({kind:"proxyStatus",root,hostId:hop});
        assert.equal(saved.username,"test");assert(!JSON.stringify(saved).includes("proxy-pass"));
        console.info("Team proxy fresh login",kind);
        await invoke("team_v2_logout");
        const f=JSON.parse(await fs.readFile(".lab/team-v2-http/windows-native.json","utf8"));
        await invoke("team_v2_login",{profile:f.profile,username:"leader",password});
        const promptCount=await browser.execute(()=>window.__proxyPrompts);
        console.info("Team proxy SSH",kind);
        session=await invoke("team_v2_start",{hostId:root});
        await browser.waitUntil(async()=>await browser.execute(id=>window.__proxyEvents.some(e=>e.id===id&&e.kind==="connected"),session),{timeout:20000});
        await invoke("session_input",{id:session,data:"printf '\\124\\124\\137\\120\\122\\117\\130\\131\\137\\117\\113\\n'\n"});
        await browser.waitUntil(async()=>await browser.execute(id=>window.__proxyEvents.filter(e=>e.id===id&&e.kind==="data").map(e=>atob(e.detail)).join("").includes("TT_PROXY_OK"),session),{timeout:15000});
        console.info("Team proxy SFTP",kind);
        sftp=crypto.randomUUID();
        const home=await invoke("team_v2_sftp",{id:sftp,hostId:root});
        remote={connection:sftp,path:home+"/.tt-proxy-"+crypto.randomUUID()};
        const source=path.resolve(".lab/team-v2-http/proxy-"+crypto.randomUUID()),dest=source+".download";
        const payload=crypto.randomBytes(4096);await fs.writeFile(source,payload);
        await invoke("file_transfer",{id:crypto.randomUUID(),source:{connection:"local",path:source},dest:remote,overwrite:false});
        await invoke("file_transfer",{id:crypto.randomUUID(),source:remote,dest:{connection:"local",path:dest},overwrite:false});
        assert.deepEqual(await fs.readFile(dest),payload);
        console.info("Team proxy local tunnel",kind);
        tunnel=crypto.randomUUID();
        await invoke("team_v2_tunnel",{id:tunnel,tunnel:{hostId:root,mode:"local",bindAddress:"127.0.0.1",bindPort:0,targetAddress:"127.0.0.1",targetPort:41841}});
        const port=await browser.execute(id=>window.__proxyEvents.find(e=>e.id===id&&e.kind==="tunnel"&&e.status==="running")?.port,tunnel);
        assert((await readHttp(port)).includes("alive"));
        await invoke("tunnel_stop",{id:tunnel});tunnel=crypto.randomUUID();
        await invoke("team_v2_tunnel",{id:tunnel,tunnel:{hostId:root,mode:"dynamic",bindAddress:"127.0.0.1",bindPort:0,targetAddress:"127.0.0.1",targetPort:41841}});
        const socksPort=await browser.execute(id=>window.__proxyEvents.find(e=>e.id===id&&e.kind==="tunnel"&&e.status==="running")?.port,tunnel);
        assert((await socksHttp(socksPort,41841)).includes("alive"));

        await invoke("tunnel_stop",{id:tunnel});tunnel=null;
        console.info("Team proxy remote tunnel",kind);
        const echo=http.createServer((req,res)=>res.end("PROXY_REMOTE_OK"));
        await new Promise(resolve=>echo.listen(0,"127.0.0.1",resolve));
        try {
          tunnel=crypto.randomUUID();
          await invoke("team_v2_tunnel",{id:tunnel,tunnel:{hostId:root,mode:"remote",bindAddress:"127.0.0.1",bindPort:0,targetAddress:"127.0.0.1",targetPort:echo.address().port}});
          const reverse=await browser.execute(id=>window.__proxyEvents.find(e=>e.id===id&&e.kind==="tunnel"&&e.status==="running")?.port,tunnel);
          const response=await promisify(execFile)("wsl",["-d","Ubuntu-24.04","-u","root","--","curl","--fail","--silent","--max-time","10",`http://127.0.0.1:${reverse}/`]);
          assert.equal(response.stdout,"PROXY_REMOTE_OK");
        }finally{if(tunnel)await invoke("tunnel_stop",{id:tunnel});tunnel=null;await new Promise(resolve=>echo.close(resolve));}
        console.info("Team proxy verify no prompts",kind);
        assert.equal(await browser.execute(()=>window.__proxyPrompts),promptCount,"Saved proxy must not prompt after fresh login");
        console.info("Team proxy forget",kind);
        await action({kind:"saveProxy",root,hostId:hop,credentialId:saved.credentialId,revision:saved.revision,secret:null});
        assert.equal((await action({kind:"proxyStatus",root,hostId:hop})).credentialId,null);
        await browser.execute(()=>window.__proxyPass="wrong-password");
        console.info("Team proxy wrong password",kind);
        const rejected=await invoke("team_v2_start",{hostId:root});
        await browser.waitUntil(async()=>await browser.execute(id=>window.__proxyEvents.some(e=>e.id===id&&e.kind==="error"),rejected),{timeout:20000});
        assert.equal(await browser.execute(id=>window.__proxyEvents.some(e=>e.id===id&&e.kind==="connected"),rejected),false);
        await invoke("session_input",{id:rejected,close:true}).catch(()=>{});
      } finally {
        await browser.execute(()=>window.__proxyPass="proxy-pass");
        if(remote)await invoke("file_action",{endpoint:remote,action:"remove"}).catch(()=>{});
        if(sftp)await invoke("sftp_disconnect",{id:sftp}).catch(()=>{});
        if(tunnel)await invoke("tunnel_stop",{id:tunnel}).catch(()=>{});
        if(session)await invoke("session_input",{id:session,close:true}).catch(()=>{});
        await action({kind:"selectHistory",id:root,revision:null}).catch(()=>{});
        server.close();
      }
    }
  });


  it("authenticates saved secrets and certificate/key/password MFA without silent fallback", async function () {
    this.timeout(180000);
    const lab=spawn(path.resolve(process.platform==="win32"?".tools/auth-venv/Scripts/python.exe":".tools/auth-venv/bin/python"),[path.resolve("tests/ssh_auth_lab.py"),path.resolve("tests/ssh-keys/rsa4096.pkcs1")],{windowsHide:true,stdio:["pipe","pipe","pipe"]});
    lab.stderr.resume();
    const fixture=await new Promise((resolve,reject)=>{
      let text="";const timer=setTimeout(()=>reject(Error("Auth laboratory did not start")),15000);
      lab.on("error",()=>{clearTimeout(timer);reject(Error("Auth laboratory unavailable"));});
      lab.stdout.on("data",chunk=>{text+=chunk;if(text.includes("\n")){clearTimeout(timer);try{resolve(JSON.parse(text.split("\n")[0]));}catch{reject(Error("Invalid lab response"));}}});
    });
    const sessions=[];let unlisten;
    try {
      await browser.executeAsync((port,done)=>{
        window.__authEvents=[];window.__authPrompts=[];window.__authMode="correct";
        Promise.all([
          window.__TAURI__.event.listen("session-event",e=>window.__authEvents.push(e.payload)),
          window.__TAURI__.event.listen("session-prompt",e=>{
            if(e.payload.kind==="hostKey" && e.payload.detail.address===`[127.0.0.1]:${port}`)window.__TAURI__.core.invoke("prompt_answer",{id:e.payload.id,answers:["trust"]});
            if(e.payload.kind!=="authentication")return;
            const prompts=e.payload.detail.prompts;window.__authPrompts.push({session:e.payload.sessionId,count:prompts.length});
            let answers=prompts.map(p=>p.prompt==="One-time code"?"654321":p.prompt==="Confirmation"?"confirm":"Disposable-Auth-Password-2026!");
            if(window.__authMode==="cancel")answers=[];
            if(window.__authMode==="wrong")answers=answers.map(()=>"wrong-code");
            window.__TAURI__.core.invoke("prompt_answer",{id:e.payload.id,answers});
          })
        ]).then(stops=>{window.__authStops=stops;done();});
      },fixture.port);
      unlisten=true;
      async function credential(kind,name,secret,passphrase=null,certificate=null) {
        const id=crypto.randomUUID();await action({kind:"saveCredential",id,expectedRevision:0,credentialKind:kind,label:name,secret,passphrase,certificate});return id;
      }
      const key=await credential("ssh_key","Encrypted disposable RSA",await fs.readFile("tests/ssh-keys/rsa4096.encrypted.pkcs8","utf8"),"termterm-disposable-test");
      const cert=await credential("ssh_key","Disposable certificate",await fs.readFile("tests/ssh-keys/rsa4096.pkcs1","utf8"),null,fixture.certificate);
      const badKey=await credential("ssh_key","Rejected disposable key",await fs.readFile("tests/ssh-keys/ed25519.pkcs8","utf8"));
      const pass=await credential("password","Saved lab password","Disposable-Auth-Password-2026!");
      const badPass=await credential("password","Rejected lab password","Wrong-Disposable-Password!");
      async function hostFor(user,id,method="credential") {
        const hostId=crypto.randomUUID();
        await action({kind:"saveResource",vaultId:vault,id:hostId,expectedRevision:0,clientAt:new Date().toISOString(),content:{kind:"host",parentId:null,name:`Auth ${user}`,address:"127.0.0.1",port:fixture.port,protocol:"ssh",usernamePolicy:"shared_required",username:user,deleted:false}});
        await action({kind:"saveProfile",selection:{hostId,expectedRevision:0,method,credentialId:id,username:null}});return hostId;
      }
      async function verify(hostId,success,prompts,mode="correct") {
        await browser.execute(mode=>window.__authMode=mode,mode);
        const id=await invoke("team_v2_start",{hostId});sessions.push(id);
        await browser.waitUntil(async()=>await browser.execute(id=>window.__authEvents.some(e=>e.id===id&&(e.kind==="connected"||e.kind==="error")),id),{timeout:20000});
        const state=await browser.execute(id=>({connected:window.__authEvents.some(e=>e.id===id&&e.kind==="connected"),prompts:window.__authPrompts.filter(p=>p.session===id).length,errors:window.__authEvents.filter(e=>e.id===id&&e.kind==="error").map(e=>e.detail)}),id);
        assert.equal(state.connected,success,JSON.stringify(state));assert.equal(state.prompts,prompts);
        if(success)await browser.waitUntil(async()=>await browser.execute(id=>window.__authEvents.filter(e=>e.id===id&&e.kind==="data").map(e=>atob(e.detail)).join("").includes("TT_AUTHENTICATED"),id));
        await invoke("session_input",{id,close:true}).catch(()=>{});
      }
      const stored=await hostFor("password",pass);
      await verify(stored,true,0);
      await verify(await hostFor("key",key),true,0);
        await verify(await hostFor("key_password",key),true,1);
      await verify(await hostFor("cert_mfa",cert),true,2);
      const mfa=await hostFor("key_mfa",key);await verify(mfa,true,2);
      await verify(await hostFor("password_mfa",pass),true,2);
      await verify(await hostFor("interactive",null,"keyboard_interactive"),true,2);
      await verify(await hostFor("password_mfa",null,"prompt"),true,3);
      await verify(await hostFor("key_mfa",badKey),false,0);
      await verify(await hostFor("password_mfa",badPass),false,0);
      await verify(mfa,false,1,"wrong");await verify(mfa,false,1,"cancel");
      const chained=await hostFor("password_mfa",pass);
      const plan=await action({kind:"prepareChain",id:chained,hops:[mfa]});
      await action({kind:"finishChain",planId:plan.planId,cancel:false});await verify(chained,true,4);
      // Fresh Team login reloads the selected encrypted credentials; no SSH prompt.
      await invoke("team_v2_logout");
      const f=JSON.parse(await fs.readFile(".lab/team-v2-http/windows-native.json","utf8"));
      await invoke("team_v2_login",{profile:f.profile,username:"leader",password});
      await verify(stored,true,0);
    } finally {
      for(const id of sessions)await invoke("session_input",{id,close:true}).catch(()=>{});
      if(unlisten)await browser.execute(()=>window.__authStops?.forEach(stop=>stop()));
      lab.stdin.end();await new Promise(resolve=>{if(lab.exitCode!==null)return resolve();lab.once("exit",resolve);setTimeout(()=>{lab.kill();resolve();},3000).unref();});
    }
  });

  it("edits an encrypted personal default and host override, with version checks and locked usernames", async () => {
    await button("Varsayılan bağlantı profilim");

    await $('input[aria-label="Profil kullanıcı adı"]').waitForEnabled();
    await $('input[aria-label="Profil kullanıcı adı"]').setValue(
      "kişisel_admin",
    );

    await button("Profili kaydet");
    await browser.waitUntil(async () => !(await $(".ts-dialog").isExisting()));

    let profiles = await action({ kind: "profiles" });
    assert.equal(profiles.default.username, "kişisel_admin");

    const credential = profiles.default.credentialId;

    const selection = {
      hostId: host,
      expectedRevision: 0,
      method: "credential",
      credentialId: credential,
      username: "host_admin",
    };

    await action({ kind: "saveProfile", selection });
    await assert.rejects(action({ kind: "saveProfile", selection }));

    profiles = await action({ kind: "profiles" });
    assert.equal(
      profiles.hosts.find((p) => p.hostId === host).username,
      "host_admin",
    );

    await action({
      kind: "saveProfile",
      selection: {
        ...selection,
        expectedRevision: 1,
        method: "inherit",
        credentialId: null,
        username: null,
      },
    });

    profiles = await action({ kind: "profiles" });
    assert.equal(
      profiles.hosts.find((p) => p.hostId === host).method,
      "inherit",
    );

    // An owner also cannot override a shared-required username in a personal profile.

    let current = await action({ kind: "resource", id: host });

    await action({
      kind: "saveResource",
      vaultId: vault,
      id: host,
      expectedRevision: current.revision,
      clientAt: new Date().toISOString(),
      content: {
        ...current.content,
        usernamePolicy: "shared_required",
        username: "company_admin",
      },
    });

    await assert.rejects(
      action({
        kind: "saveProfile",
        selection: { ...selection, expectedRevision: 2 },
      }),
    );

    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "native.user",
      password,
    });

    assert.deepEqual(await action({ kind: "profiles" }), {
      default: null,
      hosts: [],
    });

    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "leader",
      password,
    });

    assert.equal(
      (await action({ kind: "profiles" })).default.username,
      "kişisel_admin",
    );
  });

  it("compares concurrent edits and keeps the server version through the native decision screen", async () => {
    const old = await action({ kind: "resource", id: host });

    const change = {
      kind: "saveResource",
      vaultId: vault,
      id: host,
      expectedRevision: old.revision,
      clientAt: new Date().toISOString(),
      content: { ...old.content, name: "Sunucudaki değişiklik" },
    };

    assert.equal((await externalEdit(change)).status, "applied");

    const candidate = await action({
      ...change,localGeneration:old.localGeneration,
      content: { ...old.content, name: "Çakışan değişiklik" },
    });
    assert.equal(candidate.status, "conflict");

    await button("Bekleyen kararlar");
    await button("Çakışan değişiklik");
    await expect($(".ts-dialog table")).toBeDisplayed();

    await browser.saveScreenshot(
      path.resolve("artifacts/team-v2/windows-conflicts.png"),
    );

    // Another edit after comparison must not be overwritten by the stale decision.

    await externalEdit({
      ...change,
      expectedRevision: old.revision+1,
      content: { ...old.content, name: "Daha yeni değişiklik" },
    });

    await button("Sunucudakini koru");
    await expect($('.ts-dialog [role="alert"]')).toBeDisplayed();

    await button("Kapat");
    await button("Çakışan değişiklik");
    await button("Sunucudakini koru");

    await expect($("p*=Bekleyen karar yok.")).toBeDisplayed();

    assert.equal(
      (await action({ kind: "resource", id: host })).content.name,
      "Daha yeni değişiklik",
    );

    assert.equal(
      (await action({ kind: "conflicts", after: null })).conflicts.length,
      0,
    );
  });

  it("compares encrypted history, refuses stale restore and preserves a return point", async () => {
    const before = await action({ kind: "resource", id: host });
    const history = await action({ kind: "history", id: host });
    const oldest = history.versions.at(-1);
    await button("Ortak kasalar");
    await $('button[aria-label="Yenile"]').click();
    await browser.waitUntil(
      async () => !(await $(".ts-progress").isExisting()),
    );
    if (await $("button*=Üretim").isExisting()) await button("Üretim");
    const card = await $(".record-card");
    await card.waitForDisplayed();
    await browser.execute(
      (el) =>
        el.dispatchEvent(
          new MouseEvent("contextmenu", {
            bubbles: true,
            clientX: 550,
            clientY: 340,
          }),
        ),
      card,
    );
    await button("Kayıt geçmişi");
    await browser.waitUntil(
      async () =>
        (await $(".ts-history").getAttribute("aria-busy")) === "false",
    );
    await selectValue(
      await $('select[aria-label="Geçmiş sürüm"]'),
      String(oldest.revision),
    );
    await expect($(".ts-history table")).toBeDisplayed();
    await browser.saveScreenshot(
      path.resolve("artifacts/team-v2/windows-history.png"),
    );
    await button("Yeni revizyon olarak geri yükle");
    await $('[aria-label="Geri yükleme bağlantı önizlemesi"]').waitForDisplayed();
    const newer = await action({
      kind: "saveResource",
      vaultId: vault,
      id: host,
      expectedRevision: before.revision,
      clientAt: new Date().toISOString(),
      content: { ...before.content, name: "History concurrent edit" },
    });
    await button("Geri yüklemeyi uygula");
    await expect($('.ts-history [role="alert"]')).toBeDisplayed();
    assert.equal(
      (await action({ kind: "resource", id: host })).revision,
      newer.revision,
    );
    await $(".ts-history").$("button=Yenile").click();
    await browser.waitUntil(
      async () =>
        (await $(".ts-history").getAttribute("aria-busy")) === "false",
    );
    await button("Yeni revizyon olarak geri yükle");
    await button("Geri yüklemeyi uygula");
    await expect($('.ts-history [role="status"]')).toBeDisplayed();
    const restored = await action({ kind: "resource", id: host });
    assert.deepEqual(restored.content, oldest.content);
    assert.equal(restored.revision, newer.revision + 1);
    const h = await action({ kind: "history", id: host });
    assert.equal(h.pins[0].revision, newer.revision);
    await button("Kapat");
    await action({
      kind: "restoreVersion",
      id: host,
      sourceRevision: newer.revision,
      currentRevision: restored.revision,
      clientAt: new Date().toISOString(),
    });
    assert.equal(
      (await action({ kind: "resource", id: host })).content.name,
      "History concurrent edit",
    );
    await action({ kind: "releaseReturnPoint", id: host });
  });

  let connectionCase;
  it("prepares per-hop keys and updates shared credentials through the native screens", async () => {
    const ssh = JSON.parse(
      await fs.readFile(".lab/team-v2-http/ssh-native.json", "utf8"),
    );

    const current = await action({ kind: "resource", id: host });

    await action({
      kind: "saveResource",
      vaultId: vault,
      id: host,
      expectedRevision: current.revision,
      clientAt: new Date().toISOString(),
      content: {
        ...current.content,
        name: "SSH kişisel kimlik testi",
        address: ssh.host,
        port: ssh.port,
        usernamePolicy: "shared_required",
        username: ssh.username,
      },
    });

    await browser.executeAsync((done) => {
      window.__teamSshEvents = [];
      window.__TAURI__.event
        .listen("session-event", (e) => {
          window.__teamSshEvents.push(e.payload);
        })
        .then(() => done());
    });

    async function key(name) {
      const id = crypto.randomUUID();
      await action({
        kind: "saveCredential",
        id,
        expectedRevision: 0,
        credentialKind: "ssh_key",
        label: "Public disposable " + name,
        certificate: name === "rsa4096.pkcs1" ? ssh.certificate : null,
        secret: await fs.readFile("tests/ssh-keys/" + name, "utf8"),
        passphrase: null,
      });
      const p = (await action({ kind: "profiles" })).hosts.find(
        (p) => p.hostId === host,
      );
      await action({
        kind: "saveProfile",
        selection: {
          hostId: host,
          expectedRevision: p?.revision ?? 0,
          method: "credential",
          credentialId: id,
          username: null,
        },
      });
      return id;
    }

    async function connect(expectStoredTrust = false) {
      if (await $(".top-tabs").isExisting())
        await $(".top-tabs").$("button=Kasa").click();
      await browser.execute(() => {
        window.__teamSshEvents = [];
      });
      await button("Ortak kasalar");
      await $('button[aria-label="Yenile"]').click();
      await browser.waitUntil(
        async () => !(await $(".ts-progress").isExisting()),
      );

      if (await $("button*=Üretim").isExisting()) await button("Üretim");

      const card = await $(".ts-content .record-card");
      await card.waitForDisplayed();
      await doubleClick(card);
      if (expectStoredTrust) {
        await browser.waitUntil(
          async () => {
            assert(
              !(await $("button*=Parmak izine güven").isExisting()),
              "Known host was forgotten across logout/login",
            );
            return await browser.execute(() =>
              window.__teamSshEvents.some((e) => e.kind === "connected"),
            );
          },
          { timeout: 20000 },
        );
        return await browser.execute(
          () =>
            window.__teamSshEvents.filter((e) => e.kind === "connected").at(-1)
              .id,
        );
      }
      await browser.waitUntil(
        async () =>
          (await $("button*=Parmak izine güven").isExisting()) ||
          (await $(".ts-terminal-panel .terminal-status").isExisting()),
      );

      if (await $("button*=Parmak izine güven").isExisting())
        await button("Parmak izine güven");
      else {
        await browser.waitUntil(
          async () =>
            (await $("button*=Parmak izine güven").isExisting()) ||
            (await browser.execute(() =>
              window.__teamSshEvents.some((e) => e.kind === "connected"),
            )),
        );
        if (await $("button*=Parmak izine güven").isExisting())
          await button("Parmak izine güven");
      }

      await browser.waitUntil(
        async () =>
          await browser.execute(() =>
            window.__teamSshEvents.some((e) => e.kind === "connected"),
          ),
        { timeout: 20000 },
      );

      return await browser.execute(
        () =>
          window.__teamSshEvents.filter((e) => e.kind === "connected").at(-1)
            .id,
      );
    }

    const ownerKey = await key("rsa4096.pkcs1");
    const jumpIds=[crypto.randomUUID(),crypto.randomUUID()];
    for (const [index,id] of jumpIds.entries()) {
      await action({kind:"saveResource",vaultId:vault,id,expectedRevision:0,clientAt:new Date().toISOString(),content:{
        kind:"host",parentId:null,name:`Chain jump ${index+1}`,address:ssh.host,port:ssh.port,protocol:"ssh",usernamePolicy:"shared_required",username:ssh.username,deleted:false}});
      await action({kind:"saveProfile",selection:{hostId:id,expectedRevision:0,method:"credential",credentialId:ownerKey,username:null}});
    }
    async function setChain(hops) {
      const plan=await action({kind:"prepareChain",id:host,hops});
      await action({kind:"finishChain",planId:plan.planId,cancel:false});
    }
    await setChain(jumpIds);
    await button("Ortak kasalar");await $('button[aria-label="Yenile"]').click();
    if(await $("button*=Üretim").isExisting())await button("Üretim");
    const targetCard=await $(".record-card");await targetCard.waitForDisplayed();
    await browser.execute(el=>el.dispatchEvent(new MouseEvent("contextmenu",{bubbles:true,cancelable:true,clientX:500,clientY:220})),targetCard);
    await button("Host chain");await $('.ts-dialog select').waitForEnabled();
    await button("Erişim önizlemesi");await $('button=Onayla ve kaydet').waitForDisplayed();
    await browser.saveScreenshot("artifacts/team-v2/windows-chain.png");
    await button("Onayla ve kaydet");await $('.ts-dialog').waitForExist({reverse:true});


    await button("Kimliklerim");
    const sourceCard = await $(`[data-identity-id="${ownerKey}"]`);
    await contextMenu(sourceCard);
    await button("Ortak kimlik olarak paylaş");
    await selectValue(await $('select[aria-label="Paylaşım kapsamı"]'), folder);
    await button("Paylaşımı önizle");
    await $("button=Ortak kopyayı oluştur").waitForDisplayed();
    await browser.saveScreenshot("artifacts/team-v2/windows-sharing.png");
    await button("Vazgeç");
    await $('.ts-dialog[aria-label="Ortak kimlik paylaşımı"]').waitForExist({
      reverse: true,
    });
    const share = await action({
      kind: "prepareShare",
      sourceId: ownerKey,
      scopeId: folder,
      recipients: [],
    });
    const shared = await action({ kind: "finishShare", planId: share.id });
    await button("Kimliklerim");
    await $('button[aria-label="Yenile"]').click();
    await contextMenu($(`[data-identity-id="${shared.credentialId}"]`));
    await $("button=Alıcıları / kimliği güncelle").waitForDisplayed();
    await button("Alıcıları / kimliği güncelle");
    await $('select[aria-label="Yeni sürümün içeriği"]').waitForEnabled();
    await selectValue(
      await $('select[aria-label="Yeni sürümün içeriği"]'),
      ownerKey,
    );
    await button("Paylaşımı önizle");
    await $("button=Yeni sürümü kaydet").waitForDisplayed();
    await browser.saveScreenshot("artifacts/team-v2/windows-shared-update.png");
    await button("Yeni sürümü kaydet");
    await $('.ts-dialog[aria-label="Ortak kimlik paylaşımı"]').waitForExist({
      reverse: true,
    });
    assert.equal(
      (await action({ kind: "credentials" })).find(
        (c) => c.id === shared.credentialId,
      ).revision,
      2,
    );
    const exportPath = path.resolve(
      ".lab/team-v2-http/shared-" + crypto.randomUUID() + ".key",
    );
    try {
      await action({
        kind: "exportShared",
        id: shared.credentialId,
        path: exportPath,
      });
      assert.equal(
        await fs.readFile(exportPath, "utf8"),
        await fs.readFile("tests/ssh-keys/rsa4096.pkcs1", "utf8"),
      );
      await assert.rejects(
        action({
          kind: "exportShared",
          id: shared.credentialId,
          path: exportPath,
        }),
      );
    } finally {
      await fs.rm(exportPath, { force: true });
    }
    let profile = (await action({ kind: "profiles" })).hosts.find(
      (p) => p.hostId === host,
    );
    await action({
      kind: "saveProfile",
      selection: {
        hostId: host,
        expectedRevision: profile.revision,
        method: "credential",
        credentialId: shared.credentialId,
        username: null,
      },
    });
    connectionCase = {ownerKey,jumpIds,key,connect,setChain,ssh,shared};
  });
  it("checks shared-key revocation, real chain SFTP/tunnels, PTY resize and historical reconnect", async () => {
    const {ownerKey,jumpIds,connect,setChain,ssh,shared}=connectionCase;
    let profile;
    let session = await connect();
    await action({
      kind: "removeShared",
      id: shared.credentialId,
      expectedRevision: 2,
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) =>
            window.__teamSshEvents.some(
              (e) => e.id === id && e.kind === "closed",
            ),
          session,
        ),
      { timeout: 25000 },
    );
    assert(
      (await action({ kind: "credentials" })).some((c) => c.id === ownerKey),
    );
    profile = (await action({ kind: "profiles" })).hosts.find(
      (p) => p.hostId === host,
    );
    await action({
      kind: "saveProfile",
      selection: {
        hostId: host,
        expectedRevision: profile.revision,
        method: "credential",
        credentialId: ownerKey,
        username: null,
      },
    });
    session = await connect(true);
    const sftpId = crypto.randomUUID();
    const remoteHome = await invoke("team_v2_sftp", {
      id: sftpId,
      hostId: host,
    });
    const source = path.resolve(
      ".lab/team-v2-http/transfer-" + crypto.randomUUID() + ".txt",
    );
    const downloaded = source + ".download";
    const remote = {
      connection: sftpId,
      path: remoteHome + "/.termterm-v2-" + crypto.randomUUID(),
    };
    const payload = Buffer.from(
      "Team SFTP gerçek aktarım · Türkçe\n" +
        crypto.randomBytes(2048).toString("hex"),
    );
    await fs.writeFile(source, payload);
    try {
      assert.equal(
        await invoke("file_transfer", {
          id: crypto.randomUUID(),
          source: { connection: "local", path: source },
          dest: remote,
          overwrite: false,
        }),
        payload.length,
      );
      await invoke("file_transfer", {
        id: crypto.randomUUID(),
        source: remote,
        dest: { connection: "local", path: downloaded },
        overwrite: false,
      });
      assert.equal(
        crypto
          .createHash("sha256")
          .update(await fs.readFile(downloaded))
          .digest("hex"),
        crypto.createHash("sha256").update(payload).digest("hex"),
      );
    } finally {
      await invoke("file_action", { endpoint: remote, action: "remove" }).catch(
        () => {},
      );
      await invoke("sftp_disconnect", { id: sftpId });
    }
    await browser.executeAsync((done) => {
      window.__teamTunnelEvents = [];
      window.__TAURI__.event
        .listen("tunnel-event", (e) =>
          window.__teamTunnelEvents.push(e.payload),
        )
        .then(() => done());
    });
    async function tunnel(mode, targetPort = 41841) {
      const id = crypto.randomUUID();
      await invoke("team_v2_tunnel", {
        id,
        tunnel: {
          hostId: host,
          mode,
          bindAddress: "127.0.0.1",
          bindPort: 0,
          targetAddress: "127.0.0.1",
          targetPort,
        },
      });
      const port = await browser.execute(
        (id) =>
          window.__teamTunnelEvents.find(
            (e) => e.id === id && e.status === "running",
          )?.port,
        id,
      );
      assert(port > 0);
      return { id, port };
    }
    const local = await tunnel("local");
    try {
      assert((await readHttp(local.port)).includes("alive"));
    } finally {
      await invoke("tunnel_stop", { id: local.id });
    }
    const dynamic = await tunnel("dynamic");
    try {
      assert((await socksHttp(dynamic.port, 41841)).includes("alive"));
    } finally {
      await invoke("tunnel_stop", { id: dynamic.id });
    }
    const echo = http.createServer((req, res) =>
      res.end("TERMTERM_REMOTE_TUNNEL_OK"),
    );
    await new Promise((resolve) => echo.listen(0, "127.0.0.1", resolve));
    try {
      const reverse = await tunnel("remote", echo.address().port);
      try {
        const response = await promisify(execFile)("wsl", [
          "-d",
          "Ubuntu-24.04",
          "-u",
          "root",
          "--",
          "curl",
          "--fail",
          "--silent",
          "--max-time",
          "10",
          `http://127.0.0.1:${reverse.port}/health`,
        ]);
        assert.equal(response.stdout, "TERMTERM_REMOTE_TUNNEL_OK");
      } finally {
        await invoke("tunnel_stop", { id: reverse.id });
      }
    } finally {
      await new Promise((resolve) => echo.close(resolve));
    }

    await invoke("session_input", { id: session, cols: 117, rows: 43 });

    await invoke("session_input", {
      id: session,
      data: "printf '\\124\\105\\101\\115\\137\\122\\123\\101\\137\\117\\113\\n'; stty size\r",
    });

    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) =>
            window.__teamSshEvents
              .filter((e) => e.id === id && e.kind === "data")
              .map((e) => atob(e.detail))
              .join("")
              .includes("TEAM_RSA_OK\r\n43 117"),
          session,
        ),
      { timeout: 15000 },
    );

    await browser.saveScreenshot(
      path.resolve("artifacts/team-v2/windows-ssh.png"),
    );

    // Daily navigation uses the same upper tabs and never recreates SSH.
    const sessionCount = await browser.execute(
      () => window.__teamSshEvents.filter((e) => e.kind === "connected").length,
    );
    await $(".top-tabs").$("button=Kasa").click();
    await expect($(".record-card")).toBeDisplayed();
    await $(".top-tabs").$("button=SFTP").click();
    await expect($(".sftp-panels")).toBeDisplayed();
    const hostName=(await action({kind:'resource',id:host})).content.name;
    const rightPane=await $('[data-file-pane="1"]');
    await rightPane.$('.sftp-host-trigger').click();
    await $('.sftp-host-search input').setValue(hostName);
    await $('.sftp-host-choice').click();
    await browser.waitUntil(async()=>!!(await rightPane.$('.path-bar input').getValue()),{timeout:20000});
    assert((await $('[data-file-pane="0"] .path-bar input').getValue()).length>0);
    await rightPane.$('.sftp-host-trigger').click();
    await $('.sftp-host-search input').setValue('');
    await $('.sftp-host-browser nav').$('button=Kasa kökü').click();
    assert((await $('.sftp-host-results section').getText()).includes(hostName));
    await browser.saveScreenshot(
      path.resolve("artifacts/team-v2/windows-sftp.png"),
    );
    await $('.sftp-host-search button').click();
    const tab = await $(".top-session-tab.active");
    assert(!(await tab.isExisting()));
    await $$(".session-tab-label").then((tabs) => tabs.at(-1).click());
    await expect($(".ts-terminal-mount")).toBeDisplayed();
    // Embedded wdio1.4 special-key dispatch omits ctrlKey/shiftKey (executor.rs).
    // Explicit modifier events exercise native app routing; trusted key coverage is in Playwright.
    const nextTab = () =>
      browser.execute(() =>
        (document.activeElement ?? document.body).dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "Tab",
            code: "Tab",
            ctrlKey: true,
            bubbles: true,
            cancelable: true,
          }),
        ),
      );
    await nextTab();
    await expect($(".record-card")).toBeDisplayed();
    await nextTab();
    await expect($(".sftp-panels")).toBeDisplayed();
    await nextTab();
    await expect($(".ts-terminal-mount")).toBeDisplayed();
    assert.equal(
      await browser.execute(
        () =>
          window.__teamSshEvents.filter((e) => e.kind === "connected").length,
      ),
      sessionCount,
    );
    await invoke("session_input", {
      id: session,
      data: "printf '\\116\\101\\126\\137\\117\\113\\n'\r",
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) =>
            window.__teamSshEvents
              .filter((e) => e.id === id && e.kind === "data")
              .map((e) => atob(e.detail))
              .join("")
              .includes("NAV_OK\r\n"),
          session,
        ),
    );

    // Close the identity/session and reopen the same persistent encrypted trust store.
    // The current port is deliberately unavailable: only the pinned version can connect.
    await setChain([]);
    const good = await action({ kind: "resource", id: host });
    await action({
      kind: "saveResource",
      id: host,
      vaultId: vault,
      expectedRevision: good.revision,
      clientAt: new Date().toISOString(),
      content: { ...good.content, port: 1 },
    });
    await action({ kind: "selectHistory", id: host, revision: good.revision });
    await invoke("team_v2_logout");
    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "leader",
      password,
    });
    session = await connect(true);
    assert(
      (await action({ kind: "historySelections" })).some(
        (p) => p.resourceId === host && p.revision === good.revision,
      ),
    );
    await action({ kind: "selectHistory", id: host, revision: null });
    await action({
      kind: "restoreVersion",
      id: host,
      sourceRevision: good.revision,
      currentRevision: good.revision + 1,
      clientAt: new Date().toISOString(),
    });
    await action({ kind: "releaseReturnPoint", id: host });
    await setChain(jumpIds);
    connectionCase.tunnel=tunnel;
  });
  it("connects a second account with its own key and revokes SSH SFTP and tunnel access", async () => {
    const {ownerKey,jumpIds,key,connect,tunnel}=connectionCase;
    let session;
    await button("Team oturumunu kapat");
    await button("Team hesabı");

    await invoke("team_v2_login", {
      profile: fixture.profile,
      username: "native.user",
      password,
    });

    assert(
      !(await action({ kind: "credentials" })).some((c) => c.id === ownerKey),
    );
    const memberKey=await key("rsa2048.pkcs1");
    const memberDefault=(await action({kind:"profiles"})).default;
    await action({kind:"saveProfile",selection:{hostId:null,expectedRevision:memberDefault?.revision ?? 0,method:"credential",credentialId:memberKey,username:null}});
    for (const id of jumpIds) await assert.rejects(action({kind:"resource",id}));


    console.info("Native chain, SFTP and tunnel stage complete; checking member isolation.");
    // Complete UI sign-in as the member, using its already activated credentials.

    fixture.username = "native.user";
    await loginFields(password);

    await button("İstanbul ortak kasa");
    session = await connect();

    await invoke("session_input", {
      id: session,
      data: "printf '\\115\\105\\115\\102\\105\\122\\137\\117\\113\\n'\r",
    });

    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) =>
            window.__teamSshEvents
              .filter((e) => e.id === id && e.kind === "data")
              .map((e) => atob(e.detail))
              .join("")
              .includes("MEMBER_OK\r\n"),
          session,
        ),
    );

    const admin = await httpRequest("/v1/auth/login", {
      teamId: fixture.profile.teamId,
      username: "leader",
      password,
      deviceId: crypto.randomUUID(),
      deviceName: "Revocation lab",
    });

    const member = (await action({ kind: "overview" })).accountId;
    const memberSftp = crypto.randomUUID();
    const memberHome = await invoke("team_v2_sftp", {
      id: memberSftp,
      hostId: host,
    });
    const memberTunnel = await tunnel("local");
    assert((await readHttp(memberTunnel.port)).includes("alive"));
    assert(
      Array.isArray(
        await invoke("file_list", {
          endpoint: { connection: memberSftp, path: memberHome },
        }),
      ),
    );

    const plan = await httpRequest(
      "/v1/access/plans",
      {
        accountId: member,
        resourceId: host,
        changes: [{ permission: "connect", effect: "deny" }],
      },
      admin.accessToken,
    );

    assert.equal(plan.requiredKeys, 0);
    await httpRequest(
      `/v1/access/plans/${plan.id}/finish`,
      { cancel: false },
      admin.accessToken,
    );

    await browser.waitUntil(
      async () =>
        await browser.execute(
          (id) =>
            window.__teamSshEvents.some(
              (e) => e.id === id && e.kind === "closed",
            ),
          session,
        ),
      { timeout: 25000 },
    );

    await httpRequest("/v1/auth/logout", {}, admin.accessToken);
    await assert.rejects(
      invoke("team_v2_sftp", { id: crypto.randomUUID(), hostId: host }),
    );
    await browser.waitUntil(
      async () => {
        try {
          await invoke("file_list", {
            endpoint: { connection: memberSftp, path: memberHome },
          });
          return false;
        } catch {
          return true;
        }
      },
      { timeout: 15000 },
    );
    await browser.waitUntil(
      async () => {
        try {
          await readHttp(memberTunnel.port);
          return false;
        } catch {
          return true;
        }
      },
      { timeout: 15000 },
    );
  });
  it("reads server audit and executes bounded retention through the native management screen", async () => {
    if (await $('button[aria-label="Team oturumunu kapat"]').isExisting())
      await button("Team oturumunu kapat");
    await button("Team hesabı");
    fixture.username = "leader";
    await loginFields(password);
    await button("Üyeler ve yetkiler");
    await button("İşlem geçmişi");
    await $('section[aria-label="İşlem geçmişi"] tbody tr').waitForDisplayed();
    await $("button=Geçmişi yenile").waitForEnabled();
    assert(
      (await $('section[aria-label="İşlem geçmişi"]').getText()).includes(
        "Otomatik bakım:",
      ),
    );
    await browser.saveScreenshot("artifacts/team-v2/windows-audit.png");
    await button("Saklama süresi dolanları temizle");
    await $(
      'section[aria-label="İşlem geçmişi"] [role="status"]',
    ).waitForDisplayed();
    await $("button=Geçmişi yenile").waitForEnabled();
    assert(
      (await action({ kind: "audit", before: null })).events.some(
        (e) => e.action === "maintenance.prune",
      ),
    );
    const current = await action({ kind: "resource", id: host });
    await action({
      kind: "selectHistory",
      id: host,
      revision: current.revision,
    });
    await button("Korunan sürümleri yönet");
    await $('.ts-dialog input[type="checkbox"]').waitForDisplayed();
    await $('.ts-dialog input[type="checkbox"]').click();
    await button("Kaldırmayı önizle");
    await $("button=Korumaları kaldır").waitForDisplayed();
    await browser.saveScreenshot("artifacts/team-v2/windows-pins.png");
    await button("Korumaları kaldır");
    await $('.ts-dialog [role="status"]').waitForDisplayed();
    assert(
      (await $('.ts-dialog [role="status"]').getText()).includes(
        "koruma kaldırıldı",
      ),
    );
    await button("Kapat");
    await $(".ts-dialog").waitForExist({ reverse: true });
  });
  it("recovers private credentials after reset and requires the recipient desktop to verify", async () => {
    const members = await action({ kind: "members" });
    const member = members.find((m) => m.username === "native.user");
    const reset = await action({
      kind: "resetMember",
      accountId: member.accountId,
    });
    await button("Team oturumunu kapat");
    await button("Team hesabı");
    fixture.username = "native.user";
    await loginFields(reset.temporaryPassword);
    const fields = await $$(".ts-login input");
    await fields[0].setValue(password);
    await fields[1].setValue(password);
    await button("Parolayı belirle");
    await loginFields(password);
    assert((await action({ kind: "credentials" })).some((c) => c.unavailable));
    await button("Team oturumunu kapat");
    await button("Team hesabı");
    fixture.username = "leader";
    await loginFields(password);
    await assert.rejects(
      action({
        kind: "recoveryBegin",
        accountId: member.accountId,
        path: recoveryFile,
        password: "Wrong-Recovery-Password",
        reauthentication: password,
        reason: "Disposable native recovery",
      }),
    );
    const preview = await action({
      kind: "recoveryBegin",
      accountId: member.accountId,
      path: recoveryFile,
      password: "Recovery-Native-Lab-2026!",
      reauthentication: password,
      reason: "Disposable native reset recovery",
    });
    assert(preview.total > 0);
    for (;;) {
      if ((await action({ kind: "recoveryStep" })).completed) break;
    }
    await button("Team oturumunu kapat");
    await button("Team hesabı");
    fixture.username = "native.user";
    await loginFields(password);
    await button("Kimliklerim");
    await button("Kurtarmayı doğrula");
    await button("Bu kurtarmayı doğrula");
    await $("button=Açılabildiğini doğrula").waitForDisplayed();
    await browser.saveScreenshot("artifacts/team-v2/windows-recovery.png");
    await button("Açılabildiğini doğrula");
    await $("button=Kapat").waitForEnabled();
    await browser.waitUntil(async () =>
      (await $('.ts-dialog [role="status"]').getText()).includes(
        "doğrulandı ve tamamlandı",
      ),
    );
    await button("Kapat");
    await $(".ts-dialog").waitForExist({ reverse: true });
    assert(
      (await action({ kind: "credentials" })).every((c) => !c.unavailable),
    );
    assert.equal((await action({ kind: "recoveryPending" })).length, 0);
  });
  it("journals every native save and rejects stale local edits without bypassing CAS",async()=>{
    await button("Team oturumunu kapat");await button("Team hesabı");fixture.username="leader";await loginFields(password);
    const id=crypto.randomUUID();
    const content={kind:"host",parentId:folder,name:"Günlük kabul testi",address:"127.0.0.1",protocol:"ssh",port:22,usernamePolicy:"personal",username:null,deleted:false};
    const created=await action({kind:"saveResource",vaultId:vault,id,expectedRevision:0,clientAt:new Date().toISOString(),content});
    assert.equal(created.status,"applied");assert.equal(created.revision,1);
    const cached=await action({kind:"resource",id});
    assert.equal(typeof cached.localGeneration,"number");
    const saved=await action({kind:"saveResource",vaultId:vault,id,expectedRevision:1,clientAt:new Date().toISOString(),content:{...content,name:"Sunucudaki yeni sürüm"}});
    assert.equal(saved.revision,2);
    await assert.rejects(action({kind:"saveResource",vaultId:vault,id,expectedRevision:1,localGeneration:cached.localGeneration,clientAt:new Date().toISOString(),content:{...content,name:"Eski yerel düzenleme"}}));
    await button("Team oturumunu kapat");await button("Team hesabı");await loginFields(password);
    assert.equal((await action({kind:"journalStatus"})).pending,0);
    await browser.saveScreenshot("artifacts/team-v2/windows-journal.png");
    const current=await action({kind:"resource",id});
    assert.equal(current.content.name,"Sunucudaki yeni sürüm");
    assert.equal(current.revision,2);
  });

  it("rotates the company key through the native file and progress UI",async()=>{
    const newFile=path.resolve('.lab/team-v2-http/native-rotation-'+crypto.randomUUID()+'.ttrecovery');
    await $('.main-nav').$('button=Team').click();await button('Kurtarma anahtarını değiştir');
    const d=await $('.ts-dialog');
    await d.$('label*=Eski kurtarma dosyası').$('input').setValue(recoveryFile);
    await d.$('label*=Yeni kurtarma dosyası').$('input').setValue(newFile);
    await d.$('label*=Eski dosyanın parolası').$('input').setValue('Recovery-Native-Lab-2026!');
    await d.$('label*=Yeni dosyanın parolası').$('input').setValue('Recovery-Native-Lab-2026!');
    await d.$('label*=Team hesap parolan').$('input').setValue(password);
    await d.$('textarea').setValue('Windows native recovery key rotation');
    await button('Dosyaları doğrula ve aç');await $('.ts-dialog [role="status"]').waitForExist({timeout:20000});
    const finish=await $('button=Eski anahtarı emekliye ayır ve bitir');
    for(let i=0;i<20&&!(await finish.isEnabled());i++){
      await button('Sonraki parçayı doğrula');await $('button=Sonraki parçayı doğrula').waitForEnabled({timeout:20000});
    }
    assert(await finish.isEnabled());
    await browser.saveScreenshot('artifacts/team-v2/windows-rotation-preview.png');
    await finish.click();await browser.waitUntil(async()=> (await $('.ts-dialog [role="status"]').getText()).includes('etkinleştirildi'));
    assert((await fs.stat(newFile)).size>100);recoveryFile=newFile;
    await button('Kapat');
  });
  it("recovers a shared credential only after the native recipient preview",async()=>{
    const source=crypto.randomUUID(),label='Native shared recovery';
    await action({kind:'saveCredential',id:source,expectedRevision:0,credentialKind:'password',label,secret:'Disposable-Shared-Recovery-123!',passphrase:null});
    const p=await action({kind:'prepareShare',sourceId:source,scopeId:host,recipients:[]});
    const created=await action({kind:'finishShare',planId:p.id});
    await $('.main-nav').$('button=Team').click();
    await button('Ortak kimliği kurtar');
    await $('.ts-dialog[aria-label="Kurtarılacak ortak kimlik"]').$(`button=${label}`).click();
    const d=await $('.ts-dialog');
    await d.$('label*=Şirket kurtarma dosyası').$('input').setValue(recoveryFile);
    await d.$('label*=Dosya parolası').$('input').setValue('Recovery-Native-Lab-2026!');
    await d.$('label*=Team hesap parolan').$('input').setValue(password);
    await d.$('textarea').setValue('Windows native shared identity recovery');
    await button('Dosyayı aç ve alıcıları önizle');await $('.ts-dialog table').waitForExist({timeout:20000});
    assert.equal((await action({kind:'credentials'})).find(x=>x.id===created.credentialId).revision,1);
    await browser.saveScreenshot('artifacts/team-v2/windows-shared-recovery.png');
    await button('Yeni revizyonla kurtarmayı uygula');await $('.ts-dialog').waitForExist({reverse:true,timeout:20000});
    const recovered=(await action({kind:'credentials'})).find(x=>x.id===created.credentialId);assert.equal(recovered.revision,2);
    const secret=await action({kind:'revealShared',id:created.credentialId});assert.equal(secret.secret,'Disposable-Shared-Recovery-123!');
  });

  it("moves a host through the native access preview and reloads the folder view",async()=>{
    const a=crypto.randomUUID(),b=crypto.randomUUID(),h=crypto.randomUUID();
    for(const [id,kind,name,parentId] of [[a,'folder','Taşıma kaynağı',null],[b,'folder','Taşıma hedefi',null],[h,'host','Taşınacak makine',a]]){
      await action({kind:'saveResource',vaultId:vault,id,expectedRevision:0,clientAt:new Date().toISOString(),content:{kind,name,parentId,address:kind==='host'?'127.0.0.1':null,port:kind==='host'?22:null,protocol:kind==='host'?'ssh':null,usernamePolicy:kind==='host'?'personal':null,username:null,deleted:false}});
    }
    await $('.main-nav').$('button*=Hosts').click();await $('button[aria-label="Yenile"]').click();
    await browser.waitUntil(async()=>{for(const g of await $$('.group-main'))if((await g.getText()).includes('Taşıma kaynağı'))return true;return false;});
    const groups=await $$('.group-main');for(const g of groups)if((await g.getText()).includes('Taşıma kaynağı')){await g.waitForEnabled();await g.click();break;}
    const card=await $('.record-card');await card.waitForDisplayed();
    // Embedded WDIO does not emit contextmenu; trusted mouse dispatch is covered by Playwright.
    await browser.execute(el=>el.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:500,clientY:220})),card);
    await button('Taşı');
    await selectValue(await $('select[aria-label="Hedef klasör"]'),b);
    await button('Taşıma ve erişim önizlemesi');await $('.ts-dialog table').waitForExist();
    assert.equal((await action({kind:'resource',id:h})).content.parentId,a);
    const apply=await $('button=Onayla ve taşı');
    for(let i=0;i<20;i++) {
      await browser.waitUntil(async()=>{
        const alert=await $('.ts-dialog [role="alert"]');
        if(await alert.isExisting())throw new Error('Move preview failed: '+await alert.getText());
        const more=await $('button=Sonraki erişimler');
        if(await more.isExisting() && await more.isEnabled()){await more.click();return false;}
        const stage=await $('button=Sonraki anahtarları hazırla');
        return await apply.isEnabled() || (await stage.isExisting() && await stage.isEnabled());
      });
      if(await apply.isEnabled())break;
      await button('Sonraki anahtarları hazırla');
    }
    await browser.saveScreenshot('artifacts/team-v2/windows-move-preview.png');
    await apply.click();await $('.ts-dialog').waitForExist({reverse:true});
    assert.equal((await action({kind:'resource',id:h})).content.parentId,b);
    await browser.waitUntil(async()=>!(await $('.record-card').isExisting()));
  });

  it("opens typed Team workspace in real SSH panels and sends startup only after authentication",async()=>{
    const ssh=JSON.parse(await fs.readFile('.lab/team-v2-http/ssh-native.json','utf8'));
    if(!recoveryFile)await action({kind:'createRecovery',path:path.resolve('.lab/team-v2-http/docs-'+crypto.randomUUID()+'.ttrecovery'),password:'Recovery-Docs-Lab-2026!',reauthentication:password});
    const credential=crypto.randomUUID();await action({kind:'saveCredential',id:credential,expectedRevision:0,credentialKind:'ssh_key',label:'Public disposable document key',secret:await fs.readFile('tests/ssh-keys/rsa4096.pkcs1','utf8'),certificate:null,passphrase:null});
    const currentDefault=(await action({kind:'profiles'})).default;
    await action({kind:'saveProfile',selection:{hostId:null,expectedRevision:currentDefault?.revision??0,method:'credential',credentialId:credential,username:null}});
    const targets=[crypto.randomUUID(),crypto.randomUUID()];
    for(const [index,id] of targets.entries())await action({kind:'saveResource',vaultId:vault,id,expectedRevision:0,clientAt:new Date().toISOString(),content:{kind:'host',parentId:null,name:'Workspace SSH '+(index+1),address:ssh.host,port:ssh.port,protocol:'ssh',usernamePolicy:'shared_required',username:ssh.username,deleted:false}});
    const id=crypto.randomUUID();const content={kind:'workspace',parentId:null,name:'Gerçek izleme alanı',address:null,port:null,protocol:null,usernamePolicy:null,username:null,deleted:false,document:{type:'workspace',layout:'grid',entries:targets.map(hostId=>({hostId,startupCommand:"printf '\\124\\124\\137\\104\\117\\103\\137\\117\\113\\n'",statsEnabled:false}))}};
    await action({kind:'saveResource',vaultId:vault,id,expectedRevision:0,clientAt:new Date().toISOString(),content});
    const row=await action({kind:'resource',id});
    await assert.rejects(invoke('team_v2_document_start',{documentId:id,targetId:targets[0],expectedRevision:row.revision,localGeneration:row.localGeneration}));
    await assert.rejects(invoke('team_v2_document_start',{documentId:id,targetId:null,expectedRevision:0,localGeneration:row.localGeneration}));
    await browser.executeAsync(done=>{window.__teamDocEvents=[];window.__TAURI__.event.listen('session-event',e=>window.__teamDocEvents.push(e.payload)).then(()=>done());});
    await button('Ortak kasalar'); // Reset folder context left by the move scenario.
    await button('Komutlar / Workspaces');await $('button[aria-label="Yenile"]').click();
    await $('.ts-workspace main[aria-busy="true"]').waitForExist({reverse:true});
    await $('.ts-document-list').$('button*=Gerçek izleme alanı').click();
    await button('Önizlenen bağlantıları aç');
    await browser.waitUntil(async()=>{
      if(await $('button*=Parmak izine güven').isExisting()){await button('Parmak izine güven');return false;}
      return await browser.execute(()=>window.__teamDocEvents.filter(e=>e.kind==='connected').length===2);
    },{timeout:30000});
    await browser.waitUntil(async()=>{
      const events=await browser.execute(()=>window.__teamDocEvents);
      const ids=events.filter(e=>e.kind==='connected').map(e=>e.id);
      return ids.length===2 && ids.every(id=>events.filter(e=>e.id===id&&e.kind==='data').map(e=>Buffer.from(e.detail,'base64').toString()).join('').includes('TT_DOC_OK'));
    },{timeout:15000});
    assert.equal((await $$('.ts-terminal-grid .ts-terminal-panel')).length,2);
    await browser.saveScreenshot('artifacts/team-v2/windows-documents.png');
    const events=await browser.execute(()=>window.__teamDocEvents);for(const e of events.filter(e=>e.kind==='connected'))await invoke('session_input',{id:e.id,close:true});
  });

});
