import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import type { DatabaseEngine } from "./types";
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
export function useMetadataSession(recordId?: string, existing?: string) {
  const [session, setSession] = useState(""),
    [error, setError] = useState("");
  useEffect(() => {
    let live = true,
      owned = "";
    setSession(existing ?? "");
    setError("");
    if (!existing && recordId)
      void rpc<{ sessionId: string }>({ op: "open", recordId })
        .then((r) => {
          owned = r.sessionId;
          if (live) setSession(owned);
          else void rpc({ op: "close", sessionId: owned }).catch(() => {});
        })
        .catch((e) => {
          if (live) setError(errorText(e));
        });
    return () => {
      live = false;
      if (owned) void rpc({ op: "close", sessionId: owned }).catch(() => {});
    };
  }, [recordId, existing]);
  return { session: existing ?? session, error };
}
export function MetadataSelect({
  sessionId,
  kind,
  schema = "",
  table = "",
  value,
  onChange,
  label,
  optional = false,
}: {
  sessionId: string;
  kind: "schemas" | "tables" | "columns";
  schema?: string;
  table?: string;
  value: string;
  onChange: (value: string) => void;
  label: string;
  optional?: boolean;
}) {
  const [items, setItems] = useState<string[]>([]),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [refresh, setRefresh] = useState(0),
    [filter, setFilter] = useState("");
  const epoch = useRef(0);
  useEffect(() => {
    const id = ++epoch.current;
    setItems([]);
    setError("");
    setFilter("");
    if (
      !sessionId ||
      (kind !== "schemas" && !schema) ||
      (kind === "columns" && !table)
    ) {
      setBusy(false);
      return;
    }
    setBusy(true);
    void rpc<{ items: string[] }>({
      op: "metadataOptions",
      sessionId,
      kind,
      schema,
      table,
    })
      .then((r) => {
        if (id === epoch.current) setItems(r.items);
      })
      .catch((e) => {
        if (id === epoch.current) setError(errorText(e));
      })
      .finally(() => {
        if (id === epoch.current) setBusy(false);
      });
    return () => {
      epoch.current++;
    };
  }, [sessionId, kind, schema, table, refresh]);
  return (
    <div className="db-metadata-select">
      <input
        aria-label={`${label} filtre`}
        placeholder="İsimde ara…"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      <select
        aria-label={label}
        title={value || label}
        value={value}
        disabled={busy || !sessionId}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">
          {busy ? "Yükleniyor…" : optional ? "Tümü / NULL" : "Seçin…"}
        </option>
        {value && !items.includes(value) && (
          <option value={value}>{value} · görünürlüğü doğrulanmadı</option>
        )}
        {items
          .filter(
            (i) =>
              i === value ||
              i.toLocaleLowerCase().includes(filter.toLocaleLowerCase()),
          )
          .map((i) => (
            <option key={i} value={i}>
              {i}
            </option>
          ))}
      </select>
      <button
        type="button"
        disabled={busy || !sessionId}
        aria-label={`${label} yenile`}
        onClick={() => setRefresh((v) => v + 1)}
      >
        Yenile
      </button>
      {error && <small role="alert">{error}</small>}
      {!busy && !error && sessionId && items.length === 0 && (
        <small>Görünür nesne yok veya üst seçim gerekli.</small>
      )}
    </div>
  );
}
export const qualifiedName = (
  schema: string,
  table: string,
  engine: DatabaseEngine,
) => {
  const q = (s: string) =>
    engine === "mssql"
      ? `[${s.replaceAll("]", "]]")}]`
      : `"${s.replaceAll('"', '""')}"`;
  return schema && table ? `${q(schema)}.${q(table)}` : "";
};
export function ObjectPicker({
  sessionId,
  value,
  onChange,
  label,
}: {
  sessionId: string;
  value: { schema?: string; table?: string };
  onChange: (value: { schema: string; table: string }) => void;
  label: string;
}) {
  return (
    <div className="db-object-picker">
      <MetadataSelect
        sessionId={sessionId}
        kind="schemas"
        value={value.schema ?? ""}
        label={`${label} şema`}
        onChange={(schema) => onChange({ schema, table: "" })}
      />
      <MetadataSelect
        sessionId={sessionId}
        kind="tables"
        schema={value.schema}
        value={value.table ?? ""}
        label={`${label} tablo`}
        onChange={(table) => onChange({ schema: value.schema ?? "", table })}
      />
    </div>
  );
}
