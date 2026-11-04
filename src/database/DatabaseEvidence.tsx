import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import { MetadataSelect, useMetadataSession } from "./MetadataPicker";
import {
  backupObservation,
  evidenceLabels,
  transactionObservation,
  evidenceRows,
  type Evidence,
} from "./evidence";
export default function DatabaseEvidence({
  recordId,
  sessionId,
  initialSchema,
}: {
  recordId: string;
  sessionId?: string;
  initialSchema: string;
}) {
  const { session, error: sessionError } = useMetadataSession(
    recordId,
    sessionId,
  );
  const [panel, setPanel] = useState("indexes"),
    [schema, setSchema] = useState(initialSchema),
    [table, setTable] = useState(""),
    [result, setResult] = useState<Evidence>(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [maxAge, setMaxAge] = useState(24),
    [transactionAge, setTransactionAge] = useState(300),
    [now, setNow] = useState(Date.now());
  const epoch = useRef(0),
    pending = useRef(false);
  useEffect(() => {
    epoch.current++;
    setResult(undefined);
    setError("");
    return () => {
      epoch.current++;
    };
  }, [session, panel, schema, table]);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  async function run() {
    if (pending.current || !session) return;
    const id = ++epoch.current;
    pending.current = true;
    setBusy(true);
    setError("");
    setResult(undefined);
    try {
      const value = await call<Evidence>("database_request", {
        request: { op: "evidence", sessionId: session, panel, schema, table },
      });
      if (id === epoch.current) {
        setResult(value);
        setNow(Date.now());
      }
    } catch (e) {
      if (id === epoch.current) setError(errorText(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return (
    <section className="db-evidence">
      <h2>İndeks, yedek ve oturum incelemesi</h2>
      <div className="db-toolbar">
        <button
          className={panel === "indexes" ? "active" : ""}
          onClick={() => setPanel("indexes")}
        >
          İndeks kanıtları
        </button>
        <button
          className={panel === "backups" ? "active" : ""}
          onClick={() => setPanel("backups")}
        >
          Yedek güncelliği
        </button>
        <button
          className={panel === "connections" ? "active" : ""}
          onClick={() => setPanel("connections")}
        >
          Bağlantılar ve transaction
        </button>
      </div>
      {panel === "connections" ? (
        <>
          <p>
            Bağlantı kapasitesi, kullanıcı/uygulama dağılımı ve açık
            transaction'lar ayrı kapsamlarda okunur. Hiçbir oturum otomatik
            sonlandırılmaz.
          </p>
          <label>
            Transaction yaş uyarısı (saniye){" "}
            <input
              type="number"
              min="1"
              max="86400"
              aria-label="Transaction yaş sınırı"
              value={transactionAge}
              onChange={(e) => setTransactionAge(Number(e.target.value))}
            />
          </label>
        </>
      ) : panel === "indexes" ? (
        <>
          <p>
            Okuma kullanımı, yazma yükü, alan ve bağımlılıkları birlikte
            inceleyin. Sıfır kullanım silme önerisi değildir; bu ekranda DDL
            çalıştırılmaz.
          </p>
          <div className="db-object-picker">
            <MetadataSelect
              sessionId={session}
              kind="schemas"
              value={schema}
              label="İndeks şeması"
              onChange={(v) => {
                setSchema(v);
                setTable("");
              }}
            />
            <MetadataSelect
              sessionId={session}
              kind="tables"
              schema={schema}
              value={table}
              label="İndeks tablosu"
              onChange={setTable}
            />
          </div>
        </>
      ) : (
        <>
          <p>
            Son yedek kaydı, yedek dosyasının varlığını veya geri
            yüklenebilirliğini kanıtlamaz. Restore geçmişi ve doğrulanmış
            kurtarma testi ayrı kavramlardır.
          </p>
          <label>
            Yaş uyarısı (saat){" "}
            <input
              type="number"
              min="1"
              max="8760"
              aria-label="Yedek yaş sınırı"
              value={maxAge}
              onChange={(e) => setMaxAge(Number(e.target.value))}
            />
          </label>
          <small>
            Bu oturumluk eşik her yedek türüne ayrı uygulanır; bir yedek zinciri
            değerlendirmesi değildir.
          </small>
        </>
      )}
      <button
        className="primary"
        disabled={
          busy || !session || (panel === "indexes" && (!schema || !table))
        }
        onClick={() => void run()}
      >
        {busy ? "Kanıtlar okunuyor…" : "Kanıtları getir"}
      </button>
      {(error || sessionError) && <p role="alert">{error || sessionError}</p>}
      {result && (
        <>
          <p>
            Ölçüm: {new Date(result.observedAt).toLocaleString("tr")} ·{" "}
            {now - result.observedAt > 60000
              ? "Eski veri · yeniden okuyun"
              : "Anlık okuma"}{" "}
            · Her kaynak ayrı yetki ve kapsamla okunur.
          </p>
          {result.parts.map((part) => (
            <article
              className="db-evidence-part"
              key={part.id}
              data-source={part.id}
            >
              <h3>
                {part.title}{" "}
                <small>{evidenceLabels[part.status] ?? part.status}</small>
              </h3>
              <p>{part.note}</p>
              {part.message && <p role="alert">{part.message}</p>}
              {panel === "connections" &&
                part.id === "transactions" &&
                evidenceRows(part).map((row, i) => {
                  const observation = transactionObservation(
                    row,
                    transactionAge,
                  );
                  return (
                    <p
                      key={i}
                      className={`db-evidence-state ${observation.state}`}
                    >
                      Oturum {row.pid ?? row.session_id ?? row.sid ?? "?"} ·{" "}
                      {observation.text}
                    </p>
                  );
                })}
              {panel === "backups" &&
                part.id === "backups" &&
                evidenceRows(part).map((r, i) => {
                  const observation = backupObservation(r, maxAge);
                  return (
                    <p
                      key={i}
                      className={`db-evidence-state ${observation.state}`}
                    >
                      {r.type ?? r.input_type ?? "Yedek"} · {observation.text}
                    </p>
                  );
                })}
              {!!part.rows?.length && (
                <div className="db-results">
                  <table>
                    <thead>
                      <tr>
                        {part.columns?.map((c) => (
                          <th key={c.name}>{c.name}</th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {part.rows.map((row, i) => (
                        <tr key={i}>
                          {row.map((v, j) => (
                            <td key={j}>{v ?? "Bilinmiyor"}</td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
              {part.status === "unknown" && (
                <p>
                  Görünür kayıt bulunmadı; sağlıklı olarak değerlendirilmedi.
                </p>
              )}
            </article>
          ))}
        </>
      )}
    </section>
  );
}
