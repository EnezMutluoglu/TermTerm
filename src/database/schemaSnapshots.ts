export type MetadataRow = Record<string, string | null>;
export type SchemaObject = {
  name: string;
  type: string;
  detail: Record<string, MetadataRow[]>;
};
export type SchemaSnapshot = {
  format: number;
  id: string;
  recordId: string;
  source: string;
  schema: string;
  engine: string;
  username: string;
  version: string;
  capturedAt: number;
  coverage: string;
  objects: SchemaObject[];
};
export type SnapshotSummary = Omit<SchemaSnapshot, "objects"> & {
  objectCount: number;
};
export type SchemaChange = {
  object: string;
  section: string;
  name: string;
  kind: "appeared" | "not_visible" | "changed";
  before: string;
  after: string;
  beforeLabel?: string;
  afterLabel?: string;
};
const sections: Record<string, { title: string; keys: string[] }> = {
  columns: { title: "Sütun", keys: ["column_name"] },
  indexes: {
    title: "İndeks",
    keys: ["index_name", "ordinal_position", "column_name"],
  },
  primaryKeys: { title: "Birincil anahtar", keys: ["pk_name", "key_seq"] },
  foreignKeys: { title: "Yabancı anahtar", keys: ["fk_name", "key_seq"] },
  constraints: { title: "Kısıt", keys: ["constraint_name"] },
};
const volatile = new Set([
  "cardinality",
  "pages",
  "table_cat",
  "table_schem",
  "table_name",
]);
const labels: Record<string, string> = {
  type_name: "Tür",
  column_size: "Uzunluk",
  decimal_digits: "Ondalık",
  is_nullable: "NULL izni",
  column_def: "Varsayılan",
  non_unique: "Tekil değil",
  filter_condition: "Koşul",
  is_included_column: "INCLUDE",
  is_disabled: "Devre dışı",
  constraint_type: "Kısıt türü",
  status: "Durum",
  pktable_name: "Hedef tablo",
  pkcolumn_name: "Hedef sütun",
  column_name: "Sütun",
};
function rowLabel(row?: MetadataRow, changed?: string[]) {
  if (!row) return "";
  const keys = changed ?? Object.keys(labels);
  return keys
    .filter((k) => k in labels && row[k] != null)
    .slice(0, 4)
    .map((k) => `${labels[k]}: ${row[k]}`)
    .join(" · ");
}
function normalized(row: MetadataRow) {
  return Object.fromEntries(
    Object.keys(row)
      .sort()
      .filter((k) => !volatile.has(k))
      .map((k) => [k, row[k]]),
  );
}
export function compareSchemaSnapshots(
  before: SchemaSnapshot,
  after: SchemaSnapshot,
): SchemaChange[] {
  if (before.format !== 1 || after.format !== 1)
    throw Error("Görüntü sürümü desteklenmiyor.");
  if (
    before.coverage !== "visible_metadata" ||
    after.coverage !== "visible_metadata"
  )
    throw Error("Eksik görüntüler karşılaştırılamaz.");
  if (
    before.source !== after.source ||
    before.schema !== after.schema ||
    before.engine !== after.engine ||
    before.recordId !== after.recordId
  )
    throw Error(
      "Aynı bağlantı, sunucu, veritabanı, kullanıcı ve şemaya ait iki görüntü seçin.",
    );
  const changes: SchemaChange[] = [],
    a = new Map(before.objects.map((o) => [o.name, o])),
    b = new Map(after.objects.map((o) => [o.name, o]));
  for (const name of [...new Set([...a.keys(), ...b.keys()])].sort()) {
    const old = a.get(name),
      now = b.get(name);
    if (!old || !now) {
      changes.push({
        object: name,
        section: "Nesne",
        name,
        kind: old ? "not_visible" : "appeared",
        before: old?.type ?? "",
        after: now?.type ?? "",
      });
      continue;
    }
    if (old.type !== now.type)
      changes.push({
        object: name,
        section: "Nesne türü",
        name,
        kind: "changed",
        before: old.type,
        after: now.type,
      });
    for (const [section, definition] of Object.entries(sections)) {
      // A missing section is a malformed/incomplete snapshot, never an empty schema.
      if (
        !Array.isArray(old.detail[section]) ||
        !Array.isArray(now.detail[section])
      )
        throw Error("Nesne ayrıntıları eksik; karşılaştırma durduruldu.");
      const collect = (rows: MetadataRow[]) => {
        const map = new Map<string, MetadataRow>();
        for (const row of rows) {
          if (section === "indexes" && row.index_name == null) continue; // JDBC table statistics, not an index.
          const key = JSON.stringify(
            definition.keys.map((k) => row[k] ?? null),
          );
          if (map.has(key))
            throw Error("Belirsiz metadata kimliği; karşılaştırma durduruldu.");
          map.set(key, normalized(row));
        }
        return map;
      };
      const left = collect(old.detail[section]),
        right = collect(now.detail[section]);
      for (const key of [
        ...new Set([...left.keys(), ...right.keys()]),
      ].sort()) {
        const l = left.get(key),
          r = right.get(key);
        if (JSON.stringify(l) === JSON.stringify(r)) continue;
        const fields = [
          ...new Set([...Object.keys(l ?? {}), ...Object.keys(r ?? {})]),
        ].filter((k) => l?.[k] !== r?.[k]);
        const text = (v?: MetadataRow) =>
          v ? fields.map((k) => `${k}: ${v[k] ?? "NULL"}`).join(" · ") : "";
        changes.push({
          object: name,
          section: definition.title,
          name: definition.keys
            .map((k) => (r ?? l)![k])
            .filter(Boolean)
            .join(" / "),
          kind: !l ? "appeared" : !r ? "not_visible" : "changed",
          before: text(l),
          after: text(r),
          beforeLabel: rowLabel(l, l && r ? fields : undefined),
          afterLabel: rowLabel(r, l && r ? fields : undefined),
        });
      }
    }
  }
  return changes;
}
