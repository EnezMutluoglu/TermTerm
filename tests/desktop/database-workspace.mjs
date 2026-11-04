import { browser, $ } from "@wdio/globals";
import fs from "node:fs/promises";
import path from "node:path";
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
const root = process.cwd(),
  work = path.join(root, ".lab", "database", "native-" + Date.now());
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
const db = (request) => invoke("database_request", { request });
describe("Database workspace native", () => {
  let profiles, vaultPath;
  before(async () => {
    await browser.setTimeout({ script: 120000 });
    assert.ok(
      (await invoke("app_info")).defaultVaultPath.includes("databasedev.e2e"),
    );
    await fs.mkdir(work, { recursive: true });
    await fs.mkdir(path.join(root, "artifacts/database-workspace"), {
      recursive: true,
    });
    profiles = JSON.parse(
      await fs.readFile(".lab/database/profiles.json", "utf8"),
    )
      .map((p) => ({ ...p, id: crypto.randomUUID() }))
      .filter(
        (p) =>
          !process.env.DB_ENGINES ||
          process.env.DB_ENGINES.split(",").includes(p.engine),
      );
    await invoke("vault_lock");
    vaultPath = path.join(work, "database.ttvault");
    await invoke("vault_create", {
      path: vaultPath,
      name: "Database Workspace · Laboratuvar",
      password: "Disposable-database-ui-only-2026",
    });
    await invoke("records_save", {
      records: profiles.map((p) => ({
        id: p.id,
        kind: "databaseConnection",
        data: {
          ...p,
          environment: "test",
          groupId: "",
          sshHostId: "",
          caPath: p.caPath ?? "",
        },
        updatedAt: Date.now(),
      })),
    });
    await browser.refresh();
    await $("button=Veritabanları").waitForDisplayed();
    await $("button=Veritabanları").click();
  });
  it("opens real engine tabs, renders results and preserves tabs across navigation", async () => {
    for (const p of profiles) {
      const button = await $(
        `//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[@title='Bağlan / yeni sorgu']`,
      );
      await button.click();
      await browser.waitUntil(
        async () =>
          await $(".db-query-tabs")
            .getText()
            .then((t) => t.includes(p.label)),
        { timeout: 20000 },
      );
      await $(".db-toolbar button.primary").waitForEnabled();
      await browser.execute((sql) => {
        const editor = document.querySelector(".cm-content");
        editor.focus();
        editor.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "a",
            code: "KeyA",
            ctrlKey: true,
            bubbles: true,
            cancelable: true,
          }),
        );
        const clipboardData = new DataTransfer();
        clipboardData.setData("text/plain", sql);
        editor.dispatchEvent(
          new ClipboardEvent("paste", {
            clipboardData,
            bubbles: true,
            cancelable: true,
          }),
        );
      }, "SELECT order_id, customer_name, region, total_amount, status FROM tt_demo_orders ORDER BY order_id;");
      await browser.waitUntil(async () =>
        (await $(".cm-content").getText()).includes("tt_demo_orders"),
      );
      await $(".db-toolbar button.primary").click();
      await $(".db-results tbody tr").waitForDisplayed({ timeout: 20000 });
      assert.ok((await $(".db-results tbody").getText()).length > 0);
      await browser.saveScreenshot(
        path.join(root, `artifacts/database-workspace/${p.engine}-editor.png`),
      );
    }
    const before = await $(".db-query-tabs").getText();
    await $("button*=Hosts").click();
    await $("button=Veritabanları").click();
    assert.equal(await $(".db-query-tabs").getText(), before);
    await $("button*=Kontroller ve yönetim").click();
    await $(".db-action").waitForDisplayed();
    await browser.saveScreenshot(
      path.join(root, "artifacts/database-workspace/catalog.png"),
    );
    await $(".db-action").$("button=Çalıştır").click();
    await $(".db-check-result table").waitForDisplayed({ timeout: 20000 });
    await browser.saveScreenshot(
      path.join(root, "artifacts/database-workspace/health-check.png"),
    );
    await $('.modal button[aria-label="Close dialog"]').click();
  });
  it("uses a verified two-hop SSH chain and keeps monitoring outside the editor transaction", async () => {
    const ssh = JSON.parse(await fs.readFile(".lab/ssh.json", "utf8"));
    const rec = (kind, data) => ({
      id: crypto.randomUUID(),
      kind,
      data,
      updatedAt: Date.now(),
    });
    // Public host identity comes from the local lab provisioner, never from an unverified network scan.
    const known = rec("knownHost", {
      address: `[${ssh.address}]:${ssh.port}`,
      publicKey: ssh.publicKey,
      label: "Local lab host identity",
    });
    const jump = rec("host", {
      label: "SQL test jump",
      protocol: "ssh",
      address: ssh.address,
      port: ssh.port,
      username: ssh.username,
      password: ssh.password,
    });
    const target = rec("host", {
      ...jump.data,
      label: "SQL second hop",
      chain: [jump.id],
    });
    const pg = profiles.find((p) => p.engine === "postgresql");
    const tunneled = rec("databaseConnection", {
      ...pg,
      label: "PostgreSQL SSH chain test",
      host: "localhost",
      port: 55432,
      sshHostId: target.id,
    });
    await invoke("records_save", { records: [known, jump, target, tunneled] });
    const sftpId=crypto.randomUUID();const home=await invoke('sftp_connect',{id:sftpId,hostId:target.id});
    const source=path.join(work,'db-regression-İstanbul.txt'),download=path.join(work,'sftp-roundtrip.txt'),remote=home+'/tt-db-regression-'+crypto.randomUUID()+'.txt';
    const payload=Buffer.from('TermTerm SQL regression — İstanbul\n'.repeat(500));await fs.writeFile(source,payload);
    try {await invoke('file_transfer',{id:crypto.randomUUID(),source:{connection:'local',path:source},dest:{connection:sftpId,path:remote},overwrite:false});await invoke('file_transfer',{id:crypto.randomUUID(),source:{connection:sftpId,path:remote},dest:{connection:'local',path:download},overwrite:false});assert.deepEqual(await fs.readFile(download),payload);}
    finally {await invoke('file_action',{endpoint:{connection:sftpId,path:remote},action:'remove',permissions:null,target:null}).catch(()=>{});await invoke('sftp_disconnect',{id:sftpId});}
    const opened = await db({ op: "open", recordId: tunneled.id }),
      sessionId = opened.sessionId;
    try {
      const value = await db({
        op: "query",
        sessionId,
        sql: "SELECT current_database()",
      });
      assert.equal(value.results[0].rows[0][0], "termterm_sql_lab");
      await db({
        op: "transaction",
        sessionId,
        action: "autocommit",
        enabled: false,
      });
      await db({ op: "query", sessionId, sql: "SELECT 1" });
      assert.equal((await db({ op: "status", sessionId })).transaction, true);
      await db({ op: "check", recordId: tunneled.id, actionId: "PG-MON-001" });
      assert.equal((await db({ op: "status", sessionId })).transaction, true);
      await db({ op: "transaction", sessionId, action: "rollback" });
      assert.equal((await db({ op: "status", sessionId })).transaction, false);
      await assert.rejects(() =>
        db({ op: "check", recordId: tunneled.id, actionId: "MS-MON-001" }),
      );
    } finally {
      await db({ op: "close", sessionId });
    }
  });
  it("shows dev2 monitoring history and preserves editor undo across query tabs", async () => {
    await $('button*=SQL çalışma alanı').click();
    const tabs=await $$('.db-query-tabs > div > button:first-child');
    await tabs[0].click();
    const before=await $('.cm-content').getText();
    await browser.execute(()=>{const editor=document.querySelector('.cm-content');editor.focus();editor.dispatchEvent(new KeyboardEvent('keydown',{key:'End',ctrlKey:true,bubbles:true,cancelable:true}));const clipboardData=new DataTransfer();clipboardData.setData('text/plain','\n-- undo-survives-tab-switch');editor.dispatchEvent(new ClipboardEvent('paste',{clipboardData,bubbles:true,cancelable:true}));});
    await browser.waitUntil(async()=>(await $('.cm-content').getText()).includes('undo-survives'));
    await tabs[1].click();await tabs[0].click();
    await browser.execute(()=>{const editor=document.querySelector('.cm-content');editor.focus();editor.dispatchEvent(new KeyboardEvent('keydown',{key:'z',code:'KeyZ',ctrlKey:true,bubbles:true,cancelable:true}));});
    await browser.waitUntil(async()=>(await $('.cm-content').getText())===before);
    for(const p of profiles){
      const actionId=p.engine==='postgresql'?'PG-MON-013':p.engine==='mssql'?'MS-MON-009':'OR-MON-016';
      await db({op:'check',recordId:p.id,actionId});
      const history=await db({op:'history',recordId:p.id});assert.ok(history.samples.some(s=>s.actionId===actionId));assert.ok(!JSON.stringify(history).includes(p.password));assert.ok(!JSON.stringify(history).includes('query_text'));
      await $(`//div[contains(@class,'db-connection')][.//strong[text()='${p.label}']]//button[1]`).click();
      await $('button=Sağlık ve geçmiş').click();await $('button=Alan ne hızla doluyor?').click();
      await browser.waitUntil(async()=>await $(`.db-diagnostics select option[value="${actionId}"]`).isExisting());
      await browser.execute(id=>{const select=document.querySelector('.db-diagnostics select');select.value=id;select.dispatchEvent(new Event('change',{bubbles:true}));},actionId);
      await browser.waitUntil(async()=>await $('.db-diagnostics select').getValue()===actionId);
      await $('.db-diagnostics button.primary').click();await browser.waitUntil(async()=>await $('.db-diagnostics button.primary').isEnabled(),{timeout:20000});
      await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${p.engine}-dev2-monitoring.png`));
      await $('button*=SQL çalışma alanı').click();
      const schema=p.engine==='postgresql'?'public':p.engine==='mssql'?'dbo':'SYSTEM';
      await $(`//button[contains(@class,"db-tree-row")][normalize-space(.)="${schema}"]`).waitForDisplayed();await $(`//button[contains(@class,"db-tree-row")][normalize-space(.)="${schema}"]`).click();
      const tableName=p.engine==='oracle'?'TT_DEMO_ORDERS':'tt_demo_orders';
      await $(`//button[contains(@class,"db-table")][normalize-space(.)="${tableName}"]`).waitForDisplayed();await $(`//button[contains(@class,"db-table")][normalize-space(.)="${tableName}"]`).click();
      await $('.db-browser tbody tr').waitForDisplayed();
      await $('button=Sütunlar').click();await $('.db-browser tbody').waitForDisplayed();
      await browser.saveScreenshot(path.join(root,`artifacts/database-workspace/${p.engine}-dev2-metadata.png`));
      await $('.modal button[aria-label="Close dialog"]').click();
    }
    await $('button*=SQL çalışma alanı').click();
  });
  it("recovers a stopped private helper and closes an unreachable query tab", async () => {
    await $('button*=SQL çalışma alanı').click();
    const before=await $$('.db-query-tabs > div');assert.ok(before.length);
    const running=await db({op:'open',recordId:profiles[0].id});
    const applicationPath=path.resolve(process.env.TERMTERM_BINARY??'src-tauri/target/debug/termterm.exe').replaceAll("'","''");
    const helperIds=execFileSync('powershell.exe',['-NoProfile','-Command',`$apps=Get-CimInstance Win32_Process -Filter \"Name='termterm.exe'\" | Where-Object {$_.ExecutablePath -eq '${applicationPath}'}; Get-CimInstance Win32_Process -Filter \"Name='java.exe'\" | Where-Object {$apps.ProcessId -contains $_.ParentProcessId -and $_.CommandLine -match ' Main$'} | ForEach-Object {$_.ProcessId}`],{encoding:'utf8',windowsHide:true}).trim().split(/\s+/).filter(Boolean).map(Number);
    assert.equal(helperIds.length,1,'Only this native test application may own the stopped helper');process.kill(helperIds[0]);
    await new Promise(r=>setTimeout(r,150));
    await assert.rejects(()=>db({op:'query',sessionId:running.sessionId,sql:'SELECT 1'}));
    const reopened=await db({op:'open',recordId:profiles[0].id});assert.ok(reopened.sessionId);await db({op:'close',sessionId:reopened.sessionId});
    await $('.db-query-tabs > div.active button[aria-label]').click();
    await $('button=Bağlantıyı kapat').waitForDisplayed();await $('button=Bağlantıyı kapat').click();
    await browser.waitUntil(async()=>(await $$('.db-query-tabs > div')).length===before.length-1);
  });
  it("keeps DB secrets encrypted, rejects unknown operations and closes SQL on vault lock", async () => {
    const file = await fs.readFile(vaultPath);
    for (const p of profiles)
      assert.ok(!file.includes(Buffer.from(p.password)));
    const opened = await db({ op: "open", recordId: profiles[0].id });
    await assert.rejects(() =>
      db({ op: "open", recordId: "absent", profile: profiles[0] }),
    );
    await assert.rejects(() =>
      db({ op: "exec_os_command", sessionId: opened.sessionId }),
    );
    await invoke("vault_lock");
    await assert.rejects(() =>
      db({ op: "query", sessionId: opened.sessionId, sql: "SELECT 1" }),
    );
    await invoke("vault_open", {
      path: vaultPath,
      password: "Disposable-database-ui-only-2026",
    });
    await assert.rejects(() =>
      db({ op: "query", sessionId: opened.sessionId, sql: "SELECT 1" }),
    );
    await browser.refresh();
    await $("button=Veritabanları").waitForDisplayed();
    await $("button=Veritabanları").click();
  });
});
