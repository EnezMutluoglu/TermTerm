import { describe, it, expect } from "vitest";
import {
  containedRecords,
  duplicateRecords,
  moveRecords,
  connectionDetails,
} from "../src/recordActions";
import { UNGROUPED_FOLDER } from "../src/hostFolders";
import type { Entity } from "../src/types";
const record = (
  id: string,
  kind: Entity["kind"],
  data: Entity["data"],
): Entity => ({ id, kind, data, updatedAt: 1 });
const records = [
  record("group", "group", {
    label: "Production",
    username: "deploy",
    chain: ["jump"],
  }),
  record("child", "group", { label: "Nested", groupId: "group" }),
  record("a", "host", {
    label: "API",
    address: "example.test",
    groupId: "child",
    credentialId: "key",
    chain: ["jump", "b"],
  }),
  record("b", "host", { label: "DB", address: "db.test", groupId: "group" }),
  record("jump", "host", { label: "Gateway", address: "jump.test" }),
  record("key", "credential", { label: "Key", privateKey: "fixture-key" }),
];
describe("record context actions", () => {
  it("copies complete groups and remaps internal references while retaining external credentials and jumps", () => {
    const copy = duplicateRecords(records, ["group"]),
      group = copy.find((r) => r.data.label === "Production copy")!,
      nested = copy.find((r) => r.data.label === "Nested")!,
      api = copy.find((r) => r.data.label === "API")!,
      db = copy.find((r) => r.data.label === "DB")!;
    expect(copy).toHaveLength(4);
    expect(nested.data.groupId).toBe(group.id);
    expect(api.data.groupId).toBe(nested.id);
    expect(api.data.chain).toEqual(["jump", db.id]);
    expect(api.data.credentialId).toBe("key");
    expect(records[2].data.chain).toEqual(["jump", "b"]);
  });
  it("moves only selected roots and rejects cycles or a vanished destination", () => {
    expect(moveRecords(records, ["child", "a"], "").map((r) => r.id)).toEqual([
      "child",
    ]);
    expect(() => moveRecords(records, ["group"], "child")).toThrow(
      /child group/,
    );
    expect(() => moveRecords(records, ["a"], "gone")).toThrow(
      /no longer exists/,
    );
    expect(() => moveRecords(records, ["key"], "")).toThrow(/hosts and groups/);
    expect(() => duplicateRecords(records, ["a", "gone"])).toThrow(
      /no longer exists/,
    );
    expect(() => duplicateRecords(records, ["a"], "gone")).toThrow(
      /no longer exists/,
    );
  });
  it("limits virtual Ungrouped to hosts and terminates malformed group cycles", () => {
    expect(
      containedRecords(records, [UNGROUPED_FOLDER]).map((r) => r.id),
    ).toEqual(["jump"]);
    expect(
      containedRecords(
        [
          record("x", "group", { groupId: "y" }),
          record("y", "group", { groupId: "x" }),
        ],
        ["x"],
      ).map((r) => r.id),
    ).toEqual(["x", "y"]);
  });
  it("shows inherited username, identity and chain with the same empty-value rules as the backend", () => {
    const data = connectionDetails(
      {
        ...records[2],
        data: { ...records[2].data, username: "", chain: null },
      },
      records,
    );
    expect(data.username).toBe("deploy");
    expect(data.chain).toEqual(["jump"]);
    expect(data.privateKey).toBe("fixture-key");
  });
});
