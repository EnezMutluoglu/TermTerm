import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import type { RowValues } from "./rowEditing";
type Column = {
  name: string;
  type: string;
  generated: boolean;
  nullable: boolean;
  defaultValue: string | null;
  primaryKey: boolean;
};
type Field = { mode: "default" | "value" | "null"; text: string };
type Plan = {
  planId: string;
  hash: string;
  sql: string;
  values: RowValues;
  connection: { host: string; database: string; username: string };
};
type Outcome = { state: string; message: string; keys?: RowValues };
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
export default function RowInsert({
  sessionId,
  schema,
  table,
  onClose,
}: {
  sessionId: string;
  schema: string;
  table: string;
  onClose: () => void;
}) {
  const [columns, setColumns] = useState<Column[]>([]),
    [fields, setFields] = useState<Record<string, Field>>({}),
    [busy, setBusy] = useState(true),
    [error, setError] = useState(""),
    [plan, setPlan] = useState<Plan | null>(null),
    [confirmed, setConfirmed] = useState(false),
    [outcome, setOutcome] = useState<Outcome | null>(null);
  const active = useRef(false),
    pending = useRef(false),
    draft = useRef<Plan | null>(null);
  useEffect(() => {
    let live = true;
    active.current = true;
    void rpc<{ columns: Column[] }>({
      op: "rowInsertMetadata",
      sessionId,
      schema,
      table,
    })
      .then((r) => {
        if (!live) return;
        setColumns(r.columns);
        setFields(
          Object.fromEntries(
            r.columns.map((c) => [
              c.name,
              {
                mode:
                  c.generated || c.defaultValue != null || c.nullable
                    ? "default"
                    : "value",
                text: "",
              },
            ]),
          ),
        );
      })
      .catch((e) => {
        if (live) setError(errorText(e));
      })
      .finally(() => {
        if (live) setBusy(false);
      });
    return () => {
      live = false;
      active.current = false;
      if (draft.current)
        void rpc({
          op: "rowInsertDiscard",
          sessionId,
          planId: draft.current.planId,
        }).catch(() => {});
    };
  }, [sessionId, schema, table]);
  function change(name: string, field: Field) {
    setFields((v) => ({ ...v, [name]: field }));
    setPlan(null);
    setConfirmed(false);
    if (draft.current) {
      void rpc({
        op: "rowInsertDiscard",
        sessionId,
        planId: draft.current.planId,
      }).catch(() => {});
      draft.current = null;
    }
  }
  async function preview() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setPlan(null);
    setConfirmed(false);
    try {
      const values = Object.fromEntries(
        Object.entries(fields)
          .filter(([, f]) => f.mode !== "default")
          .map(([name, f]) => [name, f.mode === "null" ? null : f.text]),
      );
      const p = await rpc<Plan>({
        op: "rowInsertPreview",
        sessionId,
        schema,
        table,
        values,
      });
      if (active.current) {
        draft.current = p;
        setPlan(p);
      } else
        void rpc({ op: "rowInsertDiscard", sessionId, planId: p.planId }).catch(
          () => {},
        );
    } catch (e) {
      if (active.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (active.current) setBusy(false);
    }
  }
  async function apply() {
    if (!plan || !confirmed || pending.current || outcome) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const r = await rpc<Outcome>({
        op: "rowInsertApply",
        sessionId,
        planId: plan.planId,
        hash: plan.hash,
      });
      if (active.current) setOutcome(r);
    } catch {
      if (active.current)
        setOutcome({
          state: "unknown_outcome",
          message:
            "Sonuç alınamadı; tekrar eklemeden önce yeni bağlantıdan kontrol edin.",
        });
    } finally {
      pending.current = false;
      draft.current = null;
      if (active.current) setBusy(false);
    }
  }
  return (
    <section className="db-row-editor db-row-insert">
      <div className="db-toolbar">
        <h3>
          Yeni satır — {schema}.{table}
        </h3>
        <button disabled={busy} onClick={onClose}>
          Tabloya dön
        </button>
      </div>
      <p>
        Önizleme veri yazmaz. Onay ayrı bir transaction ile tek satır ekler;
        editör transaction’ını commit etmez. Sunucu varsayılanları ekleme anında
        hesaplanır; sequence/identity numaralarında rollback sonrası boşluk
        olabilir.
      </p>
      {error && (
        <pre className="db-query-error" role="alert">
          {error}
        </pre>
      )}
      {busy && <p role="status">İşlem sürüyor…</p>}
      <div className="db-results">
        <table>
          <thead>
            <tr>
              <th>Alan</th>
              <th>Tür</th>
              <th>Seçim</th>
              <th>Değer / varsayılan</th>
            </tr>
          </thead>
          <tbody>
            {columns.map((c) => (
              <tr key={c.name}>
                <th>
                  {c.name}
                  {c.primaryKey ? " (PK)" : ""}
                </th>
                <td>{c.type}</td>
                <td>
                  <select
                    aria-label={`${c.name} seçimi`}
                    value={fields[c.name]?.mode ?? "default"}
                    disabled={busy || c.generated || !!outcome}
                    onChange={(e) =>
                      change(c.name, {
                        ...fields[c.name],
                        mode: e.target.value as Field["mode"],
                      })
                    }
                  >
                    <option
                      value="default"
                      disabled={
                        !c.generated && !c.nullable && c.defaultValue == null
                      }
                    >
                      {c.generated
                        ? "Sunucu üretir"
                        : "Varsayılan / belirtilmedi"}
                    </option>
                    {!c.generated && <option value="value">Değer gir</option>}
                    {c.nullable && !c.generated && (
                      <option value="null">NULL</option>
                    )}
                  </select>
                </td>
                <td>
                  {fields[c.name]?.mode === "value" ? (
                    <input
                      aria-label={`${c.name} yeni değer`}
                      value={fields[c.name].text}
                      disabled={busy || !!outcome}
                      onChange={(e) =>
                        change(c.name, {
                          ...fields[c.name],
                          text: e.target.value,
                        })
                      }
                    />
                  ) : fields[c.name]?.mode === "null" ? (
                    "NULL"
                  ) : (
                    (c.defaultValue ?? (c.generated ? "Otomatik" : "NULL"))
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {!outcome && (
        <button
          disabled={busy || !columns.length}
          onClick={() => void preview()}
        >
          Eklenecek satırı önizle
        </button>
      )}
      {plan && (
        <>
          <p>
            {plan.connection.host} · {plan.connection.database} ·{" "}
            {plan.connection.username}
          </p>
          <pre>{plan.sql}</pre>
          <p>
            Bağlanacak değerler: {JSON.stringify(plan.values)}. Diğer alanlar
            sunucu varsayılanına bırakılır. Plan 5 dakika geçerlidir; uygulamada
            kısa tablo kilidi alınır.
          </p>
          {!outcome && (
            <>
              <label className="db-maintenance-confirm">
                <input
                  type="checkbox"
                  checked={confirmed}
                  disabled={busy}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />
                Bu tabloya 1 satır eklenmesini onaylıyorum.
              </label>
              <button
                disabled={busy || !confirmed}
                onClick={() => void apply()}
              >
                Onayla ve 1 satır ekle
              </button>
            </>
          )}
        </>
      )}
      {outcome && (
        <div
          role="status"
          className="db-edit-outcome"
          data-state={outcome.state}
        >
          {outcome.message}
          {outcome.keys && (
            <pre>Yeni anahtar: {JSON.stringify(outcome.keys)}</pre>
          )}
        </div>
      )}
    </section>
  );
}
