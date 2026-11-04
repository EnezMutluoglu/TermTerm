import { describe, it, expect } from "vitest";
import {
  compareSchemaSnapshots,
  type SchemaSnapshot,
} from "../src/database/schemaSnapshots";
const fixture = (): SchemaSnapshot => ({
  format: 1,
  id: "a",
  recordId: "connection",
  source: "server-db-user",
  schema: "İstanbul_%",
  engine: "postgresql",
  username: "user",
  version: "18",
  capturedAt: 1,
  coverage: "visible_metadata",
  objects: [
    {
      name: "Ödemeler",
      type: "TABLE",
      detail: {
        columns: [
          { column_name: "tutar", type_name: "numeric", nullable: "1" },
        ],
        indexes: [
          {
            index_name: "ix",
            ordinal_position: "1",
            column_name: "tutar",
            cardinality: "10",
            pages: "1",
          },
        ],
        primaryKeys: [],
        foreignKeys: [],
        constraints: [],
      },
    },
  ],
});
describe("schema snapshot comparison", () => {
  it("ignores row/key order and statistics while preserving definitions", () => {
    const a = fixture(),
      b = fixture();
    b.objects[0].detail.indexes[0] = {
      pages: "999",
      cardinality: "9999",
      column_name: "tutar",
      ordinal_position: "1",
      index_name: "ix",
    };
    expect(compareSchemaSnapshots(a, b)).toEqual([]);
    b.objects[0].detail.columns[0].nullable = "0";
    expect(compareSchemaSnapshots(a, b)).toEqual([
      expect.objectContaining({
        kind: "changed",
        before: "nullable: 1",
        after: "nullable: 0",
        object: "Ödemeler",
      }),
    ]);
  });
  it("reports additions and lost visibility without claiming DROP", () => {
    const a = fixture(),
      b = fixture();
    b.objects = [];
    expect(compareSchemaSnapshots(a, b)[0].kind).toBe("not_visible");
    expect(compareSchemaSnapshots(b, a)[0].kind).toBe("appeared");
    b.objects = structuredClone(a.objects);
    b.objects[0].detail.columns = [];
    expect(compareSchemaSnapshots(a, b)[0]).toMatchObject({
      kind: "not_visible",
      section: "Sütun",
    });
  });
  it.each(["source", "schema", "engine", "recordId"] as const)(
    "rejects mismatched %s",
    (key) => {
      const a = fixture(),
        b = fixture();
      b[key] = "different";
      expect(() => compareSchemaSnapshots(a, b)).toThrow("Aynı bağlantı");
    },
  );
  it("rejects partial, future and malformed metadata", () => {
    const a = fixture(),
      b = fixture();
    b.coverage = "partial";
    expect(() => compareSchemaSnapshots(a, b)).toThrow("Eksik");
    b.coverage = "visible_metadata";
    b.format = 2;
    expect(() => compareSchemaSnapshots(a, b)).toThrow("sürümü");
    b.format = 1;
    delete b.objects[0].detail.columns;
    expect(() => compareSchemaSnapshots(a, b)).toThrow("eksik");
  });
  it("detects ambiguous row identity and skips JDBC table statistics", () => {
    const a = fixture(),
      b = fixture();
    b.objects[0].detail.indexes.push({ index_name: null, cardinality: "55" });
    expect(compareSchemaSnapshots(a, b)).toEqual([]);
    b.objects[0].detail.columns.push({ ...b.objects[0].detail.columns[0] });
    expect(() => compareSchemaSnapshots(a, b)).toThrow("Belirsiz");
  });
});
