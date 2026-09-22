import { newEntity, type Entity } from "./types";
import { UNGROUPED_FOLDER } from "./hostFolders";

// Expand only group containment. Credentials and external jump hosts remain shared.
export function containedRecords(records: Entity[], ids: string[]) {
  const selected = new Set(ids);
  const byId = new Map(records.map((r) => [r.id, r]));
  const children = new Map<string, Entity[]>();
  for (const record of records) {
    const key =
      record.data.groupId || (record.kind === "host" ? UNGROUPED_FOLDER : "");
    if (!children.has(key)) children.set(key, []);
    children.get(key)!.push(record);
  }
  const queue = ids.filter(
    (id) => id === UNGROUPED_FOLDER || byId.get(id)?.kind === "group",
  );
  for (let i = 0; i < queue.length; i++)
    for (const child of children.get(queue[i]) ?? []) {
      if (selected.has(child.id)) continue;
      selected.add(child.id);
      if (child.kind === "group") queue.push(child.id);
    }
  return records.filter((r) => selected.has(r.id));
}

export function duplicateRecords(
  records: Entity[],
  ids: string[],
  destination?: string,
) {
  const existing = new Set(records.map((r) => r.id));
  if (ids.some((id) => !existing.has(id)))
    throw Error(
      "A selected record no longer exists. Select the records again.",
    );
  if (
    destination &&
    !records.some((r) => r.id === destination && r.kind === "group")
  )
    throw Error("Destination group no longer exists.");
  const originals = containedRecords(records, ids),
    included = new Set(originals.map((r) => r.id));
  const remap = new Map(originals.map((r) => [r.id, crypto.randomUUID()]));
  return originals.map((r) => {
    const data = structuredClone(r.data);
    const root = !included.has(data.groupId);
    if (root) data.label = `${data.label || r.kind} copy`;
    for (const field of ["groupId", "credentialId", "hostId"])
      if (remap.has(data[field])) data[field] = remap.get(data[field]);
    for (const field of ["chain", "hostIds", "inventoryGroups"])
      if (Array.isArray(data[field]))
        data[field] = data[field].map((id: string) => remap.get(id) ?? id);
    if (destination !== undefined && root && ["host", "group"].includes(r.kind))
      data.groupId = destination;
    return { ...newEntity(r.kind, data), id: remap.get(r.id)! };
  });
}

export function moveRecords(
  records: Entity[],
  ids: string[],
  destination: string,
) {
  const selected = new Set(ids),
    selection = records.filter((r) => selected.has(r.id));
  if (
    selection.length !== new Set(ids).size ||
    selection.some((r) => !["host", "group"].includes(r.kind))
  )
    throw Error("Only existing hosts and groups can be moved.");
  if (
    destination &&
    !records.some((r) => r.id === destination && r.kind === "group")
  )
    throw Error("Destination group no longer exists.");
  const descendants = new Set(containedRecords(records, ids).map((r) => r.id));
  if (descendants.has(destination))
    throw Error("A group cannot be moved into itself or a child group.");
  // If parent and child are selected together, keep the child inside its parent.
  return selection
    .filter((r) => !descendants.has(r.data.groupId))
    .map((r) => ({ ...r, data: { ...r.data, groupId: destination } }));
}

export function connectionDetails(record: Entity, records: Entity[]) {
  const byId = new Map(records.map((r) => [r.id, r]));
  const ancestry: Entity[] = [record],
    seen = new Set([record.id]);
  let parent = byId.get(record.data.groupId);
  while (parent && !seen.has(parent.id)) {
    ancestry.unshift(parent);
    seen.add(parent.id);
    parent = byId.get(parent.data.groupId);
  }
  const data: Entity["data"] = {};
  const inheritedEmpty = [
    "username",
    "password",
    "credentialId",
    "keyPath",
    "privateKey",
    "certificatePath",
    "certificate",
    "passphrase",
    "startup",
  ];
  for (const item of ancestry)
    for (const [key, value] of Object.entries(item.data)) {
      if (value == null || (value === "" && inheritedEmpty.includes(key)))
        continue;
      data[key] =
        key === "environment" ? { ...data.environment, ...value } : value;
    }
  const credential = byId.get(data.credentialId);
  if (credential)
    for (const field of [
      "username",
      "password",
      "privateKey",
      "keyPath",
      "certificatePath",
      "certificate",
      "passphrase",
      "agent",
      "agentKey",
    ])
      if (!data[field]) data[field] = credential.data[field];
  return data;
}
