import { describe, expect, it } from "vitest";
import { sqlAllowed, sqlConnectAllowed } from "../src/database/teamPermissions";
import { newEntity } from "../src/types";
describe("Team SQL UI permissions", () => {
  const record = (permissions: string[]) =>
    newEntity("databaseConnection", { _teamPermissions: permissions });
  it("does not turn SSH connect or edit into SQL execution", () => {
    const r = record(["read", "connect", "edit"]);
    expect(sqlConnectAllowed(true, r)).toBe(false);
    expect(sqlAllowed(true, r, "sql_run")).toBe(false);
  });
  it("separates monitoring, execution, management and export", () => {
    const r = record(["read", "sql_monitor"]);
    expect(sqlConnectAllowed(true, r)).toBe(true);
    expect(sqlAllowed(true, r, "sql_monitor")).toBe(true);
    for (const p of ["sql_run", "sql_manage", "sql_export"] as const)
      expect(sqlAllowed(true, r, p)).toBe(false);
    expect(sqlConnectAllowed(true, record(["sql_run"]))).toBe(false);
  });
  it("preserves personal vault permissions", () => {
    expect(sqlConnectAllowed(false, record([]))).toBe(true);
    expect(sqlAllowed(false, record([]), "sql_manage")).toBe(true);
  });
});
