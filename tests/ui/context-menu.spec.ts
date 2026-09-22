import { test, expect, type Page } from "@playwright/test";
const r = (id: string, kind: string, data: any) => ({
  id,
  kind,
  data,
  updatedAt: 1,
});
const fixture = [
  r("g", "group", { label: "Production", username: "deploy" }),
  r("dest", "group", { label: "Destination" }),
  r("child", "group", { label: "Nested", groupId: "g" }),
  r("a", "host", {
    label: "API",
    address: "api.example.test",
    groupId: "g",
    credentialId: "key",
    chain: ["jump"],
  }),
  r("b", "host", { label: "DB", address: "db.example.test", groupId: "g" }),
  r("c", "host", {
    label: "Nested host",
    address: "nested.example.test",
    groupId: "child",
  }),
  r("jump", "host", { label: "Gateway", address: "jump.example.test" }),
  r("key", "credential", {
    label: "Deploy key",
    publicKey: "ssh-ed25519 fixture",
    privateKey: "PRIVATE_FIXTURE_HIDDEN",
  }),
  r("snippet", "snippet", { label: "Health check", command: "uptime" }),
  r("tunnel", "tunnel", {
    label: "Database forward",
    hostId: "a",
    mode: "local",
    bindAddress: "127.0.0.1",
    bindPort: 15432,
    targetAddress: "127.0.0.1",
    targetPort: 5432,
  }),
  r("workspace", "workspace", {
    label: "Operations",
    hostIds: ["a", "b"],
    layout: "split",
  }),
  r("known", "knownHost", {
    label: "Trusted gateway",
    address: "jump.example.test",
    fingerprint: "SHA256:fixture",
  }),
  r("log", "log", { label: "Previous session", content: "fixture log output" }),
];
async function start(page: Page) {
  await page.addInitScript((records) => {
    (window as any).recordsFixture = records;
  }, fixture);
  await page.goto("/tests/ui/terminal-harness.html");
  await expect(
    page.getByRole("heading", { name: "Hosts", exact: true }),
  ).toBeVisible();
}
const card = (page: Page, name: string) =>
  page
    .locator(".record-card")
    .filter({ has: page.locator("h3", { hasText: new RegExp(`^${name}$`) }) });
async function menu(page: Page, name: string, action?: string) {
  await card(page, name).click({ button: "right" });
  if (action)
    await page.getByRole("menuitem", { name: action, exact: true }).click();
}
async function folder(page: Page, name: string) {
  await page.locator(".group-main").filter({ hasText: name }).dblclick();
}
const saved = (page: Page) =>
  page.evaluate(() => (window as any).testVault.records);

test("single click is read-only, actions live in the context menu and editing really saves", async ({
  page,
}) => {
  await start(page);
  await folder(page, "Production");
  await card(page, "API").click();
  await expect(
    page.getByRole("complementary", { name: "Record details" }),
  ).toContainText("deploy");
  await expect(page.locator(".record-details input")).toHaveCount(0);
  await expect(
    page.locator(
      '.record-card input[type="checkbox"], .selection-bar, .card-menu, .card-connect',
    ),
  ).toHaveCount(0);
  await expect(page.locator(".record-details")).not.toContainText(
    "PRIVATE_FIXTURE_HIDDEN",
  );
  await menu(page, "API");
  await page.screenshot({ path: "artifacts/context-review/host-menu.png" });
  await page.getByRole("menuitem", { name: "Edit", exact: true }).click();
  await page
    .getByRole("textbox", { name: "Label *", exact: true })
    .fill("API edited");
  await page
    .locator(".editor-footer")
    .getByRole("button", { name: "Save", exact: true })
    .click();
  await expect(card(page, "API edited")).toBeVisible();
  expect((await saved(page)).find((r: any) => r.id === "a").data.label).toBe(
    "API edited",
  );
  await menu(page, "API edited", "Copy address");
  expect(await page.evaluate(() => (window as any).clipboardText)).toBe(
    "api.example.test",
  );
});

test("multi-select moves exact hosts; a nested group duplicate retains relationships", async ({
  page,
}) => {
  await start(page);
  await folder(page, "Production");
  await card(page, "API").click();
  await card(page, "DB").click({ modifiers: [process.platform === 'darwin' ? 'Meta' : 'Control'] });
  await menu(page, "DB", "Move to group…");
  await page.getByLabel("Destination group").selectOption("dest");
  await page.getByRole("button", { name: "Move records", exact: true }).click();
  await expect(card(page, "API")).toHaveCount(0);
  const moved = await saved(page);
  expect(
    moved
      .filter((r: any) => ["a", "b"].includes(r.id))
      .every((r: any) => r.data.groupId === "dest"),
  ).toBe(true);
  expect(moved.find((r: any) => r.id === "c").data.groupId).toBe("child");
  await page
    .locator(".breadcrumb")
    .getByRole("button", { name: "Hosts", exact: true })
    .click();
  await page
    .locator(".group-main")
    .filter({ hasText: "Production" })
    .click({ button: "right" });
  await page.getByRole("menuitem", { name: "Duplicate", exact: true }).click();
  await expect(
    page.locator(".group-main").filter({ hasText: "Production copy" }),
  ).toBeVisible();
  const copied = await saved(page),
    group = copied.find((r: any) => r.data.label === "Production copy"),
    nested = copied.find((r: any) => r.data.groupId === group.id);
  expect(copied.find((r: any) => r.data.groupId === nested.id).data.label).toBe(
    "Nested host",
  );
});

