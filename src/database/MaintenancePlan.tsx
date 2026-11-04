import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import { Modal } from "../components";
import { MetadataSelect, useMetadataSession } from "./MetadataPicker";
export const maintenanceActions = new Set([
  "PG-OPS-004",
  "MS-OPS-003",
  "OR-OPS-003",
]);
type Plan = {
  planId: string;
  hash: string;
  sql: string;
  target: { schema: string; table: string; statistic: string };
  connection: {
    host: string;
    port: number;
    database: string;
    username: string;
  };
  before: Record<string, string | null>;
  effect: string;
  expiresInSeconds: number;
};
type Outcome = {
  state: string;
  message: string;
  after?: Record<string, string | null>;
  warnings?: string[];
};
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
export default function MaintenancePlan({
  actionId,
  title,
  recordId,
  sessionId,
  initialSchema,
  onClose,
}: {
  actionId: string;
  title: string;
  recordId: string;
  sessionId?: string;
  initialSchema: string;
  onClose: () => void;
}) {
  const { session, error: sessionError } = useMetadataSession(
    recordId,
    sessionId,
  );
  const [schema, setSchema] = useState(initialSchema),
    [table, setTable] = useState(""),
    [statistic, setStatistic] = useState(""),
    [statistics, setStatistics] = useState<string[]>([]);
  const [plan, setPlan] = useState<Plan>(),
    [outcome, setOutcome] = useState<Outcome>(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [confirmed, setConfirmed] = useState(false);
  const generation = useRef(0),
    pending = useRef(false),
    currentPlan = useRef<Plan | undefined>(undefined);
  useEffect(() => {
    let live = true;
    setStatistics([]);
    setStatistic("");
    if (session && schema && table && actionId === "MS-OPS-003")
      void rpc<{ statistics: string[] }>({
        op: "changeOptions",
        sessionId: session,
        schema,
        table,
      })
        .then((r) => {
          if (live) setStatistics(r.statistics);
        })
        .catch((e) => {
          if (live) setError(errorText(e));
        });
    return () => {
      live = false;
    };
  }, [session, schema, table, actionId]);
  useEffect(() => {
    generation.current++;
    setPlan(undefined);
    setOutcome(undefined);
    setError("");
    setConfirmed(false);
    const old = currentPlan.current;
    currentPlan.current = undefined;
    if (old && session)
      void rpc({
        op: "changeDiscard",
        sessionId: session,
        planId: old.planId,
      }).catch(() => {});
  }, [session, schema, table, statistic]);
  useEffect(
    () => () => {
      generation.current++;
      const p = currentPlan.current;
      if (p && session)
        void rpc({
          op: "changeDiscard",
          sessionId: session,
          planId: p.planId,
        }).catch(() => {});
    },
    [session],
  );
  async function execute(apply: boolean) {
    if (pending.current || !session || (apply && (!plan || !confirmed))) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setOutcome(undefined);
    const epoch = ++generation.current;
    const approved = plan;
    setPlan(undefined);
    currentPlan.current = undefined;
    setConfirmed(false);
    try {
      if (apply) {
        const r = await rpc<Outcome>({
          op: "changeApply",
          sessionId: session,
          planId: approved!.planId,
          hash: approved!.hash,
        });
        if (epoch === generation.current) setOutcome(r);
      } else {
        const p = await rpc<Plan>({
          op: "changePreview",
          sessionId: session,
          actionId,
          schema,
          table,
          statistic,
        });
        if (epoch === generation.current) {
          setPlan(p);
          currentPlan.current = p;
        }
      }
    } catch (e) {
      if (epoch === generation.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (epoch === generation.current) setBusy(false);
    }
  }
  return (
    <Modal
      wide
      title={`${actionId} · ${title}`}
      onClose={() => {
        if (!pending.current) onClose();
      }}
    >
      <section className="db-maintenance">
        <p>
          Önizleme değişiklik yapmaz. Uygulama ayrı bağlantıda çalışır; açık SQL
          transaction'ınız korunur. Plan beş dakika geçerli ve tek
          kullanımlıktır.
        </p>
        <fieldset disabled={busy}>
          <div>
            <span>Şema</span>
            <MetadataSelect
              sessionId={session}
              kind="schemas"
              value={schema}
              label="Bakım şeması"
              onChange={(v) => {
                setSchema(v);
                setTable("");
              }}
            />
          </div>
          <div>
            <span>Tablo</span>
            <MetadataSelect
              sessionId={session}
              kind="tables"
              schema={schema}
              value={table}
              label="Bakım tablosu"
              onChange={setTable}
            />
          </div>
          {actionId === "MS-OPS-003" && (
            <label>
              İstatistik
              <select
                aria-label="Bakım istatistiği"
                value={statistic}
                onChange={(e) => setStatistic(e.target.value)}
              >
                <option value="">İstatistik seçin</option>
                {statistics.map((n) => (
                  <option key={n}>{n}</option>
                ))}
              </select>
            </label>
          )}
        </fieldset>
        <button
          disabled={
            busy ||
            !table ||
            !session ||
            (actionId === "MS-OPS-003" && !statistic)
          }
          onClick={() => void execute(false)}
        >
          Değişiklik planını hazırla
        </button>
        {(error || sessionError) && <p role="alert">{error || sessionError}</p>}
        {busy && (
          <p role="status">
            Sunucu yanıtı bekleniyor… Bu pencereyi işlem bitince
            kapatabilirsiniz.
          </p>
        )}
        {plan && (
          <div className="db-change-preview">
            <p>
              Sunucu: {plan.connection.host}:{plan.connection.port} · DB/PDB:{" "}
              {plan.connection.database} · Hesap: {plan.connection.username}
            </p>
            <h3>
              {plan.target.schema}.{plan.target.table} {plan.target.statistic}
            </h3>
            <p>{plan.effect}</p>
            <h4>Mevcut istatistik bilgisi</h4>
            <pre>{JSON.stringify(plan.before, null, 2)}</pre>
            <h4>Uygulanacak SQL</h4>
            <pre>{plan.sql}</pre>
            {actionId === "OR-OPS-003" && (
              <p>
                Bind değerleri:{" "}
                {JSON.stringify([plan.target.schema, plan.target.table])}
              </p>
            )}
            <small>Plan SHA-256: {plan.hash}</small>
            <label className="db-maintenance-confirm">
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
              />{" "}
              Hedefi ve SQL'i kontrol ettim; istatistik/plan değişikliğini
              uygula.
            </label>
            <button
              className="primary"
              disabled={!confirmed || busy}
              onClick={() => void execute(true)}
            >
              Onaylanan planı uygula
            </button>
          </div>
        )}
        {outcome && (
          <div className="db-change-outcome" role="status">
            <strong>
              {{
                succeeded: "Tamamlandı",
                failed: "Uygulanmadı",
                conflict: "Hedef değişti",
                partial: "Uyarılı sonuç",
                unknown_outcome: "Sonuç belirsiz",
              }[outcome.state] ?? outcome.state}
            </strong>
            <p>{outcome.message}</p>
            {outcome.after && (
              <pre>{JSON.stringify(outcome.after, null, 2)}</pre>
            )}
            {outcome.warnings?.map((w, i) => (
              <p key={i}>{w}</p>
            ))}
          </div>
        )}
      </section>
    </Modal>
  );
}
