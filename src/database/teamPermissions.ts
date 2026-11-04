import type { Entity } from "../types";

export type SqlPermission =
  "sql_run" | "sql_monitor" | "sql_manage" | "sql_export" | "edit" | "reveal";
export function sqlAllowed(
  team: boolean,
  record: Entity | undefined,
  permission: SqlPermission,
): boolean {
  if (!team) return true;
  return (
    !!record &&
    Array.isArray(record.data._teamPermissions) &&
    record.data._teamPermissions.includes("read") &&
    record.data._teamPermissions.includes(permission)
  );
}
export function sqlConnectAllowed(
  team: boolean,
  record: Entity | undefined,
): boolean {
  return ["sql_run", "sql_monitor", "sql_manage"].some((p) =>
    sqlAllowed(team, record, p as SqlPermission),
  );
}
