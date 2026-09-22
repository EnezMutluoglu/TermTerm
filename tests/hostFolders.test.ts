import { describe, expect, it } from "vitest";
import {
  indexHostFolders,
  hostInFolder,
  storedGroupId,
  UNGROUPED_FOLDER,
} from "../src/hostFolders";
import type { Entity } from "../src/types";

const row = (id: string, kind: Entity["kind"], groupId?: string): Entity => ({
  id,
  kind,
  data: { label: id, ...(groupId === undefined ? {} : { groupId }) },
  updatedAt: 0,
});

describe("folder host counts", () => {
  it("counts descendants without showing them as root or parent contents", () => {
    const records = [
      row("deep-host", "host", "deep"),
      row("deep", "group", "child"),
      row("parent", "group"),
      row("child", "group", "parent"),
      row("direct", "host", "child"),
      row("root-a", "host"),
      row("root-b", "host", ""),
      row("other", "group"),
      row("other-host", "host", "other"),
      row("key", "credential", "child"),
    ];
    const index = indexHostFolders(records);
    expect(index.totalHosts.get("parent")).toBe(2);
    expect(index.totalHosts.get("child")).toBe(2);
    expect(index.totalHosts.get("deep")).toBe(1);
    expect(index.totalHosts.get("other")).toBe(1);
    expect(index.directHosts.get("parent") ?? 0).toBe(0);
    expect(index.directHosts.get("")).toBe(2);
    expect(index.children.get("")?.map((g) => g.id)).toEqual([
      "parent",
      "other",
      UNGROUPED_FOLDER,
    ]);
    expect(index.hostCount).toBe(5);
    expect(
      records
        .filter((r) => r.kind === "host" && (r.data.groupId ?? "") === "")
        .map((r) => r.id),
    ).toEqual(["root-a", "root-b"]);
  });
  it("shows no loose hosts at the root and keeps Ungrouped virtual", () => {
    const records = [
      row("g", "group"),
      row("nested", "host", "g"),
      row("loose", "host"),
      row("loose2", "host", ""),
    ];
    const folders = indexHostFolders(records);
    expect(records.filter((r) => hostInFolder(r, ""))).toEqual([]);
    expect(
      records.filter((r) => hostInFolder(r, "g")).map((r) => r.id),
    ).toEqual(["nested"]);
    expect(
      records.filter((r) => hostInFolder(r, UNGROUPED_FOLDER)).map((r) => r.id),
    ).toEqual(["loose", "loose2"]);
    expect(folders.totalHosts.get(UNGROUPED_FOLDER)).toBe(2);
    expect(folders.groups.map((r) => r.id)).toEqual(["g"]);
    expect(storedGroupId(UNGROUPED_FOLDER)).toBe("");
    expect(storedGroupId("g")).toBe("g");
    expect(
      indexHostFolders([row("g", "group")])
        .children.get("")
        ?.map((r) => r.id),
    ).toEqual(["g"]);
  });
  it("recalculates moves and deletions, including empty groups", () => {
    const records = [
      row("a", "group"),
      row("b", "group"),
      row("h", "host", "a"),
    ];
    expect(indexHostFolders(records).totalHosts.get("a")).toBe(1);
    records[2] = row("h", "host", "b");
    expect(indexHostFolders(records).totalHosts.get("a")).toBe(0);
    expect(indexHostFolders(records).totalHosts.get("b")).toBe(1);
    expect(indexHostFolders(records.slice(0, 2)).totalHosts.get("b")).toBe(0);
  });
  it("terminates on cycles and counts unrelated roots correctly", () => {
    const index = indexHostFolders([
      row("a", "group", "b"),
      row("b", "group", "a"),
      row("h", "host", "a"),
      row("root", "host"),
    ]);
    expect(index.totalHosts.get("a")).toBe(1);
    expect(index.totalHosts.get("b")).toBe(1);
    expect(index.directHosts.get("")).toBe(1);
  });
});
