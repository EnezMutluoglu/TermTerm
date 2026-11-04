import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import { copyText } from "../clipboard";
import { Modal } from "../components";
import { useMetadataSession } from "./MetadataPicker";
import type { DatabaseConnection } from "./types";
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
type Plan = {
  planId: string;
  hash: string;
  host: string;
  database: string;
  schema?: string;
  steps?: { database: string; label: string; sql: string }[];
  sql?: string;
  names?: string[];
};
type Outcome = {
  state: string;
  message: string;
  database?: string;
  completedSteps?: number;
  totalSteps?: number;
  credentials?: { username: string; password: string }[];
};
type Environment = {
  identity: Record<string, string>[];
  providerRoles: Record<string, string>[];
  settings: Record<string, string>[];
  extensions: {
    name: string;
    default_version: string;
    installed_version: string | null;
    trusted: string;
    superuser: string;
    required_schema: string | null;
  }[];
  schemas: { nspname: string }[];
  validation: string;
};
export default function ProvisioningDialog({
  recordId,
  profile,
  onClose,
}: {
  recordId: string;
  profile: DatabaseConnection;
  onClose: () => void;
}) {
  const { session, error: sessionError } = useMetadataSession(recordId);
  const [mode, setMode] = useState<"database" | "extensions">("database"),
    [name, setName] = useState(""),
    [locale, setLocale] = useState(
      profile.engine === "mssql" ? "Turkish_CI_AS" : "tr_TR.UTF-8",
    );
  const [oracleMode, setOracleMode] = useState("schema"),
    [tablespace, setTablespace] = useState("USERS"),
    [quotaMb, setQuota] = useState(1024),
    [pdbMaxMb, setMax] = useState(4096);
  const [env, setEnv] = useState<Environment>(),
    [extension, setExtension] = useState(""),
    [schema, setSchema] = useState("public");
  const [plan, setPlan] = useState<Plan>(),
    [outcome, setOutcome] = useState<Outcome>(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [confirmed, setConfirmed] = useState(false);
  const [inspection, setInspection] = useState<{
    message: string;
    sections: {
      label: string;
      state: string;
      message: string;
      rows: Record<string, string | null>[];
    }[];
    expectedSteps: { label: string; sql: string }[];
  } | null>(null);
  const pending = useRef(false),
    live = useRef(true),
    draft = useRef<{ plan: Plan; kind: string } | undefined>(undefined);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
      const d = draft.current;
      if (d && session)
        void rpc({
          op: d.kind + "Discard",
          sessionId: session,
          planId: d.plan.planId,
        }).catch(() => {});
    };
  }, [session]);
  function invalidate() {
    setInspection(null);
    const d = draft.current;
    draft.current = undefined;
    setPlan(undefined);
    setConfirmed(false);
    setError("");
    if (d && session)
      void rpc({
        op: d.kind + "Discard",
        sessionId: session,
        planId: d.plan.planId,
      }).catch(() => {});
  }
  async function environment() {
    if (!session || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const r = await rpc<Environment>({
        op: "extensionEnvironment",
        sessionId: session,
      });
      if (live.current) {
        setEnv(r);
        if (!r.schemas.some((s) => s.nspname === schema))
          setSchema(r.schemas[0]?.nspname ?? "");
      }
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (live.current) setBusy(false);
    }
  }
  async function execute(apply: boolean) {
    if (!session || pending.current || (apply && (!plan || !confirmed))) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setOutcome(undefined);
    const previous = plan;
    setPlan(undefined);
    draft.current = undefined;
    setConfirmed(false);
    const kind = mode === "database" ? "provision" : "extension";
    try {
      if (apply) {
        const r = await rpc<Outcome>({
          op: kind + "Apply",
          sessionId: session,
          planId: previous!.planId,
          hash: previous!.hash,
        });
        if (live.current) setOutcome(r);
      } else {
        const r = await rpc<Plan>({
          op: kind + "Preview",
          sessionId: session,
          ...(mode === "database"
            ? { name, locale, oracleMode, tablespace, quotaMb, pdbMaxMb }
            : { extension, schema }),
        });
        if (live.current) {
          setPlan(r);
          draft.current = { plan: r, kind };
        }
      }
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (live.current) setBusy(false);
    }
  }
  async function inspect() {
    if (!session || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setInspection(null);
    try {
      const r = await rpc<NonNullable<typeof inspection>>({
        op: "provisionInspect",
        sessionId: session,
        name,
        locale,
        oracleMode,
        tablespace,
        quotaMb,
        pdbMaxMb,
      });
      if (live.current) setInspection(r);
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (live.current) setBusy(false);
    }
  }
  const secrets = !!outcome?.credentials?.length;
  return (
    <Modal
      wide
      title="Veritabanı ve erişim kurulumu"
      onClose={() => {
        if (!pending.current) onClose();
      }}
    >
      <section className="db-provision db-maintenance">
        <p>
          <strong>{profile.label}</strong> · {profile.host}:{profile.port} ·{" "}
          {profile.database} · {profile.username}
        </p>
        <p>
          Önizleme değişiklik yapmaz. Uygulama ayrı yönetim bağlantısı kullanır.
          Parolalar 21 karakter, büyük/küçük harf, rakam ve işaretlerden
          oluşturulur; yalnız sonuç ekranında gösterilir.
        </p>
        {profile.engine === "postgresql" && (
          <div className="db-toolbar">
            <button
              disabled={busy || secrets}
              className={mode === "database" ? "active" : ""}
              onClick={() => {
                invalidate();
                setOutcome(undefined);
                setMode("database");
              }}
            >
              DB ve kullanıcılar
            </button>
            <button
              disabled={busy || secrets || !session}
              className={mode === "extensions" ? "active" : ""}
              onClick={() => {
                invalidate();
                setOutcome(undefined);
                setMode("extensions");
                void environment();
              }}
            >
              PaaS ve extension'lar
            </button>
          </div>
        )}
        {mode === "database" ? (
          <>
            <fieldset disabled={busy || secrets}>
              <label>
                {profile.engine === "oracle" && oracleMode === "schema"
                  ? "Uygulama / schema öneki"
                  : "Yeni DB / PDB adı"}
                <input
                  aria-label="Yeni veritabanı adı"
                  value={name}
                  maxLength={48}
                  placeholder="uygulama_api_dev"
                  onChange={(e) => {
                    invalidate();
                    setName(e.target.value);
                  }}
                />
              </label>
              {profile.engine !== "oracle" ? (
                <label>
                  {profile.engine === "postgresql"
                    ? "Sunucu locale'i"
                    : "Collation"}
                  <input
                    aria-label="Kurulum locale"
                    value={locale}
                    onChange={(e) => {
                      invalidate();
                      setLocale(e.target.value);
                    }}
                  />
                </label>
              ) : (
                <>
                  <label>
                    Oracle oluşturma biçimi
                    <select
                      aria-label="Oracle oluşturma biçimi"
                      value={oracleMode}
                      onChange={(e) => {
                        invalidate();
                        setOracleMode(e.target.value);
                      }}
                    >
                      <option value="schema">
                        Mevcut PDB içinde schema + roller
                      </option>
                      <option value="pdb">
                        CDB içinde yeni PDB + schema + roller
                      </option>
                    </select>
                  </label>
                  <label>
                    Tablespace
                    <input
                      aria-label="Kurulum tablespace"
                      value={tablespace}
                      onChange={(e) => {
                        invalidate();
                        setTablespace(e.target.value);
                      }}
                    />
                  </label>
                  <label>
                    Owner depolama kotası (MiB)
                    <input
                      type="number"
                      min={10}
                      value={quotaMb}
                      onChange={(e) => {
                        invalidate();
                        setQuota(Number(e.target.value));
                      }}
                    />
                  </label>
                  {oracleMode === "pdb" && (
                    <label>
                      PDB toplam üst sınırı (MiB)
                      <input
                        type="number"
                        min={2048}
                        value={pdbMaxMb}
                        onChange={(e) => {
                          invalidate();
                          setMax(Number(e.target.value));
                        }}
                      />
                    </label>
                  )}
                </>
              )}
            </fieldset>
            <button
              disabled={busy || !session || !name}
              onClick={() => void inspect()}
            >
              Mevcut / yarım kurulumu denetle
            </button>
            {inspection && (
              <section className="db-provision-inspection">
                <p>{inspection.message}</p>
                {inspection.sections.map((part, i) => (
                  <details key={i} open>
                    <summary>
                      {part.label} ·{" "}
                      {part.state === "observed"
                        ? "Okundu"
                        : part.state === "missing"
                          ? "Görünür kayıt yok"
                          : "Bilinmiyor"}
                    </summary>
                    <p>{part.message}</p>
                    <pre>{JSON.stringify(part.rows, null, 2)}</pre>
                  </details>
                ))}
                <details>
                  <summary>Beklenen şablon adımları (uygulanmadı)</summary>
                  {inspection.expectedSteps.map((step, i) => (
                    <div key={i}>
                      <strong>
                        {i + 1}. {step.label}
                      </strong>
                      <pre>{step.sql}</pre>
                    </div>
                  ))}
                </details>
              </section>
            )}
            <details open={!plan}>
              <summary>Oluşturulacak 6 rol ve hesabın yetki özeti</summary>
              <div className="db-role-matrix">
                <table>
                  <thead>
                    <tr>
                      <th>Ad</th>
                      <th>Amaç</th>
                    </tr>
                  </thead>
                  <tbody>
                    {[
                      ["", "Ana bağlantı grubu · oturum açamaz"],
                      [
                        "_owner",
                        profile.engine === "mssql"
                          ? "DB sahibi; dbo olarak nesne oluşturur"
                          : "Sahip ve nesne oluşturma hesabı",
                      ],
                      ["_app", "CRUD ve rutin çalıştırma grubu"],
                      [
                        "_developer",
                        "CRUD ve rutin çalıştırma grubu · DDL owner ile",
                      ],
                      ["_readonly", "Tablo okuma grubu"],
                      ["_appuser", "Uygulama giriş hesabı · app üyesi"],
                    ].map(([suffix, text]) => (
                      <tr key={suffix}>
                        <td>
                          <code>
                            {name || "uygulama_api_dev"}
                            {suffix}
                          </code>
                        </td>
                        <td>{text}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </details>
            {profile.engine === "postgresql" && (
              <p>
                Gelecek nesneler owner rolüyle oluşturulmalı. Yönetici owner
                rolünün üyesi olur; owner'a azure_pg_admin/rds_superuser
                verilmez. PUBLIC fonksiyon çalıştırma ve tip erişimi
                varsayılanları da kapatılır.
              </p>
            )}
            {profile.engine === "oracle" && (
              <p>
                {oracleMode === "pdb"
                  ? "Yeni PDB için CDB$ROOT, SYSDBA bağlantısı ve sunucuda OMF gerekir. Ayrı _pdbadmin DBA hesabı da oluşturulur; üçüncü parolayı sonuçta alın. Sunucunun edition/lisans ve kapasite sınırları geçerlidir."
                  : "Mevcut PDB içinde ayrı owner schema oluşturulur; bu ayrı DB/PDB değildir."}{" "}
                Oracle 23+ schema yetkileri kullanılır. Readonly grubuna
                sequence NEXTVAL yetkisi verilmez.
              </p>
            )}
            <button
              disabled={busy || secrets || !session || !name}
              onClick={() => void execute(false)}
            >
              Kurulum planını hazırla
            </button>
          </>
        ) : (
          <>
            <p>
              İşlem seçili DB'de, yukarıdaki yönetici hesabıyla çalışır. Owner'ı
              geçici yönetici yapmak yerine mevcut yönetici kullanılır; hiçbir
              rol üyeliği değiştirilmez. Sunucu izin listeleri ve gerekli
              preload/restart adımları ayrıca sağlanmalıdır.
            </p>
            <details>
              <summary>Azure / AWS / Huawei farkları ve geçici üyelik</summary>
              <ul>
                <li>
                  Azure: azure.extensions izin listesi ve extension'ın
                  trusted/yönetici gereksinimi kontrol edilir. Owner → yönetici
                  üyeliği varken ters yönde azure_pg_admin vermek döngü
                  oluşturabilir.
                </li>
                <li>
                  AWS RDS: rds_superuser; desteklenen sürümlerde rds_extension
                  ve rds.allowed_delegated_extensions ile sınırlı delegasyon.
                  RDS ve Aurora aynı ürün/sürüm olarak varsayılmaz.
                </li>
                <li>
                  Huawei RDS: root yetkileri motor sürümüne bağlıdır. Azure rol
                  adları Huawei'ye uygulanmaz.
                </li>
                <li>
                  Geçici owner yükseltme gerekiyorsa mevcut grantor, ADMIN,
                  INHERIT ve SET seçenekleri korunarak ayrı DBA akışı gerekir.
                  Bu geliştirme sürümü bu üyelik değişimini otomatik yapmaz.
                </li>
              </ul>
            </details>
            <button
              disabled={busy || !session}
              onClick={() => void environment()}
            >
              Sunucu yetkilerini ve extension listesini yenile
            </button>
            {env && (
              <>
                <p className="db-muted">{env.validation}</p>
                <details>
                  <summary>Sunucudan okunan roller ve ayarlar</summary>
                  <pre>
                    {JSON.stringify(
                      {
                        hesap: env.identity,
                        roller: env.providerRoles,
                        ayarlar: env.settings,
                      },
                      null,
                      2,
                    )}
                  </pre>
                </details>
                {env.schemas.length === 0 && (
                  <p role="alert">
                    Bu hesap için CREATE yetkili schema görünmüyor. Hedef DB
                    sahibi/yönetici bağlantısını seçin.
                  </p>
                )}
                <fieldset disabled={busy}>
                  <label>
                    Extension
                    <select
                      aria-label="Kurulacak extension"
                      value={extension}
                      onChange={(e) => {
                        invalidate();
                        setExtension(e.target.value);
                      }}
                    >
                      <option value="">Seçin</option>
                      {env.extensions
                        .filter((e) => !e.installed_version)
                        .map((e) => (
                          <option key={e.name} value={e.name}>
                            {e.name} · {e.default_version} ·{" "}
                            {e.trusted === "t" ? "trusted" : "yönetici koşullu"}
                          </option>
                        ))}
                    </select>
                  </label>
                  <label>
                    Hedef schema
                    <select
                      aria-label="Extension schema"
                      value={schema}
                      onChange={(e) => {
                        invalidate();
                        setSchema(e.target.value);
                      }}
                    >
                      {env.schemas.map((s) => (
                        <option key={s.nspname}>{s.nspname}</option>
                      ))}
                    </select>
                  </label>
                </fieldset>
                <button
                  disabled={busy || !extension || !schema}
                  onClick={() => void execute(false)}
                >
                  Extension planını hazırla
                </button>
              </>
            )}
          </>
        )}
        {(error || sessionError) && <p role="alert">{error || sessionError}</p>}
        {busy && (
          <p role="status">
            Sunucu işlemi sürüyor… Parolaların sonuç ekranına ulaşması için
            pencereyi açık tutun.
          </p>
        )}
        {plan && (
          <div className="db-change-preview">
            <h3>
              {plan.host} → {plan.database}
              {plan.schema ? " / " + plan.schema : ""}
            </h3>
            <pre>
              {plan.steps
                ? plan.steps
                    .map(
                      (s, i) =>
                        `${i + 1}. ${s.label} (${s.database === "@admin" ? "yönetim" : "hedef"})\n${s.sql};`,
                    )
                    .join("\n\n")
                : plan.sql}
            </pre>
            <small>Plan SHA-256: {plan.hash} · 5 dakika · tek kullanım</small>
            <p>
              DB/PDB kurulumunda DDL adımları tümüyle geri alınamaz. Yarıda
              kesilirse tamamlanan adımlar korunur; tekrar çalıştırmadan önce
              DBA kontrolü gerekir.
            </p>
            <label className="db-maintenance-confirm">
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
              />
              Hedefi, SQL'i ve yetkileri kontrol ettim.
            </label>
            <button
              className="primary"
              disabled={!confirmed || busy}
              onClick={() => void execute(true)}
            >
              Onaylanan kurulumu uygula
            </button>
          </div>
        )}
        {outcome && (
          <div className="db-change-outcome" role="status">
            <strong>
              {outcome.state === "succeeded"
                ? "Tamamlandı"
                : outcome.state === "unknown_outcome"
                  ? "Sonuç belirsiz — DBA kontrolü gerekli"
                  : outcome.state === "conflict"
                    ? "Plan değişti"
                    : "Uygulanmadı"}
            </strong>
            <p>{outcome.message}</p>
            {outcome.totalSteps != null && (
              <p>
                Tamamlanan adım: {outcome.completedSteps}/{outcome.totalSteps}
              </p>
            )}
            {secrets && (
              <>
                <h3>Parolaları şimdi alın</h3>
                <p>
                  Bu pencere kapanınca tekrar gösterilmez. Kasa, sorgu geçmişi
                  veya parola panosuna otomatik kaydedilmez. Kaybolursa
                  yöneticiyle parola sıfırlamak gerekir.
                </p>
                <div className="db-once-credentials">
                  {outcome.credentials!.map((c) => (
                    <div key={c.username}>
                      <code>{c.username}</code>
                      <code data-secret="password">{c.password}</code>
                      <button
                        onClick={() =>
                          void copyText(c.password).catch((e) =>
                            setError(errorText(e)),
                          )
                        }
                      >
                        Parolayı kopyala
                      </button>
                    </div>
                  ))}
                </div>
                <button onClick={onClose}>Parolaları aldım, kapat</button>
              </>
            )}
          </div>
        )}
      </section>
    </Modal>
  );
}