test("copy/cut/paste works across folders and failures preserve the original record", async ({
  page,
}) => {
  await start(page);
  await folder(page, "Production");
  await menu(page, "API", "Cut");
  await page
    .locator(".sidebar-group")
    .filter({ hasText: "Destination" })
    .click({ button: "right" });
  await page.getByRole("menuitem", { name: "Paste here", exact: true }).click();
  await expect(card(page, "API")).toHaveCount(0);
  expect((await saved(page)).find((r: any) => r.id === "a").data.groupId).toBe(
    "dest",
  );
  await menu(page, "DB", "Copy to group…");
  await page.getByLabel("Destination group").selectOption("dest");
  await page.getByRole("button", { name: "Copy records", exact: true }).click();
  await expect
    .poll(
      async () =>
        (await saved(page)).filter((r: any) => r.data.label === "DB copy")
          .length,
    )
    .toBe(1);
  await page.evaluate(() => {
    (window as any).failNext = "records_delete";
  });
  await menu(page, "DB", "Delete…");
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(page.locator(".toast.error")).toContainText("write failed");
  expect((await saved(page)).some((r: any) => r.id === "b")).toBe(true);
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(card(page, "DB")).toHaveCount(0);
});

test("menu supports keyboard, screen edges, outside click, and rapid mutation guard", async ({
  page,
}) => {
  await start(page);
  await folder(page, "Production");
  await card(page, "API").focus();
  await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menu")).toBeVisible();
  await page.keyboard.press("End");
  await expect(
    page.getByRole("menuitem", { name: "Delete…", exact: true }),
  ).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(card(page, "API")).toBeFocused();
  await card(page, "API").dispatchEvent("contextmenu", {
    clientX: 1438,
    clientY: 898,
  });
  const box = await page.getByRole("menu").boundingBox();
  expect(box!.x + box!.width).toBeLessThanOrEqual(1440);
  expect(box!.y + box!.height).toBeLessThanOrEqual(900);
  await page.mouse.click(10, 10);
  await expect(page.getByRole("menu")).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).holdWrites = true;
  });
  await menu(page, "API", "Duplicate");
  await menu(page, "API");
  await expect(
    page.getByRole("menuitem", { name: "Duplicate", exact: true }),
  ).toBeDisabled();
  await page.keyboard.press("Escape");
  await page.evaluate(() => {
    (window as any).holdWrites = false;
    (window as any).releaseWrite();
  });
  await expect(card(page, "API copy")).toBeVisible();
  expect(
    (await saved(page)).filter((r: any) => r.data.label === "API copy"),
  ).toHaveLength(1);
});

test("every record kind has relevant actions, selected backup scope and group quick connect", async ({
  page,
}) => {
  await start(page);
  await folder(page, "Production");
  await menu(page, "API", "Export encrypted backup…");
  await page.getByLabel("Backup password").fill("fixture-password");
  await page.getByRole("button", { name: "Save file", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).calls.find((c: any) => c.command === "backup_bundle")
            ?.args.ids,
      ),
    )
    .toEqual(["a"]);
  await page.getByRole("button", { name: "Keychain", exact: true }).click();
  await menu(page, "Deploy key", "Copy public key");
  expect(await page.evaluate(() => (window as any).clipboardText)).toBe(
    "ssh-ed25519 fixture",
  );
  await page.getByRole("button", { name: "Known hosts", exact: true }).click();
  await menu(page, "Trusted gateway", "Copy fingerprint");
  expect(await page.evaluate(() => (window as any).clipboardText)).toBe(
    "SHA256:fixture",
  );
  await page.getByRole("button", { name: "Session logs", exact: true }).click();
  await menu(page, "Previous session", "Copy log");
  expect(await page.evaluate(() => (window as any).clipboardText)).toBe(
    "fixture log output",
  );
  await page
    .getByRole("button", { name: "Port forwarding", exact: true })
    .click();
  await menu(page, "Database forward", "Start forwarding");
  await menu(page, "Database forward", "Stop forwarding");
  await page
    .locator(".sidebar-group")
    .filter({ hasText: "Production" })
    .click({ button: "right" });
  await page
    .getByRole("menuitem", { name: "Quick connect hosts", exact: true })
    .click();
  await expect(page.locator(".top-session-tab")).toHaveCount(3);
  await page.getByRole("button", { name: "Snippets", exact: true }).click();
  await menu(page, "Health check", "Run in active terminal");
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).inputs.some((i: any) => i.data === "uptime\r"),
      ),
    )
    .toBe(true);
  await page.locator(".top-session-tab").first().click({ button: "right" });
  await page
    .getByRole("menuitem", { name: "Close other terminals", exact: true })
    .click();
  await expect(page.locator(".top-session-tab")).toHaveCount(1);
  await page.getByRole("button", { name: "Workspaces", exact: true }).click();
  await menu(page, "Operations", "Open workspace");
  await expect(page.locator(".top-session-tab")).toHaveCount(3);
  await page
    .locator(".sidebar-group")
    .filter({ hasText: "Production" })
    .click({ button: "right" });
  await page.getByRole("menuitem", { name: "Open group", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Production", exact: true }),
  ).toBeVisible();
  await expect(card(page, "API")).toBeVisible();
});

