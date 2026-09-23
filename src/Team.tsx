import { useEffect, useState } from "react";
import {
  Users,
  Shield,
  FolderKey,
  History,
  RefreshCw,
  ArrowLeft,
  Database,
  AlertTriangle,
  Plus,
} from "lucide-react";
import { call, chooseFile, errorText } from "./api";
import { Busy } from "./components";
import type { Vault, Entity } from "./types";
import "./team.css";

type TeamInfo = {
  id: string;
  name: string;
  ownerId: string;
  role: string;
  aclRevision: number;
  offlineHours: number;
  auditDays: number;
};
type Overview = { user: { id: string; username: string }; teams: TeamInfo[] };
type Status = {
  user: { id: string; username: string };
  online: boolean;
  pending: number;
  oldestPending?: string;
  error?: string;
  verifiedAt?: number;
  canEditOffline: boolean;
  overview?: Overview;
};
type Member = {
  userId: string;
  username: string;
  role: string;
  active: boolean;
};
type Audit = {
  id: number;
  action: string;
  server_at: string;
  actor_id: string;
  record_id?: string;
  summary: unknown;
};
type Conflict = {
  id: string;
  recordId: string;
  authorId: string;
  reason: string;
  candidateRecord: Entity;
  receivedAt: string;
};
type Version = {
  revision: number;
  serverAt: string;
  actorId: string;
  pinned: boolean;
  record: Entity;
  restoreFrom?: number;
};
const labels: Record<string, string> = {
  read: "Bilgiler ve geçmiş",
  connect: "Bağlantı",
  edit: "Oluştur / düzenle / sil",
  reveal: "Sırları görüntüle",
  export: "Dışa aktar",
  manage: "Erişim ver",
};
const roles: Record<string, string> = {
  observer: "Gözlemci",
  operator: "Operatör",
  editor: "Editör",
  manager: "Kapsam yöneticisi",
  owner: "Sahip",
};
const rolePermissions: Record<string, string[]> = {
  observer: ["read"],
  operator: ["read", "connect"],
  editor: ["read", "connect", "edit"],
  manager: ["read", "connect", "edit", "manage"],
  owner: Object.keys(labels),
};
const request = <T,>(action: string, body: unknown = {}) =>
  call<T>("team_request", { action, body });
function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}

