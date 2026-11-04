import { useEffect, useState } from "react";
import { call, errorText } from "../api";
import {
  MetadataSelect,
  ObjectPicker,
  qualifiedName,
  useMetadataSession,
} from "./MetadataPicker";
import type { DatabaseEngine } from "./types";
import type { DatabaseConnection } from "./types";
export interface ParameterSpec {
  type: string;
  default: unknown;
  min?: number;
  max?: number;
  maxLength?: number;
  nullable?: boolean;
  values?: string[];
}
export function defaults(
  specs: Record<string, ParameterSpec>,
  p?: DatabaseConnection,
) {
  return Object.fromEntries(
    Object.entries(specs).map(([name, s]) => [
      name,
      s.nullable
        ? null
        : s.type === "datetime"
          ? new Date(
              Date.now() - (name === "from_time" ? 3600000 : 0),
            ).toISOString()
          : /^(db_name|database_name)$/.test(name)
            ? (p?.database ?? "")
            : /user|login|role|owner/.test(name) && s.type !== "textArray"
              ? (p?.username ?? "")
              : s.default,
    ]),
  );
}
export default function ActionParameters({
  specs,
  values,
  onChange,
  recordId,
  sessionId,
  engine,
}: {
  specs: Record<string, ParameterSpec>;
  values: Record<string, any>;
  onChange: (v: Record<string, any>) => void;
  recordId?: string;
  sessionId?: string;
  engine: DatabaseEngine;
}) {
  const needsMetadata = Object.values(specs).some((s) =>
    [
      "object",
      "columns",
      "column",
      "schema",
      "tableName",
      "tableReference",
    ].includes(s.type),
  );
  const { session, error } = useMetadataSession(
    needsMetadata ? recordId : undefined,
    sessionId,
  );
  const [references, setReferences] = useState<
    Record<string, { schema: string; table: string }>
  >({});
  const set = (name: string, value: unknown) => {
    const next = { ...values, [name]: value };
    if (specs[name].type === "object" || specs[name].type === "schema") {
      for (const [key, s] of Object.entries(specs)) {
        if (s.type === "columns") next[key] = [];
        if (["column", "tableName"].includes(s.type)) next[key] = "";
      }
    }
    onChange(next);
  };
  return (
    <div className="db-action-parameters">
      {error && <p role="alert">{error}</p>}
      {Object.entries(specs)
        .sort(
          ([, a], [, b]) =>
            (["object", "tableReference", "schema"].includes(a.type) ? 0 : 1) -
            (["object", "tableReference", "schema"].includes(b.type) ? 0 : 1),
        )
        .map(([name, s]) => (
          <div className="db-parameter-field" data-kind={s.type} key={name}>
            <label>
              {name}{" "}
              <small>
                {s.type}
                {s.min !== undefined ? ` · ${s.min}–${s.max}` : ""}
              </small>
            </label>
            {s.type === "object" ? (
              <ObjectPicker
                sessionId={session}
                label={name}
                value={values[name] ?? {}}
                onChange={(v) => set(name, v)}
              />
            ) : s.type === "tableReference" ? (
              <>
                <ObjectPicker
                  sessionId={session}
                  label={name}
                  value={references[name] ?? {}}
                  onChange={(v) => {
                    setReferences({ ...references, [name]: v });
                    set(name, qualifiedName(v.schema, v.table, engine));
                  }}
                />
                <small>{values[name] || "Şema ve tablo seçin"}</small>
              </>
            ) : s.type === "schema" ? (
              <MetadataSelect
                sessionId={session}
                kind="schemas"
                label={name}
                value={values[name] ?? ""}
                optional={s.nullable}
                onChange={(v) => set(name, v || (s.nullable ? null : ""))}
              />
            ) : s.type === "tableName" ? (
              <MetadataSelect
                sessionId={session}
                kind="tables"
                schema={values.owner_user ?? values.schema_name ?? ""}
                label={name}
                value={values[name] ?? ""}
                onChange={(v) => set(name, v)}
              />
            ) : s.type === "column" ? (
              <MetadataSelect
                sessionId={session}
                kind="columns"
                schema={values.table?.schema}
                table={values.table?.table}
                label={name}
                value={values[name] ?? ""}
                onChange={(v) => set(name, v)}
              />
            ) : s.type === "columns" ? (
              <ColumnSelection
                sessionId={session}
                schema={values.table?.schema ?? ""}
                table={values.table?.table ?? ""}
                name={name}
                value={values[name] ?? []}
                onChange={(v) => set(name, v)}
              />
            ) : s.type === "enum" ? (
              <select
                aria-label={name}
                value={values[name] ?? s.default}
                onChange={(e) => set(name, e.target.value)}
              >
                {s.values?.map((v) => (
                  <option key={v}>{v}</option>
                ))}
              </select>
            ) : s.type === "textArray" ? (
              <input
                aria-label={name}
                placeholder="Virgülle ayrılmış değerler"
                value={(values[name] ?? []).join(",")}
                onChange={(e) =>
                  set(
                    name,
                    e.target.value
                      .split(",")
                      .map((x) => x.trim())
                      .filter(Boolean),
                  )
                }
              />
            ) : (
              <input
                aria-label={name}
                type={s.type === "integer" ? "number" : "text"}
                min={s.min}
                max={s.max}
                maxLength={s.maxLength}
                value={values[name] ?? ""}
                placeholder={
                  s.nullable
                    ? "Boş: tümü / NULL"
                    : s.type === "datetime"
                      ? "2026-10-05T12:00:00Z"
                      : ""
                }
                onChange={(e) =>
                  set(
                    name,
                    s.type === "integer"
                      ? Number(e.target.value)
                      : s.nullable && !e.target.value
                        ? null
                        : e.target.value,
                  )
                }
              />
            )}
          </div>
        ))}
    </div>
  );
}
function ColumnSelection({
  sessionId,
  schema,
  table,
  name,
  value,
  onChange,
}: {
  sessionId: string;
  schema: string;
  table: string;
  name: string;
  value: string[];
  onChange: (v: string[]) => void;
}) {
  const [items, setItems] = useState<string[]>([]),
    [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    setItems([]);
    setError("");
    if (sessionId && schema && table)
      void call<{ items: string[] }>("database_request", {
        request: {
          op: "metadataOptions",
          sessionId,
          kind: "columns",
          schema,
          table,
        },
      })
        .then((r) => {
          if (live) setItems(r.items);
        })
        .catch((e) => {
          if (live) setError(errorText(e));
        });
    return () => {
      live = false;
    };
  }, [sessionId, schema, table]);
  return (
    <div className="db-column-selection" aria-label={name}>
      {items.length === 0 && (
        <small>{error || "Sütunları yüklemek için tablo seçin."}</small>
      )}
      {items.map((c) => (
        <label key={c}>
          <input
            type="checkbox"
            aria-label={`${name} ${c}`}
            checked={value.includes(c)}
            onChange={(e) =>
              onChange(
                e.target.checked ? [...value, c] : value.filter((v) => v !== c),
              )
            }
          />
          {c}
        </label>
      ))}
    </div>
  );
}
