import { describe, expect, it } from "vitest";
import { backupObservation, evidenceRows } from "../src/database/evidence";
import { qualifiedName } from "../src/database/MetadataPicker";
describe("Database evidence", () => {
  it("does not treat absent, future or malformed backup age as healthy", () => {
    for (const age of [null, "", "NaN", "-1"])
      expect(backupObservation({ age_seconds: age }, 24).state).toBe("unknown");
  });
  it("distinguishes age from recovery verification and reports failed/damaged jobs", () => {
    expect(backupObservation({ age_seconds: "3600" }, 24)).toEqual({
      state: "observed",
      text: expect.stringContaining("restore doğrulanmadı"),
    });
    expect(backupObservation({ age_seconds: "86401" }, 24).state).toBe(
      "warning",
    );
    expect(
      backupObservation({ age_seconds: "1", status: "FAILED" }, 24).state,
    ).toBe("critical");
    expect(
      backupObservation({ age_seconds: "1", is_damaged: "1" }, 24).state,
    ).toBe("critical");
    expect(
      backupObservation(
        { age_seconds: "1", status: "COMPLETED WITH WARNINGS" },
        24,
      ).state,
    ).toBe("warning");
  });
  it("preserves null and decimal evidence and normalizes Oracle column labels", () => {
    expect(
      evidenceRows({
        id: "x",
        title: "x",
        note: "",
        status: "available",
        columns: [
          { name: "AGE_SECONDS", type: "NUMERIC" },
          { name: "BYTES", type: "NUMERIC" },
        ],
        rows: [[null, "9007199254740993"]],
      }),
    ).toEqual([{ age_seconds: null, bytes: "9007199254740993" }]);
  });
  it("quotes qualified object names without interpreting punctuation", () => {
    expect(qualifiedName('a"b', "İ,_%", "postgresql")).toBe('"a""b"."İ,_%"');
    expect(qualifiedName("a]b", "x", "mssql")).toBe("[a]]b].[x]");
    expect(qualifiedName("", "x", "oracle")).toBe("");
  });
});