export function TeamLogin({
  onReady,
  onBack,
}: {
  onReady: () => void;
  onBack: () => void;
}) {
  const [register, setRegister] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [login, setLogin] = useState(""),
    [email, setEmail] = useState(""),
    [password, setPassword] = useState("");
  const [profile, setProfile] = useState({
    label: "Şirket PostgreSQL",
    host: "localhost",
    port: 55432,
    database: "termterm_team_dev",
    username: "termterm_team_app",
    password: "",
    schema: "termterm_team",
    tls: "verify-full",
    caPath: "",
  });
  async function submit(offline = false) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await call(offline ? "team_offline" : "team_auth", {
        profile,
        login,
        email,
        password,
        register,
      });
      setPassword("");
      onReady();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="team-login">
      <div className="team-login-story">
        <div className="team-kicker">
          <Users size={22} /> TERMTERM TEAM · GELİŞTİRME
        </div>
        <h1>
          Ortak altyapı.
          <br />
          Size ait erişim.
        </h1>
        <p>
          Şirket hesabınızla giriş yapın. Yalnız izin verilen klasörleri ve
          makineleri görün; değişiklikleri geçmişiyle birlikte paylaşın.
        </p>
        <div className="team-feature">
          <Shield /> Kayıt olmak kasalara erişim vermez.
        </div>
        <div className="team-feature">
          <History /> Her kayıtta önceki 10 sürüm korunur.
        </div>
        <div className="team-feature">
          <Database /> PostgreSQL + şifreli yerel önbellek.
        </div>
        <small>
          0.4.0-dev.1 · Deneme ortamı · Kararlı güncelleme yayımlanmaz
        </small>
      </div>
      <form
        className="team-login-form"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <button type="button" className="text-btn" onClick={onBack}>
          <ArrowLeft size={16} /> Kişisel kasaya dön
        </button>
        <h2>{register ? "Team hesabı oluştur" : "Team hesabına giriş"}</h2>
        <p>Hesap parolanız PostgreSQL bağlantı parolasından ayrıdır.</p>
        <details>
          <summary>Şirket PostgreSQL bağlantısı · TLS zorunlu</summary>
          <div className="team-fields">
            {(
              [
                ["host", "Adres"],
                ["port", "Port"],
                ["database", "Veritabanı"],
                ["username", "Uygulama DB kullanıcısı"],
                ["password", "Uygulama DB parolası"],
                ["caPath", "CA sertifikası yolu"],
              ] as const
            ).map(([key, label]) => (
              <Field key={key} label={label}>
                <input
                  type={
                    key === "password"
                      ? "password"
                      : key === "port"
                        ? "number"
                        : "text"
                  }
                  value={profile[key]}
                  onChange={(e) =>
                    setProfile({
                      ...profile,
                      [key]:
                        key === "port"
                          ? Number(e.target.value)
                          : e.target.value,
                    })
                  }
                />
              </Field>
            ))}
            <button
              type="button"
              onClick={async () => {
                const path = await chooseFile(["crt", "pem"]);
                if (path) setProfile({ ...profile, caPath: path });
              }}
            >
              CA dosyası seç
            </button>
          </div>
        </details>
        <Field
          label={register ? "Kullanıcı adı" : "Kullanıcı adı veya e-posta"}
        >
          <input
            required
            autoComplete="username"
            value={login}
            onChange={(e) => setLogin(e.target.value)}
          />
        </Field>
        {register && (
          <Field label="E-posta">
            <input
              required
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
          </Field>
        )}
        <Field label="Hesap parolası">
          <input
            required
            type="password"
            minLength={12}
            autoComplete={register ? "new-password" : "current-password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </Field>
        <small>
          En az 12 karakter, en fazla 72 UTF-8 baytı. Bu denemede e-posta ile
          parola sıfırlama yoktur.
        </small>
        {error && (
          <div className="notice error" role="alert">
            {error}
          </div>
        )}
        <button className="primary" disabled={busy}>
          {busy ? "Bağlanıyor…" : register ? "Kayıt ol" : "Giriş yap"}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => setRegister(!register)}
        >
          {register ? "Zaten hesabım var" : "Yeni hesap oluştur"}
        </button>
        {!register && (
          <button
            type="button"
            disabled={busy || !password || !login}
            onClick={() => void submit(true)}
          >
            Şifreli önbellekle çevrimdışı aç
          </button>
        )}
        <p className="muted">
          İlk kullanım çevrimiçi yapılır. Sunucunun reddettiği oturum çevrimdışı
          girişle aşılamaz.
        </p>
      </form>
    </div>
  );
}

