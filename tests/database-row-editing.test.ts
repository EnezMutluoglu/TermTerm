import { expect, it } from "vitest";
import { rowChanges } from "../src/database/rowEditing";
it("keeps exact decimals, NULL and empty strings distinct", () => {
  expect(rowChanges({ amount: "9007199254740993.0001", note: null, label: "" }, { amount: "9007199254740993.0002", note: "", label: null })).toEqual({ amount: "9007199254740993.0002", note: "", label: null });
});
it("does not propose unchanged or unknown fields", () => {
  expect(rowChanges({ name: "İstanbul", note: null }, { name: "İstanbul", note: null, injected: "DROP TABLE x" })).toEqual({});
});
