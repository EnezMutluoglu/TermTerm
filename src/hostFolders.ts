import type { Entity } from "./types";

export const UNGROUPED_FOLDER = "__ungrouped__";
export const storedGroupId = (folder: string) =>
  folder === UNGROUPED_FOLDER ? "" : folder;

export function hostInFolder(record: Entity, folder: string) {
  return (
    record.kind === "host" &&
    folder !== "" &&
    (record.data.groupId || UNGROUPED_FOLDER) === folder
  );
}

export function indexHostFolders(records: Entity[]) {
  const groups = records.filter((record) => record.kind === "group");
  const byId = new Map(groups.map((group) => [group.id, group]));
  const children = new Map<string, Entity[]>();
  const directHosts = new Map<string, number>();
  const totalHosts = new Map<string, number>();
  for (const group of groups) {
    const parent = group.data.groupId || "";
    const siblings = children.get(parent) ?? [];
    siblings.push(group);
    children.set(parent, siblings);
    totalHosts.set(group.id, 0);
  }
  let hostCount = 0;
  for (const record of records) {
    if (record.kind !== "host") continue;
    hostCount++;
    const parent = record.data.groupId || "";
    directHosts.set(parent, (directHosts.get(parent) ?? 0) + 1);
  }
  // Aggregate each folder's direct hosts up its ancestry once, regardless of
  // record order. A malformed imported cycle must not hang the workspace.
  for (const group of groups) {
    const count = directHosts.get(group.id) ?? 0;
    const seen = new Set<string>();
    let current: Entity | undefined = group;
    while (current && !seen.has(current.id)) {
      seen.add(current.id);
      totalHosts.set(current.id, (totalHosts.get(current.id) ?? 0) + count);
      current = byId.get(current.data.groupId);
    }
  }
  const ungroupedCount = directHosts.get("") ?? 0;
  // This is a view of unassigned hosts, never a persisted group relationship.
  const ungrouped: Entity = {
    id: UNGROUPED_FOLDER,
    kind: "group",
    data: { label: "Ungrouped" },
    updatedAt: 0,
  };
  byId.set(UNGROUPED_FOLDER, ungrouped);
  totalHosts.set(UNGROUPED_FOLDER, ungroupedCount);
  if (ungroupedCount > 0) {
    children.set("", [...(children.get("") ?? []), ungrouped]);
  }
  return { groups, byId, children, directHosts, totalHosts, hostCount };
}
