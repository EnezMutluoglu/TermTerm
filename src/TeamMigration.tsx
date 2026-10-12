import { useEffect, useRef, useState } from "react";
import { call, chooseFile, errorText, saveFile } from "./api";
type Preview = {
  sourceTeamName: string;
  sourceSha256: string;
  capturedAt: string;
  issueRecords: number;
  vaults: { sourceId: string; name: string; recordCount: number }[];
  members: {
    sourceId: string;
    username: string;
    role: string;
    active: boolean;
    targetAccountId: string | null;
    mappingIssues: string[];
  }[];
  targetAccounts: { accountId: string; username: string; displayName: string; role: string; status: string }[];
  targetFingerprint: string;
  accountsReady: boolean;
  unresolvedAccounts: number;
  rules: {
    kind: string;
    sourceAccountId: string;
    permission: string;
    permissions: string[];
    effect: string;
  }[];
  records: {
    sourceId: string;
    kind: string;
    name: string;
    deleted: boolean;
    issues: string[];
    unsupportedFields: string[];
  }[];
  limitations: string[];
};
export default function TeamMigration({
  sourceTeamId,
}: {
  sourceTeamId?: string;
}) {
  const [path, setPath] = useState(""),
    [password, setPassword] = useState(""),
    [busy, setBusy] = useState(false);
  const [error, setError] = useState(""),
    [result, setResult] = useState<Preview | null>(null),
    [notice, setNotice] = useState("");
  const [query, setQuery] = useState(""),
    [page, setPage] = useState(0);
  const [mappingPassword, setMappingPassword] = useState("");
  const [bindings, setBindings] = useState<Record<string, string>>({});
  const [mappingDirty, setMappingDirty] = useState(false);
  const [accountPage, setAccountPage] = useState(0);
  const [targetQuery, setTargetQuery] = useState("");
  const gate = useRef(false),
    generation = useRef(0);
  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );
  const reset = () => {
    setResult(null);
    setNotice("");
    setError("");
    setPage(0);
    setAccountPage(0);
    setBindings({});
    setMappingPassword("");
    setMappingDirty(false);
  };
  const execute = async (mapping = false) => {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    if (!mapping) reset();
    else { setError(""); setNotice(""); setMappingDirty(true); }
    const epoch = generation.current;
    try {
      if (sourceTeamId) {
        const dest = await saveFile("ttmigration", "Team-gecis.ttmigration");
        if (!dest) return;
        const r = await call<{
          vaults: number;
          records: number;
          unreadable: number;
        }>("team_migration_export", {
          teamId: sourceTeamId,
          path: dest,
          password,
        });
        if (epoch === generation.current)
          setNotice(
            `${r.vaults} kasa, ${r.records} kayıt arşivlendi. Açılamayan: ${r.unreadable}. Kaynak değiştirilmedi.`,
          );
      } else {
        const r = await call<Preview>("team_migration_preview", {
          path,
          password: mapping ? mappingPassword : password,
          mappings: mapping ? Object.entries(bindings).filter(([,id])=>id).map(([sourceAccountId,targetAccountId])=>({sourceAccountId,targetAccountId})) : null,
          expectedSourceSha256: mapping ? result?.sourceSha256 : null,
          expectedTargetFingerprint: mapping ? result?.targetFingerprint : null,
        });
        if (epoch === generation.current) {
          setResult(r);
          setBindings(Object.fromEntries(r.members.map(m=>[m.sourceId,m.targetAccountId??""])));
          setMappingDirty(false);
        }
      }
    } catch (e) {
      if (epoch === generation.current) setError(errorText(e));
    } finally {
      if (epoch === generation.current) {
        setBusy(false);
        setPassword("");
        setMappingPassword("");
      }
      gate.current = false;
    }
  };
  const filtered =
    result?.records.filter((r) =>
      `${r.name} ${r.kind}`
        .toLocaleLowerCase("tr")
        .includes(query.toLocaleLowerCase("tr")),
    ) ?? [];
  return (
    <section
      className="ts-migration"
      aria-label="Eski Team geçişi"
      aria-busy={busy}
    >
      <h2>v0.4.0 Team geçişi</h2>
      <p>
        {sourceTeamId
          ? "Eski takımın sahibi, sunucudaki güncel kayıt ve izinleri şifreli geçiş arşivine alabilir. Yerelde bekleyen düzenlemeleri önce eşitleyin."
          : "Eski Team → Bağlantı ve politika ekranından alınan .ttmigration arşivini açın. Yeni takıma hiçbir kayıt yazılmaz."}
      </p>
      <p className="muted">
        Bu arşiv tam PostgreSQL yedeği değildir; geçmiş/audit kayıtlarını
        içermez. Tam geçiş öncesinde ayrıca veritabanı yedeği alınmalıdır.
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void execute();
        }}
      >
        {!sourceTeamId && (
          <label>
            Geçiş arşivi
            <div className="ts-migration-file">
              <input
                aria-label="Geçiş arşivi"
                value={path}
                disabled={busy}
                onChange={(e) => {
                  setPath(e.target.value);
                  reset();
                }}
              />
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void chooseFile(["ttmigration"])
                    .then((p) => {
                      if (p) {
                        setPath(p);
                        reset();
                      }
                    })
                    .catch((e) => setError(errorText(e)))
                }
              >
                Dosya seç
              </button>
            </div>
          </label>
        )}
        <label>
          Arşiv parolası
          <input
            type="password"
            autoComplete="off"
            value={password}
            disabled={busy}
            onChange={(e) => {
              setPassword(e.target.value);
              reset();
            }}
            minLength={sourceTeamId ? 12 : 1}
            required
          />
        </label>
        <button disabled={busy || (!sourceTeamId && !path) || !password}>
          {busy
            ? "İşleniyor…"
            : sourceTeamId
              ? "Şifreli geçiş arşivi oluştur"
              : "Salt okunur önizleme"}
        </button>
      </form>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      {result && (
        <>
          <h3>{result.sourceTeamName}</h3>
          <p>
            {result.vaults.length} kasa · {result.records.length} kayıt ·{" "}
            {result.members.length} hesap · {result.rules.length} izin/yetki
            kuralı · {result.issueRecords} sorunlu kayıt
          </p>
          <small>
            Kaynak zamanı: {result.capturedAt}
            <br />
            SHA256: {result.sourceSha256}
          </small>
          <ul>
            {result.limitations.map((x) => (
              <li key={x}>{x}</li>
            ))}
          </ul>
          <details>
            <summary>
              Kaynak ve hedef hesap eşlemesi
            </summary>
            <p>İsim benzerliğiyle hesaplar birleştirilmez. Hedef hesabı Üyeler alanında oluşturun; etkin üyeler ilk girişlerini tamamlamalı. Roller aynı olmalı, kapalı kaynak hesapları kapalı kalmalı.</p>
            <label>Hedef hesap ara<input disabled={busy} value={targetQuery} onChange={e=>setTargetQuery(e.target.value)} /></label>
            <form onSubmit={e=>{e.preventDefault();void execute(true);}}>
            <table>
              <thead>
                <tr>
                  <th>Kullanıcı</th>
                  <th>Rol</th>
                  <th>Durum</th>
                  <th>Hedef hesap</th>
                  <th>Kontrol</th>
                </tr>
              </thead>
              <tbody>
                {result.members.slice(accountPage*100, (accountPage+1)*100).map((m) => (
                  <tr key={m.sourceId}>
                    <td>{m.username}</td>
                    <td>{m.role}</td>
                    <td>{m.active ? "Etkin" : "Kapalı"}</td>
                    <td><select aria-label={`${m.username} hedef hesabı`} disabled={busy} value={bindings[m.sourceId]??""} onChange={e=>{setBindings(b=>({...b,[m.sourceId]:e.target.value}));setMappingDirty(true);}}>
                      <option value="">Hesap seçin</option>
                      {(result.targetAccounts??[]).filter(t=>t.accountId===bindings[m.sourceId] || `${t.username} ${t.displayName}`.toLocaleLowerCase("tr").includes(targetQuery.toLocaleLowerCase("tr"))).filter((t,i)=>i<100 || t.accountId===bindings[m.sourceId]).map(t=><option key={t.accountId} value={t.accountId}>{t.displayName} ({t.username}) · {t.role} · {t.status}</option>)}
                    </select></td>
                    <td>{mappingDirty ? "Değişiklik yeniden doğrulanmalı" : m.mappingIssues?.join(" · ") || "Eşleme doğrulandı"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div><button type="button" disabled={busy||accountPage===0} onClick={()=>setAccountPage(p=>p-1)}>Önceki hesaplar</button>
              <span> {accountPage+1} / {Math.max(1,Math.ceil(result.members.length/100))} </span>
              <button type="button" disabled={busy||(accountPage+1)*100>=result.members.length} onClick={()=>setAccountPage(p=>p+1)}>Sonraki hesaplar</button></div>
            <small>Aramada ilk 100 hedef ve mevcut seçim gösterilir; listeyi isimle daraltın.</small>
            <label>Eşleme doğrulaması için arşiv parolası<input type="password" autoComplete="off" required disabled={busy} value={mappingPassword} onChange={e=>setMappingPassword(e.target.value)} /></label>
            <button disabled={busy||!mappingPassword}>Hesap eşlemesini doğrula</button>
            </form>
            <p role="status">{mappingDirty ? "Eşleme değişti; yeniden doğrulayın." : result.accountsReady ? "Hesap eşlemesi doğrulandı. Veriler henüz aktarılmadı." : `${result.unresolvedAccounts??result.members.length} hesap için seçim veya aktivasyon gerekli.`}</p>
          </details>
          <details>
            <summary>İzinler ve açık engeller (ilk 100)</summary>
            <table>
              <thead>
                <tr>
                  <th>Üye</th>
                  <th>Kural</th>
                  <th>İzin</th>
                  <th>Etkisi</th>
                </tr>
              </thead>
              <tbody>
                {result.rules.slice(0, 100).map((a, i) => (
                  <tr key={i}>
                    <td>
                      {result.members.find(
                        (m) => m.sourceId === a.sourceAccountId,
                      )?.username ?? a.sourceAccountId}
                    </td>
                    <td>{a.kind}</td>
                    <td>{a.permission ?? a.permissions?.join(", ")}</td>
                    <td>{a.effect ?? "yetki devri"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </details>
          <label>
            Kayıt ara
            <input
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setPage(0);
              }}
            />
          </label>
          <table>
            <thead>
              <tr>
                <th>Kayıt</th>
                <th>Tür</th>
                <th>Durum / dönüşüm kararı</th>
              </tr>
            </thead>
            <tbody>
              {filtered.slice(page * 100, (page + 1) * 100).map((r) => (
                <tr key={r.sourceId}>
                  <td>{r.name || r.sourceId}</td>
                  <td>
                    {r.kind}
                    {r.deleted ? " (silinmiş)" : ""}
                  </td>
                  <td>
                    {r.issues.join(" · ") || "Önizlendi"}
                    {r.unsupportedFields.length > 0 && (
                      <small>Alanlar: {r.unsupportedFields.join(", ")}</small>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div>
            <button disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
              Önceki
            </button>
            <span>
              {" "}
              {page + 1} / {Math.max(1, Math.ceil(filtered.length / 100))}{" "}
            </span>
            <button
              disabled={(page + 1) * 100 >= filtered.length}
              onClick={() => setPage((p) => p + 1)}
            >
              Sonraki
            </button>
          </div>
          <p>
            Hesap eşlemesi veri aktarmak veya erişim vermek değildir. Hedefe
            atomik uygulama bu geliştirme adımında henüz etkin değil.
          </p>
        </>
      )}
    </section>
  );
}
