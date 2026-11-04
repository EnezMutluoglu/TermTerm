import LobViewer from "./LobViewer";
import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import type { RowSet } from "./types";
import RowEditor from "./RowEditor";
import RowDelete from "./RowDelete";
import RowInsert from "./RowInsert";
import type { RowValues } from "./rowEditing";
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
type Detail = {
  constraints: Record<string, string | null>[];
  columns: Record<string, string | null>[];
  primaryKeys: Record<string, string | null>[];
  indexes: Record<string, string | null>[];
  foreignKeys: Record<string, string | null>[];
};
type Page = RowSet & {
  next: (string | null)[] | null;
  stable: boolean;
  detail: Detail;
  largeValues?: { keyColumns: string[]; columns: string[]; reason?: string };
  editing?: { enabled: boolean; reason?: string; keyColumns: string[] };
};
export default function TableBrowser({
  sessionId,
  schema,
  table,
  onColumns,
}: {
  sessionId: string;
  schema: string;
  table: string;
  onColumns: (cols: string[]) => void;
}) {
  const [page, setPage] = useState<Page | null>(null),
    [size, setSize] = useState(100),
    [sort, setSort] = useState(""),
    [direction, setDirection] = useState("ASC"),
    [column, setColumn] = useState(""),
    [operator, setOperator] = useState("="),
    [value, setValue] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [after, setAfter] = useState<(string | null)[] | null>(null),
    [previous, setPrevious] = useState<((string | null)[] | null)[]>([]),
    [section, setSection] = useState("rows"),
    [dirty, setDirty] = useState(false);
  const [snapshots, setSnapshots] = useState<{ at: string; detail: Detail }[]>(
    [],
  );
  const [largeValue, setLargeValue] = useState<{
    column: string;
    keys: RowValues;
  } | null>(null);
  const [editingKeys, setEditingKeys] = useState<RowValues | null>(null);
  const [inserting, setInserting] = useState(false);
  const [deletingKeys, setDeletingKeys] = useState<RowValues | null>(null);
  const epoch = useRef(0),
    pending = useRef(false),
    skipReload = useRef(false);
  async function load(reset = false) {
    if (pending.current) return;
    pending.current = true;
    const token = ++epoch.current;
    setBusy(true);
    setError("");
    try {
      const col = page?.detail.columns.find((c) => c.column_name === column);
      const numeric =
        col && [2, 3, 4, 5, 6, 7, 8, -5].includes(Number(col.data_type));
      const filters = column
        ? [
            {
              column,
              operator,
              value: numeric ? { kind: "decimal", value } : value,
            },
          ]
        : [];
      const result = await rpc<Page>({
        op: "browse",
        sessionId,
        schema,
        table,
        pageSize: size,
        sort,
        direction,
        filters,
        after: reset ? null : after,
      });
      if (token !== epoch.current) return;
      setPage(result);
      setDirty(false);
      onColumns(result.columns.map((c) => c.name));
      if (reset) {
        skipReload.current = after !== null;
        setAfter(null);
        setPrevious([]);
      }
    } catch (e) {
      if (token === epoch.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (token === epoch.current) setBusy(false);
    }
  }
  useEffect(() => {
    if (skipReload.current) {
      skipReload.current = false;
      return;
    }
    void load();
    return () => {
      epoch.current++;
    };
  }, [sessionId, schema, table, after]);
  const currentDetail =
    section === "columns"
      ? page?.detail.columns
      : section === "indexes"
        ? page?.detail.indexes
        : section === "keys"
          ? [
              ...(page?.detail.primaryKeys ?? []),
              ...(page?.detail.foreignKeys ?? []),
              ...(page?.detail.constraints ?? []),
            ]
          : null;
  const titles: Record<string, string> = {
    column_name: "Sütun",
    type_name: "Tür",
    column_size: "Uzunluk",
    decimal_digits: "Ondalık",
    nullable: "NULL izni",
    column_def: "Varsayılan",
    is_autoincrement: "Kimlik",
    index_name: "İndeks",
    ordinal_position: "Sıra",
    non_unique: "Tekil değil",
    filter_condition: "Koşul",
    pk_name: "PK",
    fk_name: "FK",
    constraint_name: "Kısıt",
    constraint_type: "Tür",
    fkcolumn_name: "FK sütunu",
    pkcolumn_name: "Hedef sütun",
    pktable_name: "Hedef tablo",
    key_seq: "Sıra",
    status: "Durum",
  };
  const detailKeys = (
    section === "columns"
      ? [
          "column_name",
          "type_name",
          "column_size",
          "decimal_digits",
          "nullable",
          "column_def",
          "is_autoincrement",
        ]
      : section === "indexes"
        ? [
            "index_name",
            "column_name",
            "ordinal_position",
            "non_unique",
            "filter_condition",
          ]
        : [
            "constraint_name",
            "constraint_type",
            "pk_name",
            "fk_name",
            "column_name",
            "fkcolumn_name",
            "pktable_name",
            "pkcolumn_name",
            "key_seq",
            "status",
          ]
  ).filter((k) => currentDetail?.some((row) => row[k] != null));
  const changes =
    snapshots.length === 2
      ? (
          [
            "columns",
            "indexes",
            "primaryKeys",
            "foreignKeys",
            "constraints",
          ] as const
        ).flatMap((kind) => {
          const before = new Set(
              snapshots[0].detail[kind].map((v) =>
                JSON.stringify(
                  Object.fromEntries(
                    Object.entries(v).filter(
                      ([k]) => !["cardinality", "pages"].includes(k),
                    ),
                  ),
                ),
              ),
            ),
            now = new Set(
              snapshots[1].detail[kind].map((v) =>
                JSON.stringify(
                  Object.fromEntries(
                    Object.entries(v).filter(
                      ([k]) => !["cardinality", "pages"].includes(k),
                    ),
                  ),
                ),
              ),
            );
          return [
            ...[...now]
              .filter((v) => !before.has(v))
              .map((v) => `+ ${kind}: ${v}`),
            ...[...before]
              .filter((v) => !now.has(v))
              .map((v) => `− ${kind}: ${v}`),
          ];
        })
      : [];
  if (inserting)
    return (
      <div className="db-browser">
        <RowInsert
          sessionId={sessionId}
          schema={schema}
          table={table}
          onClose={() => {
            setInserting(false);
            void load(true);
          }}
        />
      </div>
    );
  if (largeValue)
    return (
      <LobViewer
        sessionId={sessionId}
        schema={schema}
        table={table}
        column={largeValue.column}
        keys={largeValue.keys}
        onClose={() => setLargeValue(null)}
      />
    );
  if (deletingKeys)
    return (
      <div className="db-browser">
        <RowDelete
          sessionId={sessionId}
          schema={schema}
          table={table}
          keys={deletingKeys}
          onClose={() => {
            setDeletingKeys(null);
            void load(true);
          }}
        />
      </div>
    );
  if (editingKeys)
    return (
      <div className="db-browser">
        <RowEditor
          sessionId={sessionId}
          schema={schema}
          table={table}
          keys={editingKeys}
          onClose={() => {
            setEditingKeys(null);
            void load(true);
          }}
        />
      </div>
    );
  return (
    <div className="db-browser">
      <div className="db-toolbar">
        <select
          aria-label="Sayfa boyutu"
          value={size}
          onChange={(e) => {
            setSize(Number(e.target.value));
            setDirty(true);
          }}
        >
          {[100, 500, 1000].map((n) => (
            <option key={n}>{n}</option>
          ))}
        </select>
        <select
          aria-label="Sıralama"
          value={sort}
          onChange={(e) => {
            setSort(e.target.value);
            setDirty(true);
          }}
        >
          <option value="">Benzersiz anahtar sırası</option>
          {page?.columns.map((c) => (
            <option key={c.name}>{c.name}</option>
          ))}
        </select>
        <select
          value={direction}
          onChange={(e) => {
            setDirection(e.target.value);
            setDirty(true);
          }}
        >
          <option>ASC</option>
          <option>DESC</option>
        </select>
        <button disabled={busy} onClick={() => void load(true)}>
          Yenile / uygula
        </button>
        <button
          disabled={busy || !page?.editing?.enabled}
          title={page?.editing?.reason ?? "Yeni satır önizlemesi"}
          onClick={() => setInserting(true)}
        >
          Yeni satır…
        </button>
      </div>
      <div className="db-toolbar">
        <select
          value={column}
          onChange={(e) => {
            setColumn(e.target.value);
            setDirty(true);
          }}
        >
          <option value="">Filtre yok</option>
          {page?.columns.map((c) => (
            <option key={c.name}>{c.name}</option>
          ))}
        </select>
        <select
          value={operator}
          onChange={(e) => {
            setOperator(e.target.value);
            setDirty(true);
          }}
        >
          {[
            "=",
            "<>",
            ">",
            ">=",
            "<",
            "<=",
            "LIKE",
            "IS NULL",
            "IS NOT NULL",
          ].map((v) => (
            <option key={v}>{v}</option>
          ))}
        </select>
        <input
          placeholder="Filtre değeri"
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            setDirty(true);
          }}
        />
      </div>
      <nav className="db-section-tabs">
        {[
          ["rows", "Veri"],
          ["columns", "Sütunlar"],
          ["indexes", "İndeksler"],
          ["keys", "PK / FK / Kısıt"],
          ["diff", "Metadata farkı"],
        ].map(([key, label]) => (
          <button
            key={key}
            className={section === key ? "active" : ""}
            onClick={() => setSection(key)}
          >
            {label}
          </button>
        ))}
      </nav>
      {error && <pre className="db-query-error">{error}</pre>}
      {busy && <p>Yükleniyor…</p>}
      {section === "rows" ? (
        <>
          <div className="db-results">
            <table>
              <thead>
                <tr>
                  <th>İşlem</th>
                  {page?.columns.map((c) => (
                    <th key={c.name}>
                      {c.name}
                      <small>{c.type}</small>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {page?.rows.map((r, i) => (
                  <tr key={i}>
                    <td>
                      <button
                        disabled={busy || !page.editing?.enabled}
                        title={
                          page.editing?.reason ?? "Güncel satırı aç ve düzenle"
                        }
                        onClick={() =>
                          setEditingKeys(
                            Object.fromEntries(
                              (page.editing?.keyColumns ?? []).map((k) => [
                                k,
                                r[page.columns.findIndex((c) => c.name === k)],
                              ]),
                            ),
                          )
                        }
                      >
                        Düzenle
                      </button>
                      <button
                        disabled={busy || !page.editing?.enabled}
                        title={
                          page.editing?.reason ?? "Tek satır silme önizlemesi"
                        }
                        onClick={() =>
                          setDeletingKeys(
                            Object.fromEntries(
                              (page.editing?.keyColumns ?? []).map((k) => [
                                k,
                                r[page.columns.findIndex((c) => c.name === k)],
                              ]),
                            ),
                          )
                        }
                      >
                        Sil…
                      </button>
                    </td>
                    {r.map((c, j) => (
                      <td key={j}>
                        {c ?? "NULL"}
                        {page.largeValues?.columns.includes(
                          page.columns[j].name,
                        ) &&
                          (c?.endsWith("… [alan kısaltıldı]") ||
                            c?.startsWith("[binary") ||
                            /CLOB|TEXT|BLOB|BINARY|BYTEA|IMAGE/i.test(
                              page.columns[j].type,
                            )) && (
                            <button
                              className="db-full-value"
                              onClick={() =>
                                setLargeValue({
                                  column: page.columns[j].name,
                                  keys: Object.fromEntries(
                                    page.largeValues!.keyColumns.map((k) => [
                                      k,
                                      r[
                                        page.columns.findIndex(
                                          (col) => col.name === k,
                                        )
                                      ],
                                    ]),
                                  ),
                                })
                              }
                            >
                              Tam değer…
                            </button>
                          )}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {page?.largeValues?.reason && (
            <p className="db-muted">Tam değer: {page.largeValues.reason}</p>
          )}
          {page?.editing && !page.editing.enabled && (
            <p className="db-muted">Salt okunur: {page.editing.reason}</p>
          )}
          <p>
            {page?.rows.length ?? 0} satır ·{" "}
            {page?.stable
              ? "Benzersiz anahtar ile sayfalama"
              : "Benzersiz/sıralama anahtarı doğrulanamadı; sınırlı görünüm."}
          </p>
          <button
            disabled={busy || dirty || !previous.length}
            onClick={() => {
              setAfter(previous.at(-1) ?? null);
              setPrevious(previous.slice(0, -1));
            }}
          >
            Önceki
          </button>
          <button
            disabled={busy || dirty || !page?.next}
            onClick={() => {
              setPrevious([...previous, after]);
              setAfter(page!.next);
            }}
          >
            Sonraki
          </button>
        </>
      ) : section === "diff" ? (
        <>
          <button
            disabled={busy || !page}
            onClick={() =>
              page &&
              setSnapshots([
                ...snapshots.slice(-1),
                {
                  at: new Date().toLocaleString("tr"),
                  detail: structuredClone(page.detail),
                },
              ])
            }
          >
            Metadata görüntüsü al
          </button>
          <p>{snapshots.map((s) => s.at).join(" → ")}</p>
          <pre>
            {snapshots.length < 2
              ? "İki görüntü alın; ikinci görüntüden önce Yenile seçin."
              : changes.join("\n") || "Metadata değişikliği yok."}
          </pre>
        </>
      ) : (
        <div className="db-results">
          <table>
            <thead>
              <tr>
                {detailKeys.map((k) => (
                  <th key={k}>{titles[k] ?? k}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {currentDetail?.map((row, i) => (
                <tr key={i}>
                  {detailKeys.map((k) => (
                    <td key={k}>{row[k] ?? "—"}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
