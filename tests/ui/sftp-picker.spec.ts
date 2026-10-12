import { test, expect } from "@playwright/test";
test.beforeEach(async ({ page }) => {
  await page.goto("/tests/ui/sftp-picker-harness.html");
});
test("SFTP browses nested folders, searches all names and ranks successful recent hosts", async ({
  page,
}) => {
  const right = page.locator('[data-file-pane="1"]');
  await right
    .getByRole("button", { name: "Right connection", exact: true })
    .click();
  const d = page.getByRole("dialog", { name: "Choose SFTP host" });
  await expect(
    d.getByRole("button", { name: "Connect to İstanbul PostgreSQL" }),
  ).toHaveCount(0);
  await d.getByRole("button", { name: "Üretim", exact: true }).click();
  await d.getByRole("button", { name: "Veritabanları", exact: true }).click();
  await d
    .getByRole("button", { name: "Connect to İstanbul PostgreSQL" })
    .click();
  await expect(
    right.getByRole("button", { name: "Right connection" }),
  ).toContainText("İstanbul PostgreSQL");
  await expect(right.locator(".path-bar input")).toHaveValue("/home/test");
  await right.getByRole("button", { name: "Right connection" }).click();
  await d.getByRole("textbox", { name: "Search host names" }).fill("ANKARA");
  await d.getByRole("button", { name: "Connect to Ankara backup" }).click();
  await expect(right.locator(".path-bar input")).toHaveValue("/home/test");
  await right.getByRole("button", { name: "Right connection" }).click();
  await d.getByRole("button", { name: "Vault root", exact: true }).click();
  const recent = d.getByRole("region", { name: "Recent connections" });
  await expect(recent.getByRole("button").first()).toHaveAttribute(
    "aria-label",
    "Connect to Ankara backup",
  );
  await expect(recent.getByRole("button").nth(1)).toHaveAttribute(
    "aria-label",
    "Connect to İstanbul PostgreSQL",
  );
  await expect(d).not.toContainText("Telnet only");
  await page.screenshot({ path: "artifacts/team-v2/sftp-host-picker.png" });
  await d.getByRole("textbox", { name: "Search host names" }).fill("istanbul");
  await expect(
    d.getByRole("button", { name: "Connect to İstanbul PostgreSQL" }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(d).toHaveCount(0);
  await expect(
    right.getByRole("button", { name: "Right connection" }),
  ).toBeFocused();
});
test("failed or superseded connections never retain old server files or become recent", async ({
  page,
}) => {
  const right = page.locator('[data-file-pane="1"]');
  async function choose(name: string) {
    await right.getByRole("button", { name: "Right connection" }).click();
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Connect to " + name, exact: true })
      .click();
  }
  await choose("Kök makine");
  await expect(right.locator(".path-bar input")).toHaveValue("/home/test");
  await choose("Başarısız makine");
  await expect(right).toContainText("Connection refused");
  await expect(right.locator(".path-bar input")).toHaveValue("");
  await expect(right.locator(".file-row")).toHaveCount(0);
  await expect(
    right.getByRole("button", { name: "Refresh directory" }),
  ).toBeDisabled();
  await choose("Yavaş makine");
  await right.getByRole("button", { name: "Right connection" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Local computer", exact: true })
    .click();
  await page.evaluate(() => {
    (window as any).releaseConnection();
  });
  await expect(right.locator(".path-bar input")).toHaveValue("C:/test");
  await expect
    .poll(() =>
      page.evaluate(() => {
        const w = window as any;
        const slow = w.calls.find(
          (c: any) => c.command === "sftp_connect" && c.hostId === "slow",
        );
        return w.calls.some(
          (c: any) => c.command === "sftp_disconnect" && c.id === slow?.id,
        );
      }),
    )
    .toBe(true);
  await right.getByRole("button", { name: "Right connection" }).click();
  const recent = page.getByRole("region", { name: "Recent connections" });
  await expect(recent.getByRole("button")).toHaveCount(1);
  await expect(recent).toContainText("Kök makine");
});

test("pending or failed directories cannot receive drag/drop transfers or retain old entries", async ({
  page,
}) => {
  const right = page.locator('[data-file-pane="1"]');
  await right.getByRole("button", { name: "Right connection" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Connect to Kök makine" })
    .click();
  await expect(right.locator(".file-row")).toHaveCount(1);
  const path = right.getByRole("textbox", { name: "Right path" });
  async function drop() {
    await right.evaluate((el) => {
      const dataTransfer = new DataTransfer();
      dataTransfer.setData(
        "application/termterm",
        JSON.stringify({ pane: 0, path: "C:/test/file.txt" }),
      );
      el.dispatchEvent(new DragEvent("drop", { bubbles: true, dataTransfer }));
    });
  }
  await path.fill("/pending");
  await path.press("Enter");
  await expect(
    right.getByRole("button", { name: "New folder", exact: true }),
  ).toBeDisabled();
  await drop();
  await path.fill("/denied");
  await path.press("Enter");
  await expect(right).toContainText("Permission denied");
  await expect(right.locator(".file-actions")).toContainText("0 items");
  await drop();
  await page.evaluate(() => (window as any).releaseDirectory());
  await expect(right).toContainText("Permission denied");
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.command === "file_transfer")
          .length,
    ),
  ).toBe(0);
  await path.fill("/home/test");
  await path.press("Enter");
  await expect(right.locator(".file-row")).toHaveCount(1);
  await expect(
    right.getByRole("button", { name: "New folder", exact: true }),
  ).toBeEnabled();
});


test('revoked SFTP clears visible files and ignores a late directory response',async({page})=>{
 const right=page.locator('[data-file-pane="1"]');
 await right.getByRole('button',{name:'Right connection'}).click();
 await page.getByRole('region',{name:'Hosts in this folder'}).getByRole('button',{name:'Connect to Kök makine',exact:true}).click();
 await expect(right.locator('.file-row')).toHaveCount(1);
 const id=await page.evaluate(()=>(window as any).calls.find((x:any)=>x.command==='sftp_connect').id);
 await right.getByRole('textbox',{name:'Right path'}).fill('/pending');
 await right.getByRole('textbox',{name:'Right path'}).press('Enter');
 await expect.poll(()=>page.evaluate(()=>typeof (window as any).releaseDirectory)).toBe('function');
 await page.evaluate(id=>(window as any).emitEvent('sftp-closed',{id}),id);
 await page.evaluate(()=>(window as any).releaseDirectory());
 await expect(right).toContainText('SFTP connection closed');
 await expect(right.locator('.file-row')).toHaveCount(0);
 await right.getByRole('button',{name:'Right connection'}).click();
 await page.getByRole('region',{name:'Hosts in this folder'}).getByRole('button',{name:'Connect to Kök makine',exact:true}).click();
 await expect(right.locator('.file-row')).toHaveCount(1);
 await page.evaluate(id=>(window as any).emitEvent('sftp-closed',{id}),id);
 await expect(right.locator('.file-row')).toHaveCount(1);
});
