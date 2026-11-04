import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import { rowChanges, type RowValues } from "./rowEditing";
type Draft = {
  draftId: string;
  schema: string;
  table: string;
  original: RowValues;
  columns: {
    name: string;
    type: string;
    nullable: boolean;
    readOnly: boolean;
  }[];
};
type Plan = {
  planId: string;
  sql: string;
  changes: { column: string; before: string | null; after: string | null }[];
  keys: RowValues;
};
type Outcome = {
  state: "succeeded" | "conflict" | "failed" | "unknown_outcome";
  message?: string;
  current?: RowValues;
  rolledBack?: boolean;
};
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
export default function RowEditor({
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
  const [draft, setDraft] = useState<Draft | null>(null),
    [values, setValues] = useState<RowValues>({}),
    [plan, setPlan] = useState<Plan | null>(null);
  const [busy, setBusy] = useState(true),
    [error, setError] = useState(""),
    [outcome, setOutcome] = useState<Outcome | null>(null);
  const alive = useRef(true),
    token = useRef(""),
    pending = useRef(false);
  useEffect(() => {
    let current = true;
    alive.current = true;
    queueMicrotask(() => {
      if (!current) return;
      void rpc<Draft>({ op: "rowEditOpen", sessionId, schema, table, keys })
        .then((d) => {
          if (!current) {
            void rpc({
              op: "rowEditDiscard",
              sessionId,
              draftId: d.draftId,
            }).catch(() => {});
            return;
          }
          token.current = d.draftId;
          setDraft(d);
          setValues(d.original);
        })
        .catch((e) => {
          if (current) setError(errorText(e));
        })
        .finally(() => {
          if (current) setBusy(false);
        });
    });
    return () => {
      current = false;
      alive.current = false;
      if (token.current)
        void rpc({
          op: "rowEditDiscard",
          sessionId,
          draftId: token.current,
        }).catch(() => {});
    };
  }, [sessionId, schema, table, keys]);
  async function preview() {
    if (!draft || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const p = await rpc<Plan>({
        op: "rowEditPreview",
        sessionId,
        draftId: draft.draftId,
        changes: rowChanges(draft.original, values),
      });
      if (alive.current) setPlan(p);
    } catch (e) {
      if (alive.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (alive.current) setBusy(false);
    }
  }
  async function apply() {
    if (!draft || !plan || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const result = await rpc<Outcome>({
        op: "rowEditApply",
        sessionId,
        draftId: draft.draftId,
        planId: plan.planId,
      });
      if (alive.current) {
        setOutcome(result);
      }
    } catch (e) {
      if (alive.current)
        setOutcome({
          state: "unknown_outcome",
          message: `Sonuç doğrulanamadı; otomatik tekrar yapılmadı. ${errorText(e)}`,
        });
    } finally {
      pending.current = false;
      if (alive.current) setBusy(false);
    }
  }
  const changed = draft
    ? Object.keys(rowChanges(draft.original, values)).length
    : 0;
  return (
    <section className="db-row-editor" aria-label="Satır düzenleme">
      <header>
        <div>
          <h3>
            Satır düzenle · {schema}.{table}
          </h3>
          <p>
            Satır ayrı bağlantıdan güncel haliyle okunur. Taslak 5 dakika
            geçerli.
          </p>
        </div>
        <button disabled={busy} onClick={onClose}>
          Kapat
        </button>
      </header>
      {error && (
        <pre className="db-query-error" role="alert">
          {error}
        </pre>
      )}
      {busy && <p role="status">İşlem sürüyor…</p>}
      {outcome ? (
        <div className={`db-edit-outcome ${outcome.state}`} role="status">
          <h4>
            {outcome.state === "succeeded"
              ? "1 satır kaydedildi"
              : outcome.state === "conflict"
                ? "Çakışma · üzerine yazılmadı"
                : outcome.state === "unknown_outcome"
                  ? "Kaydetme sonucu belirsiz"
                  : "Kaydedilemedi"}
          </h4>
          <p>{outcome.message}</p>
          {outcome.rolledBack && <p>Transaction geri alındı.</p>}
          {outcome.current && (
            <div className="db-results">
              <table>
                <thead>
                  <tr>
                    <th>Alan</th>
                    <th>İlk okunan</th>
                    <th>Sunucudaki</th>
                  </tr>
                </thead>
                <tbody>
                  {Object.keys(outcome.current)
                    .filter((k) => outcome.current![k] !== draft?.original[k])
                    .map((k) => (
                      <tr key={k}>
                        <td>{k}</td>
                        <td>{draft?.original[k] ?? "NULL"}</td>
                        <td>{outcome.current![k] ?? "NULL"}</td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
          )}
          {outcome.state !== "succeeded" && (
            <p>
              Satırı yeniden açıp güncel değerler üzerinden yeni önizleme
              hazırlayın.
            </p>
          )}
        </div>
      ) : plan ? (
        <>
          <h4>Değişiklik önizlemesi · {plan.changes.length} alan</h4>
          <div className="db-results">
            <table>
              <thead>
                <tr>
                  <th>Alan</th>
                  <th>Önce</th>
                  <th>Sonra</th>
                </tr>
              </thead>
              <tbody>
                {plan.changes.map((c) => (
                  <tr key={c.column}>
                    <td>{c.column}</td>
                    <td>{c.before ?? "NULL"}</td>
                    <td>{c.after ?? "NULL"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <details>
            <summary>SQL ve satır anahtarı</summary>
            <pre>{plan.sql}</pre>
            <pre>{JSON.stringify(plan.keys, null, 2)}</pre>
            <p>
              Değerler bind edilir; SQL metnine eklenmez. Kaydetmeden önce
              satırın tamamı tekrar karşılaştırılır.
            </p>
          </details>
          <p>
            Onayla ve kaydet, tek transaction içinde commit eder. SQL editörünün
            açık transaction'ı ayrı kalır.
          </p>
          <button disabled={busy} onClick={() => setPlan(null)}>
            Düzenlemeye dön
          </button>
          <button
            className="primary"
            disabled={busy}
            onClick={() => void apply()}
          >
            Onayla ve kaydet
          </button>
        </>
      ) : (
        draft && (
          <>
            <div className="db-edit-fields">
              {draft.columns.map((c) => (
                <div className="db-edit-field" key={c.name}>
                  <span>
                    {c.name}{" "}
                    <small>
                      {c.type}
                      {c.readOnly ? " · Anahtar / üretilen alan" : ""}
                    </small>
                  </span>
                  <div>
                    <input
                      aria-label={`${c.name} değeri`}
                      disabled={busy || c.readOnly || values[c.name] === null}
                      value={values[c.name] ?? ""}
                      onChange={(e) =>
                        setValues({ ...values, [c.name]: e.target.value })
                      }
                    />
                    <label className="db-null-toggle">
                      <input
                        type="checkbox"
                        aria-label={`${c.name} NULL`}
                        disabled={busy || c.readOnly || !c.nullable}
                        checked={values[c.name] === null}
                        onChange={(e) =>
                          setValues({
                            ...values,
                            [c.name]: e.target.checked ? null : "",
                          })
                        }
                      />
                      NULL
                    </label>
                  </div>
                </div>
              ))}
            </div>
            <button
              className="primary"
              disabled={busy || changed === 0}
              onClick={() => void preview()}
            >
              Değişiklikleri önizle ({changed})
            </button>
            <button disabled={busy} onClick={() => setValues(draft.original)}>
              Değişiklikleri geri al
            </button>
          </>
        )
      )}
    </section>
  );
}
