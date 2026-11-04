export interface SqlStep {
  title: string;
  sql: string;
  description: string;
}
export function documentData(
  kind: "savedQuery" | "databaseRunbook",
  label: string,
  description: string,
  connectionId: string,
  groupId: string,
  steps: SqlStep[],
) {
  if (!label.trim() || label.length > 160)
    throw Error("Belge adı 1–160 karakter olmalı.");
  if (
    !steps.length ||
    steps.length > 50 ||
    steps.some(
      (s) => !s.sql.trim() || (kind === "databaseRunbook" && !s.title.trim()),
    )
  )
    throw Error("Her adımın başlığı ve SQL'i dolu olmalı (en fazla 50 adım).");
  const base = { label: label.trim(), description, connectionId, groupId };
  return kind === "savedQuery"
    ? { ...base, sql: steps[0].sql }
    : {
        ...base,
        steps: steps.map((s) => ({
          title: s.title.trim(),
          description: s.description,
          sql: s.sql,
        })),
      };
}