export default function Team({
  vault,
  onVault,
  onLogout,
}: {
  vault: Vault | null;
  onVault: (v: Vault) => void;
  onLogout: () => void;
}) {
  const [tab, setTab] = useState("vaults"),
    [status, setStatus] = useState<Status>(),
    [overview, setOverview] = useState<Overview>(),
    [teamId, setTeamId] = useState("");
  const [vaults, setVaults] = useState<
      { id: string; name: string; revision: number }[]
    >([]),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [message, setMessage] = useState("");
  const [members, setMembers] = useState<Member[]>([]),
    [memberId, setMemberId] = useState(""),
    [scope, setScope] = useState(""),
    [role, setRole] = useState("operator"),
    [query, setQuery] = useState(""),
    [found, setFound] = useState<
      { id: string; username: string; email: string }[]
    >([]);
  const [name, setName] = useState(""),
    [acl, setAcl] = useState<
      {
        user_id: string;
        resource_id: string | null;
        permission: string;
        effect: string;
      }[]
    >([]),
    [audit, setAudit] = useState<Audit[]>([]),
    [conflicts, setConflicts] = useState<Conflict[]>([]),
    [recordId, setRecordId] = useState(""),
    [versions, setVersions] = useState<Version[]>([]),
    [temporary, setTemporary] = useState<number>(),
    [bytes, setBytes] = useState<number>();
  const [merging, setMerging] = useState<string>(),
    [mergeText, setMergeText] = useState("");
  const [moveParent,setMoveParent]=useState("");
  const [movePreview,setMovePreview]=useState<{aclRevision:number;recordRevision:number;before:{id:string;recipients:{userId:string}[]}[];after:{id:string;recipients:{userId:string}[]}[]} >();
  async function decide(c: Conflict, choice: string) {
    await call("team_decide", {
      conflictId: c.id,
      currentRevision:
        records.find((r) => r.id === c.recordId)?.data._teamRevision ?? 0,
      choice,
      merged:
        choice === "merge"
          ? { ...c.candidateRecord, data: JSON.parse(mergeText) }
          : null,
    });
    setConflicts(await request("conflicts", { vaultId: vault?.id }));
    setMerging(undefined);
    onVault(await call<Vault>("vault_info"));
  }
  const team = overview?.teams.find((t) => t.id === teamId),
    owner = team?.ownerId === overview?.user.id;
  const records = vault?.records ?? [];
  async function load() {
    const s = await call<Status>("team_status");
    setStatus(s);
    const o = s?.online ? await request<Overview>("overview") : s?.overview;
    if (o) {
      setOverview(o);
      setTeamId((old) =>
        o.teams.some((t) => t.id === old) ? old : (o.teams[0]?.id ?? ""),
      );
    }
  }
  async function run(job: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await job();
      await load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    void load().catch((e) => setError(errorText(e)));
    const timer = setInterval(() => void load().catch(() => {}), 15000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    if (!teamId) return;
    void request<typeof vaults>("vaults", { teamId })
      .then(setVaults)
      .catch((e) => setError(errorText(e)));
  }, [teamId, overview?.teams.find((t) => t.id === teamId)?.aclRevision]);
  async function loadMembers() {
    const data = await request<{ members: Member[]; acl: typeof acl }>(
      "members",
      { teamId },
    );
    setMembers(data.members);
    setAcl(data.acl);
    setMemberId(
      (id) => id || data.members.find((m) => m.role !== "owner")?.userId || "",
    );
  }
  useEffect(() => {
    if (!teamId) return;
    if (tab === "members")
      void loadMembers().catch((e) => setError(errorText(e)));
    if (tab === "audit")
      void request<Audit[]>("audit", { teamId })
        .then(setAudit)
        .catch((e) => setError(errorText(e)));
    if (tab === "decisions" && vault)
      void request<Conflict[]>("conflicts", { vaultId: vault.id })
        .then(setConflicts)
        .catch((e) => setError(errorText(e)));
  }, [tab, teamId, vault?.id]);
  async function setPermission(permission: string, effect: string) {
    const o = await request<Overview>("overview");
    const t = o.teams.find((t) => t.id === teamId);
    await request("acl_set", {
      teamId,
      vaultId: vault?.id,
      userId: memberId,
      recordId: scope || null,
      permission,
      effect,
      aclRevision: t?.aclRevision,
    });
    await loadMembers();
  }
  return (
    <section className="team-workspace">
      <header className="team-header">
        <div>
          <div className="team-kicker">
            <Users size={16} /> TEAM ÇALIŞMA ALANI
          </div>
          <h1>{team?.name ?? "Takımınızı oluşturun"}</h1>
          <p>Kaynaklara göre erişim, karar kuyruğu ve kayıt geçmişi.</p>
        </div>
        <div className="team-header-actions">
          <span
            className={"team-status " + (status?.online ? "online" : "offline")}
          >
            {status?.online ? "PostgreSQL bağlı" : "Çevrimdışı"}
          </span>
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await request("sync");
                if (vault) onVault(await call<Vault>("vault_info"));
              })
            }
          >
            <RefreshCw size={15} /> Eşitle
          </button>
          <button
            onClick={() =>
              void run(async () => {
                await call("team_logout");
                onLogout();
              })
            }
          >
            Çıkış
          </button>
        </div>
      </header>
      <div className="team-selection">
        <label>
          Takım{" "}
          <select value={teamId} onChange={(e) => setTeamId(e.target.value)}>
            {overview?.teams.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name} · {roles[t.role]}
              </option>
            ))}
          </select>
        </label>
        <span>{status?.user.username}</span>
        <span>{status?.pending ?? 0} bekleyen işlem</span>
        {status?.oldestPending && (
          <small>
            En eski: {new Date(status.oldestPending).toLocaleString()}
          </small>
        )}
      </div>
      {!status?.online && (
        <div className="notice">
          <AlertTriangle size={16} />{" "}
          {status?.canEditOffline
            ? "24 saatlik düzenleme süresi içinde."
            : "Ortak kayıt düzenlemesi kapalı; önceden izin verilen bağlantılar kullanılabilir."}{" "}
          Yeni yetki iptalleri bağlantı kurulana kadar bilinemez.
        </div>
      )}
      {error && (
        <div role="alert" className="notice error">
          {error}
        </div>
      )}
      {message && (
        <div role="status" className="notice">
          {message}
        </div>
      )}
      <div className="team-tabs" role="tablist">
        {[
          ["vaults", "Ortak kasalar"],
          ["members", "Üyeler ve yetkiler"],
          ["decisions", "Bekleyen kararlar"],
          ["audit", "İşlem geçmişi"],
          ["policy", "Bağlantı ve politika"],
        ].map(([id, label]) => (
          <button
            key={id}
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? "active" : ""}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="team-content" aria-busy={busy}>
        {tab === "vaults" && (
          <>
            <div className="team-section-title">
              <h2>Ortak kasalar</h2>
              <span>{vaults.length} kasa</span>
            </div>
            <div className="team-vault-grid">
              {vaults.map((v) => (
                <button
                  key={v.id}
                  className={
                    "team-vault-card " + (v.id === vault?.id ? "selected" : "")
                  }
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      onVault(
                        await call<Vault>("team_open_vault", {
                          vaultId: v.id,
                          name: v.name,
                          teamId,
                        }),
                      );
                      setMessage(
                        `${v.name} açıldı. Hosts ve SFTP bölümleri yalnız izinli kayıtları kullanır.`,
                      );
                    })
                  }
                >
                  <FolderKey size={28} />
                  <strong>{v.name}</strong>
                  <span>
                    {v.id === vault?.id ? "Açık ortak kasa" : "Kasayı aç"}
                  </span>
                </button>
              ))}
            </div>
            <div className="team-create">
              <input
                placeholder="Yeni takım / kasa adı"
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
              <button
                disabled={busy || !name.trim()}
                onClick={() =>
                  void run(async () => {
                    const t = await request<{ id: string }>("create_team", {
                      name,
                    });
                    setTeamId(t.id);
                    setName("");
                  })
                }
              >
                <Plus size={16} /> Takım oluştur
              </button>
              {owner && (
                <button
                  disabled={busy || !name.trim()}
                  onClick={() =>
                    void run(async () => {
                      await request("create_vault", { teamId, name });
                      setVaults(await request("vaults", { teamId }));
                      setName("");
                    })
                  }
                >
                  Ortak kasa oluştur
                </button>
              )}
            </div>
            <p className="muted">
              Üyeliğiniz yoksa sahibin sizi eklemesini bekleyebilir veya kendi
              takımınızı oluşturabilirsiniz. Kişisel kasalarınız değişmez.
            </p>
            {vault && (
              <div className="team-history">
                <h2>
                  <History size={18} /> Kayıt sürümleri
                </h2>
                <select
                  value={recordId}
                  onChange={(e) => {
                    setRecordId(e.target.value);
                    setVersions([]);
                    setTemporary(undefined);
                  }}
                >
                  <option value="">Kayıt seçin</option>
                  {records
                    .filter((r) => r.kind !== "group" || !r.data._teamPathOnly)
                    .map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.data.label} · {r.kind}
                      </option>
                    ))}
                </select>
                <button
                  disabled={!recordId || busy}
                  onClick={() =>
                    void run(async () =>
                      setVersions(await call("team_history", { recordId })),
                    )
                  }
                >
                  Geçmişi getir
                </button>
                {recordId&&records.find(r=>r.id===recordId)?.data._teamPermissions?.includes("manage")&&<div className="team-acl"><h3>Klasör taşıma önizlemesi</h3><select aria-label="Hedef klasör" value={moveParent} onChange={e=>{setMoveParent(e.target.value);setMovePreview(undefined);}}><option value="">Kasa kökü</option>{records.filter(r=>r.kind==="group"&&r.id!==recordId).map(r=><option key={r.id} value={r.id}>{r.data.label}</option>)}</select><button disabled={busy} onClick={()=>void run(async()=>setMovePreview(await request('move_preview',{vaultId:vault.id,recordId,parentId:moveParent||null})))}>Erişim değişikliğini önizle</button>{movePreview&&<div><p>{movePreview.after.length} kayıt yeni klasörün izinlerini devralacak.</p>{movePreview.after.map(item=>{const before=movePreview.before.find(b=>b.id===item.id)?.recipients??[];const gained=item.recipients.filter(u=>!before.some(b=>b.userId===u.userId));const lost=before.filter(u=>!item.recipients.some(b=>b.userId===u.userId));return <p key={item.id}>{records.find(r=>r.id===item.id)?.data.label??'Kayıt'}: {gained.length} yeni erişim, {lost.length} kaldırılan erişim</p>;})}<button disabled={busy} onClick={()=>void run(async()=>{onVault(await call<Vault>('team_move',{recordId,parentId:moveParent||null,aclRevision:movePreview.aclRevision,recordRevision:movePreview.recordRevision}));setMovePreview(undefined);setMessage('Kayıt ve kapsam izinleri birlikte taşındı.');})}>Önizlemeyi onayla ve taşı</button></div>}</div>}
                {temporary && (
                  <div className="notice">
                    Geçmiş sürüm kullanılıyor · r{temporary}
                    <button
                      onClick={() =>
                        void run(async () => {
                          await call("team_checkout", {
                            recordId,
                            revision: null,
                            permanent: false,
                          });
                          setTemporary(undefined);
                        })
                      }
                    >
                      Güncele dön
                    </button>
                  </div>
                )}
                {versions.map((ver) => (
                  <div className="team-version" key={ver.revision}>
                    <strong>r{ver.revision}</strong>
                    <span>{new Date(ver.serverAt).toLocaleString()}</span>
                    <span>
                      {ver.record.data.label}
                      {ver.pinned ? " · Sabitlenmiş" : ""}
                      {ver.restoreFrom
                        ? ` · r${ver.restoreFrom} geri yüklendi`
                        : ""}
                    </span>
                    <button
                      disabled={busy}
                      onClick={() =>
                        void run(async () => {
                          await call("team_checkout", {
                            recordId,
                            revision: ver.revision,
                            permanent: false,
                          });
                          setTemporary(ver.revision);
                        })
                      }
                    >
                      Geçici kullan
                    </button>
                    <button
                      disabled={busy || !status?.online}
                      onClick={() =>
                        void run(async () => {
                          await call("team_checkout", {
                            recordId,
                            revision: ver.revision,
                            permanent: true,
                          });
                          setVersions(await call("team_history", { recordId }));
                          setMessage(
                            "Seçilen içerik yeni revizyon olarak kaydedildi.",
                          );
                        })
                      }
                    >
                      Kalıcı geri yükle
                    </button>
                  </div>
                ))}
                <small>
                  Host geçmişi bağlı kimlikleri, jump hostları veya uzak
                  makinedeki dosyaları geri almaz.
                </small>
              </div>
            )}
          </>
        )}
        {tab === "members" && (
          <>
            <h2>Başlangıç yetki matrisi</h2>
            <div className="team-table-scroll">
              <table className="team-matrix">
                <thead>
                  <tr>
                    <th>Rol</th>
                    {Object.values(labels).map((l) => (
                      <th key={l}>{l}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {Object.keys(roles).map((role) => (
                    <tr key={role}>
                      <th>{roles[role]}</th>
                      {Object.keys(labels).map((p) => (
                        <td key={p}>
                          {rolePermissions[role].includes(p) ? "✓" : "—"}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="muted">
              Roller şablondur. Sahip dışındaki üyeler için kaynak izni ayrıca
              verilir. Açık engelleme devralınan izinden önceliklidir.
            </p>
            <div className="team-members">
              {members.map((m) => (
                <button
                  className={memberId === m.userId ? "selected" : ""}
                  key={m.userId}
                  onClick={() => setMemberId(m.userId)}
                >
                  <span>{m.username}</span>
                  <small>
                    {roles[m.role]}
                    {m.active ? "" : " · Erişimi kaldırıldı"}
                  </small>
                </button>
              ))}
            </div>
            {owner && (
              <div className="team-create">
                <input
                  placeholder="Kayıtlı kullanıcı adı veya tam e-posta"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                />
                <button
                  disabled={busy || query.length < 3}
                  onClick={() =>
                    void run(async () =>
                      setFound(
                        await request("search_users", { teamId, query }),
                      ),
                    )
                  }
                >
                  Kullanıcı ara
                </button>
                <select value={role} onChange={(e) => setRole(e.target.value)}>
                  {Object.entries(roles)
                    .filter(([id]) => id !== "owner")
                    .map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))}
                </select>
                {found.map((u) => (
                  <button
                    key={u.id}
                    disabled={busy}
                    onClick={() =>
                      void run(async () => {
                        await request("member_set", {
                          teamId,
                          userId: u.id,
                          role,
                          active: true,
                        });
                        await loadMembers();
                        setFound([]);
                      })
                    }
                  >
                    {u.username} · Takıma ekle
                  </button>
                ))}
              </div>
            )}
            {vault && memberId && (
              <div className="team-acl">
                <h2>Kaynak izinleri</h2>
                <Field label="İznin kapsamı">
                  <select
                    value={scope}
                    onChange={(e) => setScope(e.target.value)}
                  >
                    <option value="">{vault.name} · Tüm kasa</option>
                    {records
                      .filter(
                        (r) =>
                          ["group", "host", "credential"].includes(r.kind) &&
                          !r.data._teamPathOnly,
                      )
                      .map((r) => (
                        <option value={r.id} key={r.id}>
                          {r.kind === "group" ? "Klasör" : "Kayıt"} ·{" "}
                          {r.data.label}
                        </option>
                      ))}
                  </select>
                </Field>
                <div className="team-permissions">
                  {Object.entries(labels).map(([p, label]) => (
                    <label key={p}>
                      <span>{label}</span>
                      <select
                        aria-label={label}
                        disabled={
                          busy || !status?.online || memberId === team?.ownerId
                        }
                        value={
                          acl.find(
                            (a) =>
                              a.user_id === memberId &&
                              (a.resource_id ?? "") === scope &&
                              a.permission === p,
                          )?.effect ?? "inherit"
                        }
                        onChange={(e) =>
                          void run(() => setPermission(p, e.target.value))
                        }
                      >
                        <option value="inherit">Üst kapsamdan devral</option>
                        <option value="allow">İzin ver</option>
                        <option value="deny">Engelle</option>
                      </select>
                    </label>
                  ))}
                </div>
                <button
                  disabled={busy || memberId === team?.ownerId}
                  onClick={() =>
                    void run(async () => {
                      for (const p of rolePermissions[role])
                        await setPermission(p, "allow");
                      setMessage("Şablon seçilen kapsamda uygulandı.");
                    })
                  }
                >
                  {roles[role]} şablonunu kapsamda uygula
                </button>
                {owner && (
                  <div className="team-create">
                    <button
                      disabled={busy || memberId === team?.ownerId}
                      onClick={() =>
                        void run(async () => {
                          await request("delegate", {
                            teamId,
                            vaultId: vault.id,
                            userId: memberId,
                            recordId: scope || null,
                            permissions: [
                              "read",
                              "connect",
                              "edit",
                              "reveal",
                              "export",
                            ],
                          });
                          setMessage(
                            "Yetki verme sınırı kaydedildi; ayrıca Erişim ver iznini açın.",
                          );
                        })
                      }
                    >
                      Bu kapsamda yetki vermesine izin ver
                    </button>
                    <button
                      className="danger"
                      disabled={busy || memberId === team?.ownerId}
                      onClick={() =>
                        void run(async () => {
                          await request("member_set", {
                            teamId,
                            userId: memberId,
                            role:
                              members.find((m) => m.userId === memberId)
                                ?.role ?? "observer",
                            active: false,
                          });
                          await loadMembers();
                        })
                      }
                    >
                      Takım erişimini kaldır
                    </button>
                  </div>
                )}
              </div>
            )}
            {!vault && <p>İzin düzenlemek için önce ortak kasa açın.</p>}
          </>
        )}
        {tab === "decisions" && (
          <>
            <h2>
              Bekleyen kararlar{" "}
              <span className="team-count">{conflicts.length}</span>
            </h2>
            <p>
              Kaydı düzenleme yetkisi olan kişi karar verebilir. Karar sırasında
              kayıt tekrar değişirse yeni karşılaştırma gerekir.
            </p>
            {conflicts.length === 0 && (
              <div className="team-empty">
                <Shield size={32} />
                <h3>Bekleyen karar yok</h3>
                <p>
                  Çakışmalar ve yetkisi kaldırılan üyelerin teslim ettiği
                  değişiklikler burada görünür.
                </p>
              </div>
            )}
            {conflicts.map((c) => (
              <div className="team-conflict" key={c.id}>
                <header>
                  <h3>{c.candidateRecord.data.label ?? "Kayıt değişikliği"}</h3>
                  <span>
                    {c.reason === "permission"
                      ? "Yetki incelemesi"
                      : "Eşzamanlı düzenleme"}
                  </span>
                </header>
                <div className="team-compare">
                  <div>
                    <strong>Sunucudaki kayıt</strong>
                    <pre>
                      {JSON.stringify(
                        records.find((r) => r.id === c.recordId)?.data ?? {},
                        null,
                        2,
                      )}
                    </pre>
                  </div>
                  <div>
                    <strong>Gönderilen değişiklik</strong>
                    <pre>{JSON.stringify(c.candidateRecord.data, null, 2)}</pre>
                  </div>
                </div>
                <button
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      await decide(c,"server");
                    })
                  }
                >
                  Sunucudaki kalsın
                </button>
                <button
                  disabled={busy}
                  onClick={() => void run(()=>decide(c,"candidate"))}
                >
                  Gönderilen değişiklik kalsın
                </button>
                <button disabled={busy} onClick={()=>{setMerging(c.id);setMergeText(JSON.stringify(c.candidateRecord.data,null,2));}}>Alanları birleştir</button>
                {merging===c.id&&<div><label>Korunacak alanları düzenleyin<textarea aria-label="Birleştirilmiş kayıt" rows={10} value={mergeText} onChange={e=>setMergeText(e.target.value)}/></label><button disabled={busy} onClick={()=>void run(()=>decide(c,"merge"))}>Birleştir ve kaydet</button></div>}
              </div>
            ))}
          </>
        )}
        {tab === "audit" && (
          <>
            <h2>İşlem geçmişi</h2>
            <p>
              Son 90 gün · Terminal çıktısı, tuşlar ve dosya içerikleri
              kaydedilmez.
            </p>
            <table className="team-audit">
              <thead>
                <tr>
                  <th>Sunucu zamanı</th>
                  <th>İşlem</th>
                  <th>Kayıt</th>
                </tr>
              </thead>
              <tbody>
                {audit.map((a) => (
                  <tr key={a.id}>
                    <td>{new Date(a.server_at).toLocaleString()}</td>
                    <td>{a.action}</td>
                    <td>
                      {records.find((r) => r.id === a.record_id)?.data.label ??
                        a.record_id ??
                        "Takım"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        )}
        {tab === "policy" && (
          <>
            <h2>Bağlantı ve politika</h2>
            <div className="team-policy-grid">
              <article>
                <Database />
                <h3>PostgreSQL zorunlu</h3>
                <p>
                  Hesap, üyelik ve erişim kararlarının kaynağı PostgreSQL'dir.
                  Yerel dosya, izinli kayıtların şifreli önbelleğidir.
                </p>
              </article>
              <article>
                <History />
                <h3>24 saat düzenleme</h3>
                <p>
                  Son başarılı yetki doğrulamasından 24 saat sonra ortak kaydı
                  değiştirme durur. İzinli ve önceden indirilmiş sistemlere
                  bağlantı devam eder.
                </p>
              </article>
              <article>
                <Shield />
                <h3>Güncel + önceki 10 sürüm</h3>
                <p>
                  Geçici kullanılan sürümler ve geri yükleme dönüş noktaları
                  budamadan korunur. Büyük terminal veya SFTP içerikleri geçmişe
                  yazılmaz.
                </p>
              </article>
            </div>
            <dl className="team-policy">
              <dt>Oturum</dt>
              <dd>
                {status?.online
                  ? "Çevrimiçi · TLS sunucu doğrulaması"
                  : "Çevrimdışı"}
              </dd>
              <dt>Son yetki doğrulaması</dt>
              <dd>
                {status?.verifiedAt
                  ? new Date(status.verifiedAt).toLocaleString()
                  : "Henüz yapılmadı"}
              </dd>
              <dt>Bekleyen değişiklikler</dt>
              <dd>{status?.pending ?? 0}</dd>
              <dt>Şifreli kayıt geçmişi boyutu</dt>
              <dd>
                {bytes === undefined
                  ? "Henüz ölçülmedi"
                  : `${(bytes / 1024).toFixed(1)} KiB`}
              </dd>
            </dl>
            {owner && (
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const result = await request<{ encryptedBytes: number }>(
                      "maintenance",
                      { teamId },
                    );
                    setBytes(result.encryptedBytes);
                  })
                }
              >
                Alan kullanımını ölç / süresi dolmuş günlüğü temizle
              </button>
            )}
            <div className="notice">
              Bir cihaza daha önce ulaşmış SSH sırrını uzaktan geri almak mümkün
              değildir. Sır gösterme / dışa aktarma kısıtı uygulama içinde
              uygulanır; uzak sunucunun SSH ve dosya izinlerinin yerini almaz.
            </div>
          </>
        )}
        {busy && <Busy label="Team işlemi yürütülüyor…" />}
      </div>
    </section>
  );
}
