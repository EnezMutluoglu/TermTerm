import {test,expect} from "@playwright/test";
test("development builds cannot initiate an update",async({page})=>{
  await page.goto("/tests/ui/updates-harness.html?mode=development");
  await expect(page.getByRole("button",{name:"Check for updates"})).toBeDisabled();
  await expect(page.getByText("Development build: stable updates are disabled.")).toBeVisible();
});
test("update check errors leave the UI usable",async({page})=>{
  await page.goto("/tests/ui/updates-harness.html?mode=error");
  await page.getByRole("button",{name:"Check for updates"}).click();
  await expect(page.getByRole("alert")).toContainText("test network failure");
  await expect(page.getByRole("button",{name:"Check for updates"})).toBeEnabled();
});
test("install needs a user confirmation and reports failures",async({page})=>{
  await page.goto("/tests/ui/updates-harness.html");
  await page.getByRole("button",{name:"Check for updates"}).click();
  await expect(page.getByRole("button",{name:"Download and install"})).toBeVisible();
  await page.getByRole("button",{name:"Download and install"}).click();
  await page.getByRole("button",{name:"Cancel",exact:true}).click();
  expect(await page.evaluate(()=>(window as any).updateCalls.filter((c:string)=>c==="update_install").length)).toBe(0);
  await page.getByRole("button",{name:"Download and install"}).click();
  await page.getByRole("button",{name:"Install and restart"}).click();
  await expect(page.getByRole("alert")).toContainText("Test installer failure");
  expect(await page.evaluate(()=>(window as any).updateCalls.filter((c:string)=>c==="update_install").length)).toBe(1);
});
