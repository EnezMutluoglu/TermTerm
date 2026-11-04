import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import {
  compareSchemaSnapshots,
  type SchemaChange,
  type SchemaSnapshot,
  type SnapshotSummary,
} from "./schemaSnapshots";
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
const stamp = (n: number) =>
  new Date(n).toLocaleString("tr-TR", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    fractionalSecondDigits: 3,
  });
export default function SchemaHistory({
  recordId,
  sessionId,
  schema,
}: {
  recordId: string;
  sessionId?: string;
  schema: string;
}) {
  const [items, setItems] = useState<SnapshotSummary[]>([]),
    [left, setLeft] = useState(""),
    [right, setRight] = useState("");
  const [changes, setChanges] = useState<SchemaChange[] | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState(""),
    [remove, setRemove] = useState(""),
    [compared, setCompared] = useState("");
  const epoch = useRef(0),
    pending = useRef(false);
  useEffect(() => {
    const token = ++epoch.current;
    setLoading(true);
    setItems([]);
    setChanges(null);
    setLeft("");
    setRight("");
    setError("");
    void rpc<{ items: SnapshotSummary[] }>({ op: "snapshotList", recordId })
      .then((r) => {
        if (token === epoch.current) setItems(r.items);
      })
      .catch((e) => {
        if (token === epoch.current) setError(errorText(e));
      })
      .finally(() => {
        if (token === epoch.current) setLoading(false);
      });
    return () => {
      epoch.current++;
    };
  }, [recordId]);
  async function run(task: (token: number) => Promise<void>) {
    if (pending.current || loading) return;
    pending.current = true;
    setBusy(true);
    setError("");
    const token = epoch.current;
    try {
      await task(token);
    } catch (e) {
      if (token === epoch.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (token === epoch.current) setBusy(false);
    }
  }
  const selectedLeft = items.find((i) => i.id === left),
    selectedRight = items.find((i) => i.id === right);
  const compatible =
    !!selectedLeft &&
    !!selectedRight &&
    left !== right &&
    selectedLeft.source === selectedRight.source;
  const visibleChanges =
    changes?.filter((c) =>
      `${c.object} ${c.name} ${c.section}`
        .toLocaleLowerCase("tr")
        .includes(filter.toLocaleLowerCase("tr")),
    ) ?? [];
  return (
    <div className="db-schema-history">
      <div className="db-catalog-heading">
        <div>
          <h2>Şema geçmişi</h2>
          <p>İki tarihte görünür olan yapıyı karşılaştırın.</p>
        </div>
        <button
          className="primary"
          disabled={busy || loading || !sessionId || !schema}
          onClick={() =>
            void run(async (token) => {
              const result = await rpc<SchemaSnapshot>({
                op: "snapshotCapture",
                sessionId,
                schema,
              });
              if (token !== epoch.current) return;
              const { objects, ...summary } = result;
              setItems((old) => [
                { ...summary, objectCount: objects.length },
                ...old,
              ]);
              setRight(result.id);
              setChanges(null);
            })
          }
        >
          {busy ? "İşlem sürüyor…" : "Şema görüntüsü al"}
        </button>
      </div>
      <p className="db-muted">
        {sessionId && schema
          ? `Seçili şema: ${schema}`
          : "Yeni görüntü için bağlanıp nesne gezgininden bir şema seçin."}{" "}
        · En fazla 200 tablo/görünüm, 30 saniye.
      </p>
      <p className="db-snapshot-notice">
        Yalnız bu DB kullanıcısının görebildiği metadata okunur. Artık
        görünmeyen bir nesne silinmiş veya erişimi değişmiş olabilir. Yakalama
        sırasında yapılan DDL için tutarlı bir anlık görüntü garantisi yoktur.
        Tablo verisi ve otomatik DDL içermez.
      </p>
      {error && (
        <pre className="db-query-error" role="alert">
          {error}
        </pre>
      )}
      <div className="db-toolbar">
        {(
          [
            ["Önce", left, setLeft],
            ["Sonra", right, setRight],
          ] as const
        ).map(([label, value, setter]) => (
          <label key={label}>
            {label}
            <select
              aria-label={`${label} görüntüsü`}
              value={value}
              disabled={busy || loading}
              onChange={(e) => {
                setter(e.target.value);
                setChanges(null);
              }}
            >
              <option value="">Görüntü seçin</option>
              {items.map((i) => (
                <option key={i.id} value={i.id}>
                  {stamp(i.capturedAt)} · {i.schema} · {i.objectCount} nesne ·{" "}
                  {i.username}
                </option>
              ))}
            </select>
          </label>
        ))}
        <button
          disabled={busy || loading || !compatible}
          onClick={() =>
            void run(async (token) => {
              const [a, b] = await Promise.all([
                rpc<SchemaSnapshot>({ op: "snapshotGet", recordId, id: left }),
                rpc<SchemaSnapshot>({ op: "snapshotGet", recordId, id: right }),
              ]);
              const result = compareSchemaSnapshots(a, b);
              if (token === epoch.current) {
                setChanges(result);
                setCompared(
                  `${stamp(a.capturedAt)} → ${stamp(b.capturedAt)} · ${a.schema}${a.version !== b.version ? " · Sunucu sürümü değişmiş" : ""}`,
                );
              }
            })
          }
        >
          Karşılaştır
        </button>
      </div>
      {left && right && !compatible && (
        <p className="db-muted">
          Aynı hedef/kullanıcı/şemadan iki farklı görüntü seçin.
        </p>
      )}
      {changes !== null && (
        <>
          <p>
            {compared} · {changes.length} fark
          </p>
          <input
            aria-label="Şema farklarını filtrele"
            placeholder="Nesne veya sütun ara…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <div className="db-results db-schema-diff">
            <table>
              <thead>
                <tr>
                  <th>Nesne</th>
                  <th>Alan</th>
                  <th>Durum</th>
                  <th>Önce</th>
                  <th>Sonra</th>
                </tr>
              </thead>
              <tbody>
                {visibleChanges.slice(0, 500).map((c, i) => (
                  <tr key={i}>
                    <td>{c.object}</td>
                    <td>
                      {c.section}
                      <small>{c.name}</small>
                    </td>
                    <td>
                      {c.kind === "appeared"
                        ? "Yeni görünür"
                        : c.kind === "not_visible"
                          ? "Artık görünmüyor"
                          : "Değişti"}
                    </td>
                    <td>
                      {c.beforeLabel ? (
                        <details>
                          <summary>{c.beforeLabel}</summary>
                          <p>{c.before}</p>
                        </details>
                      ) : (
                        c.before || "—"
                      )}
                    </td>
                    <td>
                      {c.afterLabel ? (
                        <details>
                          <summary>{c.afterLabel}</summary>
                          <p>{c.after}</p>
                        </details>
                      ) : (
                        c.after || "—"
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!changes.length && <p>Görünür metadata değişikliği yok.</p>}
            {visibleChanges.length > 500 && (
              <p>İlk 500 fark gösteriliyor; listeyi aramayla daraltın.</p>
            )}
          </div>
        </>
      )}
      <details className="db-snapshot-library">
        <summary>Kaydedilen görüntüler ({items.length})</summary>
        {items.map((i) => (
          <div key={i.id}>
            <span>
              {stamp(i.capturedAt)} · {i.schema} · {i.objectCount} nesne
            </span>
            {remove === i.id ? (
              <>
                <button
                  disabled={busy || loading}
                  onClick={() =>
                    void run(async (token) => {
                      await rpc({ op: "snapshotDelete", recordId, id: i.id });
                      if (token === epoch.current) {
                        setItems(items.filter((v) => v.id !== i.id));
                        setRemove("");
                        if (left === i.id) setLeft("");
                        if (right === i.id) setRight("");
                        setChanges(null);
                      }
                    })
                  }
                >
                  Silmeyi onayla
                </button>
                <button onClick={() => setRemove("")}>Vazgeç</button>
              </>
            ) : (
              <button
                disabled={busy || loading}
                onClick={() => setRemove(i.id)}
              >
                Sil
              </button>
            )}
          </div>
        ))}
      </details>
      <p className="db-muted">
        Yerel, şifreli saklama · Kasa başına 40 görüntü / 48 MiB · Otomatik
        silme yok · Kasa yedeğine/senkronizasyona dahil değil.
      </p>
    </div>
  );
}
