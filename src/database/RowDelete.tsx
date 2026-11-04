import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import type { RowValues } from "./rowEditing";

type Plan = {
  planId: string;
  hash: string;
  sql: string;
  original: RowValues;
  connection: { host: string; database: string; username: string };
};
type Outcome = { state: string; message: string; affectedRows?: number };
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });

export default function RowDelete({
  sessionId,
  schema,
  table,
  keys,
  onClose,
}: {
  sessionId: string;
  schema: string;
  table: string;
  keys: RowValues;
  onClose: () => void;
}) {
  const [plan, setPlan] = useState<Plan | null>(null);
  const [busy, setBusy] = useState(true);
  const [confirmed, setConfirmed] = useState(false);
  const [error, setError] = useState("");
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const active = useRef(false),
    pending = useRef(false),
    draft = useRef<Plan | null>(null);
  useEffect(() => {
    let live = true;
    active.current = true;
    void rpc<Plan>({ op: "rowDeletePreview", sessionId, schema, table, keys })
      .then((p) => {
        if (!live) {
          void rpc({
            op: "rowDeleteDiscard",
            sessionId,
            planId: p.planId,
          }).catch(() => {});
          return;
        }
        draft.current = p;
        setPlan(p);
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
          op: "rowDeleteDiscard",
          sessionId,
          planId: draft.current.planId,
        }).catch(() => {});
    };
  }, [sessionId, schema, table, keys]);
  async function apply() {
    if (!plan || !confirmed || pending.current || outcome) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const result = await rpc<Outcome>({
        op: "rowDeleteApply",
        sessionId,
        planId: plan.planId,
        hash: plan.hash,
      });
      if (active.current) setOutcome(result);
    } catch {
      if (active.current)
        setOutcome({
          state: "unknown_outcome",
          message:
            "Sonuç alınamadı. Yeniden silmeden önce yeni bağlantıdan satırı kontrol edin.",
        });
    } finally {
      pending.current = false;
      draft.current = null;
      if (active.current) setBusy(false);
    }
  }
  return (
    <section className="db-row-editor db-row-delete">
      <div className="db-toolbar">
        <h3>
          Satır silme önizlemesi — {schema}.{table}
        </h3>
        <button disabled={busy} onClick={onClose}>
          Tabloya dön
        </button>
      </div>
      <p>
        Yalnız bu satır, ayrı bir transaction ile silinip commit edilir. Kontrol
        sırasında kısa süreli tablo kilidi alınır. Editörün açık transaction’ı
        değişmez.
      </p>
      {busy && (
        <p role="status">
          {plan
            ? "Satır yeniden kontrol ediliyor ve siliniyor…"
            : "Yetkiler, ilişkiler ve güncel satır kontrol ediliyor…"}
        </p>
      )}
      {error && (
        <pre className="db-query-error" role="alert">
          {error}
        </pre>
      )}
      {plan && (
        <>
          <p>
            {plan.connection.host} · {plan.connection.database} ·{" "}
            {plan.connection.username}
          </p>
          <div className="db-results">
            <table>
              <thead>
                <tr>
                  <th>Alan</th>
                  <th>Silinecek değer</th>
                </tr>
              </thead>
              <tbody>
                {Object.entries(plan.original).map(([name, value]) => (
                  <tr key={name}>
                    <th>
                      {name}
                      {Object.hasOwn(keys, name) ? " (PK)" : ""}
                    </th>
                    <td>{value ?? "NULL"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <details>
            <summary>Uygulanacak SQL</summary>
            <pre>{plan.sql}</pre>
            <p>
              Birincil anahtar değerleri parametre olarak bağlanır. Plan 5
              dakika geçerlidir.
            </p>
          </details>
          {!outcome && (
            <>
              <label className="db-maintenance-confirm">
                <input
                  type="checkbox"
                  checked={confirmed}
                  disabled={busy}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />
                Önizlemedeki 1 satırın kalıcı olarak silinmesini onaylıyorum.
              </label>
              <button
                className="danger"
                disabled={busy || !confirmed}
                onClick={() => void apply()}
              >
                Onayla ve 1 satırı sil
              </button>
            </>
          )}
        </>
      )}
      {outcome && (
        <p className="db-edit-outcome" role="status" data-state={outcome.state}>
          {outcome.message}
        </p>
      )}
    </section>
  );
}
