import { useEffect, useRef, useState } from "react";
import { call, errorText } from "./api";

type Content = {
  kind: string;
  parentId: string | null;
  name: string;
  address: string | null;
  protocol: string | null;
  port: number | null;
  usernamePolicy: string | null;
  username: string | null;
  deleted: boolean;
};
type Candidate = {
  id: string;
  resourceId: string;
  expectedRevision: number;
  content: Content | null;
  error: string | null;
};
type Current = {
  id: string;
  vaultId: string;
  revision: number;
  content: Content | null;
};
const fields: { key: keyof Content; label: string }[] = [
  { key: "name", label: "Ad" },
  { key: "address", label: "Adres" },
  { key: "port", label: "Port" },
  { key: "protocol", label: "Protokol" },
  { key: "usernamePolicy", label: "Kullanıcı politikası" },
  { key: "username", label: "Ortak kullanıcı adı" },
  { key: "parentId", label: "Üst klasör" },
  { key: "deleted", label: "Silinmiş" },
];
const display = (v: unknown) =>
  v === null ? "—" : v === true ? "Evet" : v === false ? "Hayır" : String(v);

export default function TeamDecisions() {
  const live = useRef(true),
    gate = useRef(false);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [rows, setRows] = useState<Candidate[]>([]),
    [selected, setSelected] = useState<Candidate | null>(null),
    [current, setCurrent] = useState<Current | null>(null),
    [merged, setMerged] = useState<Content | null>(null);
  async function rpc<T>(action: Record<string, unknown>): Promise<T> {
    const r = await call<T>("team_v2_action", { action });
    if (!live.current) throw Error("Ekran kapatıldı");
    return r;
  }
  async function run(task: () => Promise<void>) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      await task();
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      gate.current = false;
      if (live.current) setBusy(false);
    }
  }
  async function load() {
    const all: Candidate[] = [];
    let after: string | null = null;
    do {
      const page: { conflicts: Candidate[]; nextAfter: string | null } =
        await rpc({ kind: "conflicts", after });
      all.push(...page.conflicts);
      after = page.nextAfter;
    } while (after);
    setRows(all);
  }
  useEffect(() => {
    live.current = true;
    void run(load);
    return () => {
      live.current = false;
    };
  }, []);
  async function inspect(row: Candidate) {
    setSelected(row);
    setCurrent(null);
    setMerged(null);
    const value = await rpc<Current>({ kind: "conflictCurrent", id: row.id });
    setCurrent(value);
    setMerged(value.content ? { ...value.content } : row.content ? {...row.content} : null);
  }
  async function decide(replacement: Content | null) {
    if (!current || !selected) return;
    const result=await rpc<{localSyncPending?:boolean}>({
      kind: "decideConflict",
      id: selected.id,
      resourceId: current.id,
      vaultId: current.vaultId,
      currentRevision: current.revision,
      clientAt: new Date().toISOString(),
      replacement,
    });
    setSelected(null);
    setCurrent(null);
    setMerged(null);
    setNotice(
      (replacement
        ? "Seçtiğin içerik yeni sürüm olarak kaydedildi."
        : "Sunucudaki sürüm korundu.") + (result.localSyncPending ? " Yerel günlüğün karar makbuzu henüz alınamadı; eşitleme yeniden deneyecek." : ""),
    );
    await load();
  }
  return (
    <section className="ts-decisions" aria-busy={busy}>
      <p>
        Yalnız karar verme yetkinin olduğu değişiklikler gösterilir.
        Karşılaştırmadan sonra kayıt yeniden değişirse karar reddedilir; yeniden
        karşılaştırmalısın.
      </p>
      <button disabled={busy} onClick={() => void run(load)}>
        Kararları yenile
      </button>
      {error && (
        <p role="alert" className="ts-error">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      {!busy && !rows.length && <p>Bekleyen karar yok.</p>}
      {rows.map((row) => (
        <button
          className="ts-decision-row"
          disabled={busy}
          key={row.id}
          onClick={() => void run(() => inspect(row))}
        >
          <strong>{row.content?.name ?? "Açılamayan değişiklik"}</strong>
          <span>Başlangıç sürümü {row.expectedRevision}</span>
        </button>
      ))}
      {selected && (
        <div className="ts-overlay">
          <section
            className="ts-dialog ts-wide"
            role="dialog"
            aria-modal="true"
            aria-label="Çakışmayı karşılaştır"
          >
            <h2>Çakışmayı karşılaştır</h2>
            {error && <p role="alert">{error}</p>}
            {selected.error && (
              <p role="alert">
                Aday kayıt çözülemedi. Ortak kayıt değişmedi. {selected.error}
              </p>
            )}
            {current && (
              <>
                <p>
                  {current.revision === 0 ? "Yeni kayıt adayı. Onaylanana kadar ortak Hosts listesine eklenmez." : `Sunucudaki sürüm: ${current.revision}. Her alan için korunacak değeri seçebilirsin.`}
                </p>
                <table>
                  <thead>
                    <tr>
                      <th>Alan</th>
                      <th>Sunucudaki</th>
                      <th>Aday değişiklik</th>
                      <th>Korunacak</th>
                    </tr>
                  </thead>
                  <tbody>
                    {fields.map(({ key, label }) => (
                      <tr key={key}>
                        <td>{label}</td>
                        <td>{current.content ? display(current.content[key]) : "Kayıt yok"}</td>
                        <td>
                          {selected.content
                            ? display(selected.content[key])
                            : "Açılamadı"}
                        </td>
                        <td>
                          <select
                            aria-label={`${label} kaynağı`}
                            disabled={
                              busy || !selected.content || !current.content || key === "parentId"
                            }
                            value={
                              current.content && merged?.[key] === current.content[key]
                                ? "server"
                                : "candidate"
                            }
                            onChange={(e) =>
                              setMerged({
                                ...merged!,
                                [key]:
                                  e.target.value === "server"
                                    ? current.content![key]
                                    : selected.content![key],
                              })
                            }
                          >
                            <option value="server">Sunucu</option>
                            <option value="candidate">Aday</option>
                          </select>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <small>
                  Klasör taşıma ayrı yetki önizlemesi gerektirir; burada üst
                  klasör değiştirilemez.
                </small>
                <div className="ts-actions">
                  <button
                    disabled={busy}
                    onClick={() => void run(() => decide(null))}
                  >
                    {current.revision===0 ? "Yeni kaydı reddet" : "Sunucudakini koru"}
                  </button>
                  <button
                    className="ts-primary"
                    disabled={busy || !selected.content || !merged}
                    onClick={() => void run(() => decide(merged))}
                  >
                    {current.revision===0 ? "Yeni kaydı onayla" : "Seçilen alanları yeni sürüm yap"}
                  </button>
                </div>
              </>
            )}
            {!current && busy && <p>Güncel kayıt alınıyor…</p>}
            <button
              onClick={() => {
                setSelected(null);
                setCurrent(null);
                setMerged(null);
              }}
              disabled={busy}
            >
              Kapat
            </button>
          </section>
        </div>
      )}
    </section>
  );
}