test("SFTP multi-delete reports partial failure and retries only the remaining file", async ({
  page,
}) => {
  await start(page);
  await page.evaluate(() => {
    const w = window as any,
      invoke = w.__TAURI_INTERNALS__.invoke;
    let names = ["First.txt", "Second.txt"],
      denied = true;
    w.deleteAttempts = [];
    w.__TAURI_INTERNALS__.invoke = async (command: string, args: any) => {
      if (command === "file_list")
        return names.map((name) => ({
          name,
          path: args.endpoint.path + "/" + name,
          directory: false,
          symlink: false,
          size: 128,
          modified: 0,
          permissions: 0o644,
        }));
      if (command === "file_action" && args.action === "remove") {
        const name = args.endpoint.path.split("/").pop();
        w.deleteAttempts.push(name);
        if (name === "Second.txt" && denied) {
          denied = false;
          throw Error("Access denied by fixture");
        }
        names = names.filter((n) => n !== name);
        return;
      }
      return invoke(command, args);
    };
  });
  await page
    .locator(".top-tabs")
    .getByRole("button", { name: "SFTP", exact: true })
    .click();
  const pane = page.locator(".file-pane").first(),
    first = pane.getByRole("button", { name: "First.txt", exact: true }),
    second = pane.getByRole("button", { name: "Second.txt", exact: true });
  await first.click();
  await second.click({ modifiers: [process.platform === 'darwin' ? 'Meta' : 'Control'] });
  await second.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Delete…", exact: true }).click();
  await expect(page.locator(".modal")).toContainText("2 selected items");
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(page.locator(".toast")).toContainText(
    "Second.txt: Access denied",
  );
  await expect(first).toHaveCount(0);
  await expect(second).toBeVisible();
  await expect(page.locator(".modal")).toContainText("Second.txt");
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(page.locator(".modal")).toHaveCount(0);
  await expect(second).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).deleteAttempts)).toEqual([
    "First.txt",
    "Second.txt",
    "Second.txt",
  ]);
});

test("host SFTP opens the target; file actions use context menus with exact paths", async ({
  page,
}) => {
  await start(page);
  await page.evaluate(() => {
    const w = window as any,
      invoke = w.__TAURI_INTERNALS__.invoke;
    w.__TAURI_INTERNALS__.invoke = async (command: string, args: any) =>
      command === "file_list"
        ? [
            {
              name: "Notes.txt",
              path: args.endpoint.path + "/Notes.txt",
              directory: false,
              symlink: false,
              size: 128,
              modified: 0,
              permissions: 0o640,
            },
          ]
        : invoke(command, args);
  });
  await folder(page, "Production");
  await menu(page, "API", "Open SFTP");
  await expect(page.getByLabel("Right connection")).toHaveValue("a");
  await expect(page.getByLabel("Right path")).toHaveValue("/srv/demo");
  const file = page
    .locator(".file-pane")
    .last()
    .getByRole("button", { name: "Notes.txt", exact: true });
  await file.click();
  await expect(
    page.locator(".file-pane").last().locator(".file-properties"),
  ).toContainText("640");
  await file.click({ button: "right" });
  await page.screenshot({ path: "artifacts/context-review/sftp-menu.png" });
  await page.getByRole("menuitem", { name: "Copy path", exact: true }).click();
  expect(await page.evaluate(() => (window as any).clipboardText)).toBe(
    "/srv/demo/Notes.txt",
  );
  await file.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Rename…", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Renamed.txt");
  await page.getByRole("button", { name: "Apply", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).calls.find((c: any) => c.command === "file_action")
            ?.args.target,
      ),
    )
    .toBe("/srv/demo/Renamed.txt");
  await file.click({ button: "right" });
  await page
    .getByRole("menuitem", { name: "Copy to target directory", exact: true })
    .click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).calls.find((c: any) => c.command === "file_transfer")
            ?.args.dest.path,
      ),
    )
    .toBe("/home/demo/Notes.txt");
});
