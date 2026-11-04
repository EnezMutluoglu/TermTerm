import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import type { DatabaseConnection, QueryResult } from "./types";
import {
  blockingTree,
  capacity,
  compareWindows,
  growth,
  rows,
  type Sample,
} from "./diagnostics";
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
const storageActions: Record<string, [string, string][]> = {
  postgresql: [
    ["PG-MON-012", "Replication slotları"],
    ["PG-MON-013", "WAL üretimi"],
    ["PG-MON-015", "WAL alanı"],
  ],
  mssql: [
    ["MS-MON-009", "Transaction log"],
    ["MS-MON-013", "Tempdb"],
  ],
  oracle: [
    ["OR-MON-016", "Tablespace"],
    ["OR-MON-021", "FRA"],
  ],
};
const metricText = (name: string, value: number | undefined) =>
  value === undefined
    ? "Bilinmiyor"
    : /bytes|space_limit|space_used|space_reclaimable/.test(name)
      ? `${(value / 1048576).toLocaleString("tr", { maximumFractionDigits: 2 })} MiB`
      : /percent|pct/.test(name)
        ? `${value.toLocaleString("tr", { maximumFractionDigits: 2 })} %`
        : value.toLocaleString("tr", { maximumFractionDigits: 2 });
const actions = {
  postgresql: {
    blocking: "PG-MON-007",
    performance: "PG-MON-026",
    storage: "PG-MON-012",
  },
  mssql: {
    blocking: "MS-MON-005",
    performance: "MS-MON-020",
    storage: "MS-MON-009",
  },
  oracle: {
    blocking: "OR-MON-005",
    performance: "OR-MON-013",
    storage: "OR-MON-021",
  },
};
export default function DatabaseDiagnostics({
  recordId,
  profile,
  visible,
  onPolicy,
}: {
  recordId: string;
  profile: DatabaseConnection;
  visible: boolean;
  onPolicy: (warning: number, critical: number) => Promise<void>;
}) {
  const [panel, setPanel] = useState<"blocking" | "performance" | "storage">(
      "blocking",
    ),
    [storageAction, setStorageAction] = useState(""),
    [result, setResult] = useState<QueryResult>(),
    [history, setHistory] = useState<Sample[]>([]),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [refresh, setRefresh] = useState(false),
    [warning, setWarning] = useState(profile.healthWarning ?? 80),
    [critical, setCritical] = useState(profile.healthCritical ?? 90),
    [windowResults, setWindowResults] = useState<QueryResult[]>([]),
    [now, setNow] = useState(Date.now());
  const pending = useRef(false),
    epoch = useRef(0);
  useEffect(() => {
    if (!visible) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [visible]);
  const actionId =
    panel === "storage"
      ? storageActions[profile.engine].some(([id]) => id === storageAction)
        ? storageAction
        : storageActions[profile.engine][0][0]
      : actions[profile.engine][panel];
  async function run() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    const token = ++epoch.current;
    let parameters: Record<string, unknown> = {};
    if (panel === "performance") {
      if (profile.engine === "postgresql")
        parameters = {
          pg_stat_statements_view: {
            schema: "__auto__",
            table: "pg_stat_statements",
          },
        };
      if (profile.engine === "mssql")
        parameters = {
          from_time: new Date(Date.now() - 3600000).toISOString(),
          to_time: new Date().toISOString(),
        };
      if (profile.engine === "oracle")
        parameters = { min_executions: 1, row_limit: 100 };
    }
    try {
      let next: QueryResult;
      if (panel === "performance" && profile.engine === "mssql") {
        const end = Math.floor(Date.now() / 3600000) * 3600000;
        const pair: QueryResult[] = [];
        for (const offset of [2, 1])
          pair.push(
            await rpc<QueryResult>({
              op: "check",
              recordId,
              actionId,
              parameters: {
                from_time: new Date(end - offset * 3600000).toISOString(),
                to_time: new Date(end - (offset - 1) * 3600000).toISOString(),
              },
            }),
          );
        next = pair[1];
        if (token === epoch.current) setWindowResults(pair);
      } else
        next = await rpc<QueryResult>({
          op: "check",
          recordId,
          actionId,
          parameters,
        });
      const stored = await rpc<{ samples: Sample[] }>({
        op: "history",
        recordId,
      });
      if (token !== epoch.current) return;
      setResult(next);
      setHistory(
        stored.samples.filter((s) => s.actionId === actionId).reverse(),
      );
    } catch (e) {
      if (token === epoch.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (token === epoch.current) setBusy(false);
    }
  }
  useEffect(() => {
    epoch.current++;
    setResult(undefined);
    setHistory([]);
    setWindowResults([]);
    setBusy(false);
    return () => {
      epoch.current++;
    };
  }, [recordId, panel, actionId]);
  useEffect(() => {
    if (
      !visible ||
      !refresh ||
      panel === "performance" ||
      actionId === "PG-MON-015"
    )
      return;
    const timer = setInterval(() => {
      if (!document.hidden) void run();
    }, 30000);
    return () => clearInterval(timer);
  }, [visible, refresh, recordId, panel, actionId]);
  const comparison =
    history.length >= 3
      ? compareWindows(
          history.at(-3)!,
          history.at(-2)!,
          history.at(-2)!,
          history.at(-1)!,
        )
      : null;
  const seriesKeys = Array.from(
    new Set(history.flatMap((s) => s.series.map((r) => r.key))),
  );
  return (
    <div className="db-diagnostics">
      <nav className="db-section-tabs">
        {[
          ["blocking", "Kim kimi kilitliyor?"],
          ["performance", "Neden yavaşladı?"],
          ["storage", "Alan ne hızla doluyor?"],
        ].map(([key, label]) => (
          <button
            key={key}
            className={panel === key ? "active" : ""}
            onClick={() => setPanel(key as typeof panel)}
          >
            {label}
          </button>
        ))}
      </nav>
      <div className="db-toolbar">
        {panel === "storage" && (
          <select
            value={actionId}
            onChange={(e) => setStorageAction(e.target.value)}
          >
            {storageActions[profile.engine].map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        )}
        <button className="primary" disabled={busy} onClick={() => void run()}>
          {busy ? "Ölçülüyor…" : "Kontrolü çalıştır"}
        </button>
        <label>
          <input
            type="checkbox"
            disabled={panel === "performance" || actionId === "PG-MON-015"}
            checked={refresh && panel !== "performance"}
            onChange={(e) => setRefresh(e.target.checked)}
          />{" "}
          Görünürken 30 sn
        </label>
        <span>
          {result?.observedAt && now - result.observedAt > 90000
            ? "stale"
            : (result?.status ?? "Henüz ölçülmedi")}{" "}
          ·{" "}
          {result?.observedAt
            ? new Date(result.observedAt).toLocaleTimeString("tr")
            : ""}
        </span>
      </div>
      {error && <pre className="db-query-error">{error}</pre>}
      {result?.message && <p className="db-warning">{result.message}</p>}
      {panel === "blocking" ? (
        <div className="db-blocking">
          {blockingTree(result, profile.engine).map((node, i) => (
            <div key={i} style={{ marginLeft: Math.min(node.depth, 16) * 18 }}>
              <strong>
                {node.parent && node.parent !== "0" ? node.parent + " → " : ""}
                {node.id}
              </strong>
              <span>
                {node.wait} · {node.age}
              </span>
            </div>
          ))}
          {result && !blockingTree(result, profile.engine).length && (
            <p>
              Görünür kapsamda kilit kaydı yok. Yetki ve örnek zamanı sonucu
              sınırlar.
            </p>
          )}
        </div>
      ) : panel === "performance" ? (
        <>
          <p>
            {profile.engine === "mssql"
              ? "Son tamamlanmış iki saat ayrı Query Store pencereleri olarak okunur. Yalnız pencereye tam sığan aralıklar dahildir; kapalı Query Store veya eksik aralık boş sonuç verir."
              : "Son üç örnekten iki ayrı zaman penceresi oluşturulur. Süreler birlikte gösterilir; sayaç sıfırlanması karşılaştırmayı geçersiz kılar."}{" "}
            Ham SQL metni geçmişe yazılmaz.
          </p>
          <pre>
            {profile.engine === "mssql"
              ? windowResults.length
                ? JSON.stringify(
                    windowResults.map((r, i) => ({
                      window: i === 0 ? "Önceki saat" : "Son saat",
                      status: r.status,
                      rows: rows(r).map(({ query_text, ...values }) => values),
                    })),
                    null,
                    2,
                  )
                : "Karşılaştırma için kontrolü çalıştırın."
              : !comparison
                ? "Karşılaştırma için en az üç örnek gerekli."
                : comparison.valid
                  ? JSON.stringify(comparison.windows, null, 2)
                  : comparison.reason}
          </pre>
        </>
      ) : (
        <>
          <div className="db-toolbar">
            <label>
              Uyarı %{" "}
              <input
                type="number"
                value={warning}
                onChange={(e) => setWarning(Number(e.target.value))}
              />
            </label>
            <label>
              Kritik %{" "}
              <input
                type="number"
                value={critical}
                onChange={(e) => setCritical(Number(e.target.value))}
              />
            </label>
            <button
              onClick={() => {
                if (warning < 0 || critical <= warning || critical > 100) {
                  setError("Eşikler 0 ≤ uyarı < kritik ≤ 100 olmalı");
                  return;
                }
                void onPolicy(warning, critical).catch((e) =>
                  setError(errorText(e)),
                );
              }}
            >
              Eşikleri kaydet
            </button>
          </div>
          {seriesKeys.map((key) => {
            const points = history.map((s) => ({
              at: s.observedAt,
              values: s.series.find((r) => r.key === key)?.values ?? {},
            }));
            const columns = Array.from(
              new Set(points.flatMap((s) => Object.keys(s.values))),
            );
            return (
              <div className="db-metric-card" key={key}>
                <small>
                  {history.at(-1)?.series.find((s) => s.key === key)?.label ||
                    `Seri ${key.slice(0, 8)}`}
                </small>
                {columns.map((name) => {
                  const values = points
                    .filter((p) => p.values[name] !== undefined)
                    .map((p) => ({ at: p.at, value: p.values[name] }));
                  const rate = growth(values),
                    percent = /percent|pct/.test(name),
                    last = values.at(-1);
                  return (
                    <p key={name}>
                      <strong>{name}</strong> {metricText(name, last?.value)} ·{" "}
                      {percent
                        ? !points.at(-1)?.values[name] &&
                          points.at(-1)?.values[name] !== 0
                          ? "unknown"
                          : result?.status !== "ok"
                            ? result?.status
                            : capacity(values, warning, critical, now)
                        : rate === null
                          ? "Hız için yeterli/sürekli örnek yok"
                          : `${metricText(name, rate)} / sn`}
                      {percent &&
                        rate !== null &&
                        rate > 0 &&
                        last &&
                        last.value >= 0 &&
                        last.value < 100 && (
                          <small>
                            Dolma tahmini:{" "}
                            {Math.ceil((100 - last.value) / rate / 60)} dk ·
                            gözlenen eğilim
                          </small>
                        )}
                      <svg width="160" height="28" viewBox="0 0 160 28">
                        <polyline
                          fill="none"
                          stroke="#86b3e0"
                          strokeWidth="2"
                          points={values
                            .slice(-30)
                            .map(
                              (v, i, a) =>
                                `${(i * 160) / Math.max(1, a.length - 1)},${26 - (24 * (v.value - Math.min(...a.map((x) => x.value)))) / Math.max(1, Math.max(...a.map((x) => x.value)) - Math.min(...a.map((x) => x.value)))}`,
                            )
                            .join(" ")}
                        />
                      </svg>
                    </p>
                  );
                })}
              </div>
            );
          })}
          {!history.length && (
            <p>Metrik geçmişi yok. NULL değerler sıfır sayılmaz.</p>
          )}
        </>
      )}
      <p className="db-muted">
        Geçmiş: 7 gün / kasa başına 50 MiB. Uygulama kapalıyken ölçüm yapılmaz.
        Boş sonuç sağlık garantisi değildir.
      </p>
    </div>
  );
}
