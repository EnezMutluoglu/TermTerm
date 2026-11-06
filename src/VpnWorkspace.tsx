import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  FileUp,
  Folder,
  Globe2,
  Info,
  Plus,
  Search,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { call, chooseFile, desktop, errorText, saveFile } from "./api";
import { Field, Modal, Select } from "./components";
import ContextMenu, { menuPosition, type MenuPosition } from "./ContextMenu";
import TeamShare from "./TeamShare";
import VpnPreview from "./VpnPreview";
import {
  mergeVpnSession,
  mergeVpnSnapshot,
  vpnActive,
  vpnPermitted,
  type VpnAuthentication,
  type VpnFamily,
  type VpnProfile,
  type VpnSession,
} from "./vpnState";
import "./vpn.css";

const diagnostics = import.meta.env.DEV && new URLSearchParams(location.search).has("vpnDiagnostics");

const families = [
  { value: "openVpn", label: "OpenVPN" },
  { value: "wireGuard", label: "WireGuard" },
  { value: "fortinet", label: "Fortinet" },
  { value: "globalProtect", label: "GlobalProtect" },
];
const familyLabel = (family: string) =>
  families.find((item) => item.value === family)?.label || family;
type Status = {
  available: boolean;
  platform: string;
  reason: string;
  labOnly: boolean;
  maxConnections: number;
  authTestAvailable?: boolean;
};
type Draft = VpnProfile & { password?: string; importToken?: string };
type ImportResult = {token: string; family: VpnFamily; warnings: string[]; missingFiles: string[]; blocked: boolean; authentication?: VpnAuthentication; diagnostics?: {sha256: string; byteLength: number}};
function authenticationText(auth: VpnAuthentication) {
  const mode = auth.usernamePassword ? (auth.certificate ? "Sertifika/anahtar ve kullanıcı adı/parola ile giriş." : "Kullanıcı adı/parola ile giriş.")
    : auth.certificate ? (auth.privateKeyPassword ? "Sertifika/anahtar ile giriş." : "Sertifika/anahtar ile parolasız giriş.") : "Kimlik doğrulama bağlantıda belirlenir.";
  return [mode, auth.privateKeyPassword && "Özel anahtar parolası bağlantıda sorulur ve kaydedilmez.", auth.staticChallenge && "Statik OTP kodu bağlantıda sorulur ve kaydedilmez."].filter(Boolean).join(" ");
}
const newProfile = (): Draft => ({
  id: crypto.randomUUID(),
  version: 1,
  label: "",
  family: "fortinet",
  source: "manual",
  server: "",
  groupId: "Firma",
  username: "",
  hasPassword: false,
  hasConfig: false,
  realm: "",
  remember: true,
});
const stateLabel: Record<VpnSession["state"], string> = {
  connecting: "Bağlanıyor",
  challenge: "Kimlik doğrulama bekliyor",
  connected: "Bağlı",
  disconnecting: "Kapatılıyor",
  disconnected: "Bağlı değil",
  failed: "Bağlantı başarısız",
};
function sessionLabel(session: VpnSession) {
  if (session.authenticationVerified) return "Giriş doğrulandı · tünel açılmadı";
  if (session.state === "challenge") {
    if (session.challenge?.fields.some(field => field.name === "gateway" && field.purpose === "choice")) return "Portal girişi tamamlandı · gateway seçimi bekleniyor";
    if (session.challenge?.stage === "portal") return "Portal SSO girişi bekleniyor";
    if (session.challenge?.stage === "gateway") return "Gateway SSO girişi bekleniyor";
  }
  if (session.authOnly && session.state === "connecting") return `${session.authStage === "gateway" ? "Gateway girişi" : session.authStage === "gatewaySelection" ? "Gateway seçimi" : session.authStage === "portal" ? "Portal girişi" : "Giriş"} doğrulanıyor · tünel açılmayacak`;
  return stateLabel[session.state];
}

type TeamSnapshot={context:string;profiles:VpnProfile[];offline?:boolean;journal?:{pending:number;conflicts:number;quarantined:number}};
type TeamScope = {id:string;name:string};
export default function VpnWorkspace({ vaultId, onOpenAuthenticator, team=false, scopes=[] }: { vaultId: string; onOpenAuthenticator?: () => void; team?:boolean; scopes?:TeamScope[] }) {
  const [demo, setDemo] = useState(
    import.meta.env.DEV && new URLSearchParams(location.search).has("previewVpn"),
  );
  return demo ? (
    <>
      <button
        className="secondary vpn-mode-switch"
        onClick={() => setDemo(false)}
      >
        Masaüstü VPN paneline dön
      </button>
      <VpnPreview />
    </>
  ) : (
    <VpnPanel key={vaultId} vaultId={vaultId} team={team} scopes={scopes} onDemo={() => setDemo(true)} onOpenAuthenticator={onOpenAuthenticator} />
  );
}

function VpnPanel({
  onDemo,
  vaultId,
  onOpenAuthenticator,
  team, scopes,
}: {
  team: boolean; scopes: TeamScope[];
  onDemo: () => void;
  vaultId: string;
  onOpenAuthenticator?: () => void;
}) {
  const scope = useRef(vaultId);
  const [context,setContext] = useState<string>();
  const [teamState,setTeamState]=useState<TeamSnapshot>();
  const [sharing,setSharing] = useState<VpnProfile>();
  const [credentials,setCredentials] = useState<VpnProfile>();
  const [login,setLogin] = useState({username:"",password:"",remember:true});
  const [revealed,setRevealed] = useState<string>();
  const teamRpc = (action:Record<string,unknown>) => call<any>("team_v2_action",{action:{kind:"vpn",context:scope.current,action}});
  async function reloadTeam() {
    const snapshot=await call<TeamSnapshot>("team_v2_action",{action:{kind:"vpn",action:{kind:"list"}}});
    if(alive.current && snapshot.context===scope.current) {setProfiles(snapshot.profiles);setTeamState(snapshot);}
  }
  const [profiles, setProfiles] = useState<VpnProfile[]>([]);
  const [sessions, setSessions] = useState<VpnSession[]>([]);
  const [status, setStatus] = useState<Status>();
  const [selected, setSelected] = useState("");
  const [search, setSearch] = useState("");
  const [folder, setFolder] = useState("");
  const [draft, setDraft] = useState<Draft>();
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState<string[]>([]);
  const operations = useRef(new Set<string>());
  const alive = useRef(true);
  const importGeneration = useRef(0);
  const dropped = useRef<(paths: string[]) => void>(() => {});
  useEffect(() => {
    if (!desktop) return;
    const unlisten = listen<{paths: string[]; position: {x: number; y: number}}>("tauri://drag-drop", ({payload}) => {
      const element = document.elementFromPoint(payload.position.x / devicePixelRatio, payload.position.y / devicePixelRatio);
      if (element?.closest(".vpn-workspace, .vpn-drop")) dropped.current(payload.paths);
    });
    return () => { void unlisten.then(stop => stop()); };
  }, []);
  const [menu, setMenu] = useState<{
    profile: VpnProfile;
    position: MenuPosition;
  }>();
  const [auth, setAuth] = useState<{
    id: string;
    generation: number;
    challengeId: number;
  }>();
  const [values, setValues] = useState<Record<string, string>>({});
  const apply = (session: VpnSession) => {
    if (alive.current && session.vaultId === scope.current)
      setSessions((all) => mergeVpnSession(all, session));
  };
  const applySnapshot = (session: VpnSession) => {
    if (alive.current && session.vaultId === scope.current) setSessions(all => mergeVpnSnapshot(all,session));
  };
  useEffect(() => {
    alive.current = true;
    let unlisten: (() => void) | undefined;
    let current = true;
    (async () => {
      try {
        let teamProfiles:VpnProfile[]|undefined;
        if(team) {
          const snapshot=await call<TeamSnapshot>("team_v2_action",{action:{kind:"vpn",action:{kind:"list"}}});
          if(!current)return;
          scope.current=snapshot.context;setContext(snapshot.context);setTeamState(snapshot);teamProfiles=snapshot.profiles;
        }
        if (desktop) {
          const stop = await listen<VpnSession>("vpn-event", ({ payload }) => {
            if (current) apply(payload);
          });
          if (!current) {
            stop();
            return;
          }
          unlisten = stop;
        }
        void call<Status>("vpn_status").then((availability) => {
          if (current) setStatus(availability);
        }).catch((e) => { if (current) setError(errorText(e)); });
        const [saved, running] = await Promise.all([
          teamProfiles ? Promise.resolve(teamProfiles) : call<VpnProfile[]>("vpn_profiles"),
          call<VpnSession[]>("vpn_sessions"),
        ]);
        if (!current) return;
        setProfiles(saved);
        setSelected(saved[0]?.id || "");
        setSessions((all) =>
          running
            .filter((session) => session.vaultId === scope.current)
            .reduce(mergeVpnSnapshot, all),
        );
      } catch (e) {
        if (current) setError(errorText(e));
      }
    })();
    return () => {
      current = false;
      alive.current = false;
      unlisten?.();
    };
  }, []);
  useEffect(()=>{if(!team || !context)return;const timer=setInterval(()=>{if(!document.hidden && operations.current.size===0)void reloadTeam().catch(e=>{if(alive.current)setError(errorText(e));});},15000);return()=>clearInterval(timer);},[team,context]);
  const challenge = sessions.find(
    (session) =>
      session.id === auth?.id &&
      session.generation === auth.generation &&
      session.state === "challenge" &&
      session.challenge?.id === auth.challengeId,
  );
  useEffect(() => {
    if (auth && !challenge) {
      setAuth(undefined);
      setValues({});
    }
  }, [auth, challenge]);
  async function perform(key: string, work: () => Promise<void>) {
    if (operations.current.has(key)) return;
    operations.current.add(key);
    setBusy([...operations.current]);
    setError("");
    setNotice("");
    try {
      await work();
    } catch (e) {
      if (alive.current) setError(errorText(e));
    } finally {
      operations.current.delete(key);
      if (alive.current) setBusy([...operations.current]);
    }
  }
  function edit(profile: VpnProfile) {
    if(!vpnPermitted(profile,"edit"))return;
    importGeneration.current++;
    setDraft({
      id: profile.id,
      revision:profile.revision, visibility:profile.visibility, permissions:profile.permissions,
      version: profile.version,
      label: profile.label,
      family: profile.family,
      source: profile.source,
      server: profile.server,
      groupId: profile.groupId,
      username: profile.username,
      realm: profile.realm,
      remember: profile.remember,
      authentication: profile.authentication,
      hasPassword: profile.hasPassword,
      hasConfig: profile.hasConfig,
    });
    setError("");
  }
  async function save() {
    if (!draft) return;
    const submitted = draft;
    await perform("profile", async () => {
      if (!submitted.label.trim()) throw new Error("Profil adı gerekli.");
      if (
        ["openVpn", "wireGuard"].includes(submitted.family) &&
        !submitted.hasConfig &&
        !submitted.importToken
      )
        throw new Error("Bir .ovpn veya .conf dosyası içe aktarın.");
      const {
        importToken,
        hasPassword: _password, authentication: _authentication,
        hasConfig: _config,
        pending: _pending, revision, visibility: _visibility, permissions: _permissions, canUpdate: _update, canRemove: _remove, scopeId: _scope,
        ...profile
      } = submitted;
      if (submitted.family === "openVpn" && submitted.authentication && (!submitted.authentication.usernamePassword || submitted.authentication.staticChallenge)) delete profile.password;
      const saved = await call<VpnProfile>("vpn_profile_save", {
        profile: { ...profile, label: profile.label.trim() },
        importToken, teamContext: team ? context : undefined, revision:revision ?? 0,
      });
      if (!alive.current) return;
      setProfiles((all) => [
        ...all.filter((item) => item.id !== saved.id),
        saved,
      ]);
      setSelected(saved.id);
      setDraft(undefined);
      setNotice(team && saved.pending ? "VPN değişikliği şifreli yerel günlüğe kaydedildi; eşitleme bekliyor." : "VPN profili şifreli kasaya kaydedildi.");
      if(team)await reloadTeam();
    });
  }
  async function importFile(paths?: string[]) {
    if (operations.current.has("import")) return;
    const request = ++importGeneration.current;
    await perform("import", async () => {
      if (paths && paths.length !== 1) throw new Error("Tek bir VPN dosyası bırakın.");
      const path = paths?.[0] ?? await chooseFile(["ovpn", "conf"], "VPN yapılandırmasını içe aktar");
      if (!path) return;
      if (!/\.(ovpn|conf)$/i.test(path)) throw new Error("Bir .ovpn veya .conf dosyası seçin.");
      const source = path.split(/[\\/]/).pop() || "import";
      const result = await call<ImportResult>("vpn_import", {path,teamContext:team?context:undefined});
      if (!alive.current || request !== importGeneration.current) return;
      if (result.blocked || result.missingFiles.length) throw new Error([
        `${source}: Dosya içe aktarılamadı.`, ...result.warnings, ...result.missingFiles,
        ...(result.diagnostics ? [`Tanı: SHA-256 ${result.diagnostics.sha256}; ${result.diagnostics.byteLength} bayt.`] : []),
      ].join(" "));
      const family = result.family || "openVpn";
      setDraft(previous => ({...(previous || newProfile()), family, source, importToken: result.token, hasConfig: true, authentication: result.authentication}));
      setNotice([...result.warnings, ...(family === "wireGuard" ? ["WireGuard bağlantısı doğrulanmış IPv4 motorunu gerektirir; IPv6 tüneli bu geliştirme sürümünde desteklenmez."] : [])].join(" ") || "Yapılandırma güvenli biçimde hazırlandı. Profili kaydedin.");
    });
  }
  dropped.current = paths => { void importFile(paths); };
  async function connect(profile: VpnProfile) {
    await perform(profile.id, async () => {
      const session = await call<VpnSession>("vpn_connect", {
        profileId: profile.id,
        teamContext:team?context:undefined,
      });
      applySnapshot(session);
    });
  }
  async function disconnect(session: VpnSession) {
    await perform(session.profileId, async () => {
      await call("vpn_disconnect", {
        id: session.id,
        generation: session.generation,
      });
      apply({ ...session, state: "disconnected", challenge: undefined });
    });
  }
  async function remove(profile: VpnProfile) {
    await perform(profile.id, async () => {
      await call("vpn_profile_delete", { id: profile.id, teamContext:team?context:undefined, revision:profile.revision??0 });
      if (alive.current) {
        setProfiles((all) => all.filter((item) => item.id !== profile.id));
        setSelected("");
      }
    });
  }
  const activeCount =
    sessions.filter(vpnActive).length +
    busy.filter(
      (key) =>
        profiles.some((profile) => profile.id === key) &&
        !vpnActive(sessions.find((session) => session.profileId === key)),
    ).length;
  const current = profiles.find((profile) => profile.id === selected);
  const visible = profiles.filter(
    (profile) =>
      (!folder || profile.groupId === folder) &&
      `${profile.label} ${familyLabel(profile.family)} ${profile.server}`
        .toLocaleLowerCase("tr")
        .includes(search.toLocaleLowerCase("tr")),
  );
  return (
    <section className="vpn-workspace" aria-label="VPN masaüstü çalışma alanı">
      <header className="vpn-heading">
        <div className="vpn-heading-title">
          <span className="vpn-emblem">
            <ShieldCheck size={25} />
          </span>
          <div>
            <h1>VPN</h1>
            <p>Bağımsız bağlantılar ve şifreli profiller.</p>
          </div>
        </div>
        <div className="vpn-actions">
          {diagnostics && <button className="secondary" onClick={onDemo}>
            Arayüz demosu
          </button>}
          <button
            className="primary"
            disabled={!desktop || busy.includes("profile")}
            onClick={() => edit(newProfile())}
          >
            <Plus size={16} /> Yeni VPN
          </button>
        </div>
      </header>
      <div className="vpn-preview-note">
        <Info size={17} />
        <span>
          {status
            ? `${status.platform}: ${status.reason || (status.available ? "VPN bağlantıları hazır." : "VPN hizmeti hazır değil.")}`
            : "Masaüstü hizmeti kontrol ediliyor."}{" "}
          {status?.available && status.platform.toLowerCase() === "linux" && status.labOnly &&
            "Bağlantılar yalnız yalıtılmış Linux laboratuvarında kullanılabilir."}
        </span>
      </div>
      {error && (
        <p className="vpn-error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="vpn-notice" role="status">
          {notice}
        </p>
      )}
      {team && teamState?.offline && <p className="vpn-notice" role="status">Çevrimdışı VPN önbelleği açık. Daha önce doğrulanan bağlantılar kullanılabilir; düzenleme yetkisi son doğrulamadan itibaren 24 saattir.</p>}
      {team && !!teamState?.journal && (teamState.journal.pending+teamState.journal.conflicts+teamState.journal.quarantined)>0 && <p className="vpn-notice" role="status">VPN günlüğü: {teamState.journal.pending} bekleyen · {teamState.journal.conflicts} çakışan · {teamState.journal.quarantined} yerel karantinada. Çakışan ve yetkisi kaldırılan taslaklar şifreli olarak korunur; bu sürümde otomatik yönetici teslimi desteklenmez.</p>}
      <div className="vpn-overview">
        <div>
          <span>Aktif bağlantı</span>
          <strong>
            {sessions.filter((session) => session.state === "connected").length}
            <small>/ en fazla {status?.maxConnections || 3}</small>
          </strong>
        </div>
        <div>
          <span>Şifreli profil</span>
          <strong>
            {profiles.length}
            <small>OpenVPN · WireGuard · Fortinet · GlobalProtect</small>
          </strong>
        </div>
        <div>
          <span>Kimlik doğrulama</span>
          <strong>
            {sessions.filter((session) => session.state === "challenge").length}
            <small>OTP hiçbir zaman kaydedilmez</small>
          </strong>
        </div>
      </div>
      <div className="vpn-body">
        <aside className="vpn-folders">
          <span className="vpn-eyebrow">VPN KLASÖRLERİ</span>
          {[
            "",
            ...new Set(
              profiles.map((profile) => profile.groupId || "").filter(Boolean),
            ),
          ].map((name) => (
            <button
              key={name}
              className={folder === name ? "active" : ""}
              onClick={() => setFolder(name)}
            >
              <Folder size={16} />
              <span>{name || "Tüm profiller"}</span>
            </button>
          ))}
        </aside>
        <div className="vpn-profiles">
          <label className="vpn-search">
            <Search size={17} />
            <input
              aria-label="VPN ara"
              placeholder="İsim, VPN türü veya sunucu ara…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </label>
          <div className="vpn-list-caption">
            <span>{visible.length} profil</span>
            <span>En fazla üç bağımsız bağlantı</span>
          </div>
          <div className="vpn-list">
            {visible.map((profile) => {
              const session = sessions.find(
                (item) => item.profileId === profile.id,
              );
              const active = vpnActive(session);
              return (
                <article
                  key={profile.id}
                  className={
                    "vpn-card " + (selected === profile.id ? "selected" : "")
                  }
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setMenu({ profile, position: menuPosition(e) });
                  }}
                >
                  <button
                    className="vpn-card-summary"
                    onClick={() => setSelected(profile.id)}
                  >
                    <span className="vpn-provider-icon">
                      <Globe2 size={21} />
                    </span>
                    <span className="vpn-card-text">
                      <strong>{profile.label}</strong>
                      <span>
                        {familyLabel(profile.family)} ·{" "}
                        {profile.groupId || "Kişisel"}
                      </span>
                      <small>{profile.server || profile.source}</small>
                    </span>
                  </button>
                  <div className="vpn-card-bottom">
                    <span
                      className={
                        session?.state === "connected"
                          ? "vpn-connected"
                          : "vpn-disconnected"
                      }
                      role="status"
                    >
                      {session ? sessionLabel(session) : "Bağlı değil"}
                    </span>
                    {session?.state === "challenge" && session.challenge && (
                      <button
                        onClick={() => {
                          setValues(
                            Object.fromEntries(
                              session
                                .challenge!.fields.filter(
                                  (field) =>
                                    field.value &&
                                    !field.secret &&
                                    ["username", "choice"].includes(field.purpose),
                                )
                                .map((field) => [field.name, field.value!]),
                            ),
                          );
                          setAuth({
                            id: session.id,
                            generation: session.generation,
                            challengeId: session.challenge!.id,
                          });
                        }}
                      >
                        Yanıtla
                      </button>
                    )}
                    <button
                      disabled={
                        busy.includes(profile.id) || !vpnPermitted(profile,"connect") ||
                        (active
                          ? session?.state === "disconnecting"
                          : !status?.available ||
                            activeCount >= (status?.maxConnections || 3))
                      }
                      onClick={() => {
                        if (active && session) void disconnect(session);
                        else void connect(profile);
                      }}
                    >
                      {busy.includes(profile.id)
                        ? "İşlem sürüyor…"
                        : active
                          ? "Bağlantıyı kes"
                          : "Bağlan"}
                    </button>
                    {diagnostics && status?.authTestAvailable && profile.family === "globalProtect" && !active && <button type="button" className="secondary" disabled={busy.includes(profile.id)} onClick={() => void perform(profile.id, async () => { applySnapshot(await call<VpnSession>("vpn_sso_test_start",{profileId:profile.id})); })}>SSO girişini test et (tünelsiz)</button>}
                  </div>

                  {session?.error && (
                    <p className="vpn-session-error" role="alert">
                      {session.error === "cleanup_pending: windows_interface_unregister_pending"
                        ? "Windows VPN bağdaştırıcısı kaldırılıyor; temizlik doğrulanana kadar lütfen bekleyin."
                        : session.error}
                    </p>
                  )}
                  {team && <div className="vpn-card-tools">
                    <small>{profile.visibility==="shared"?"Ortak VPN":"Özel VPN"}</small>
                    {vpnPermitted(profile,"connect") && <button onClick={()=>{setCredentials(profile);setLogin({username:profile.username,password:"",remember:profile.remember});}}>Kişisel giriş</button>}
                    {(profile.visibility!=="shared" || profile.canUpdate) && <button disabled={teamState?.offline || profile.pending} onClick={()=>setSharing(profile)}>{profile.visibility==="shared"?"Paylaşımı yönet":"Bağımsız kopya paylaş"}</button>}
                    {vpnPermitted(profile,"reveal") && <button disabled={teamState?.offline} onClick={()=>void perform(profile.id,async()=>{const value=await teamRpc({kind:"reveal",id:profile.id});if(alive.current)setRevealed(value.configuration);})}>Yapılandırmayı gör</button>}
                    {vpnPermitted(profile,"export") && <button disabled={teamState?.offline} onClick={()=>void perform(profile.id,async()=>{const path=await saveFile("json","vpn.json");if(path)await teamRpc({kind:"export",id:profile.id,path});})}>Dışa aktar</button>}
                  </div>}
                  <div className="vpn-card-tools">
                    <button
                      disabled={active || busy.includes(profile.id) || !vpnPermitted(profile,"edit")}
                      onClick={() => edit(profile)}
                    >
                      Düzenle
                    </button>
                    <button
                      disabled={active || busy.includes(profile.id) || (team && !profile.canRemove)}
                      onClick={() => void remove(profile)}
                      aria-label={`${profile.label} profilini sil`}
                    >
                      <Trash2 size={14} /> Sil
                    </button>
                  </div>
                </article>
              );
            })}
            {!visible.length && (
              <p className="vpn-empty">
                Kayıtlı VPN profili yok. Yeni VPN ile başlayın.
              </p>
            )}
          </div>
          <section className="vpn-parallel">
            <strong>Aynı anda 2–3 VPN</strong>
            <p>
              Her bağlantının kimlik doğrulaması ve kapatılması bağımsızdır.
              Rota veya DNS çakışması yeni bağlantıyı engeller. Platform desteği
              hazır olmadan tünel açılmaz.
            </p>
          </section>
        </div>
        <aside className="vpn-details">
          {current && (
            <>
              <span className="vpn-eyebrow">PROFİL BİLGİLERİ</span>
              <h2>{current.label}</h2>
              <span className="vpn-pill">{familyLabel(current.family)}</span>
              <dl>
                <dt>Sunucu / dosya</dt>
                <dd>{current.server || current.source}</dd>
                <dt>Kullanıcı</dt>
                <dd>{current.authentication?.certificate && !current.authentication.usernamePassword ? "Gerekli değil" : current.username || "Bağlantıda sorulacak"}</dd>
                {current.authentication && <><dt>Kimlik doğrulama</dt><dd>{authenticationText(current.authentication)}</dd></>}
                <dt>Realm / grup</dt>
                <dd>{current.realm || "Sunucunun varsayılanı"}</dd>
                <dt>Parola</dt>
                <dd>
                  {current.hasPassword
                    ? "Şifreli kasada hatırlanıyor"
                    : "Kaydedilmedi"}
                </dd>
                <dt>OTP</dt>
                <dd>Yalnız bağlantıda sorulur, kaydedilmez</dd>
              </dl>
            </>
          )}
        </aside>
      </div>
      {menu && (
        <ContextMenu
          position={menu.position}
          title={menu.profile.label}
          onClose={() => setMenu(undefined)}
          onError={(e) => setError(errorText(e))}
          actions={[
            {
              id: "edit",
              label: "Profili düzenle",
              disabled: !vpnPermitted(menu.profile,"edit") || vpnActive(
                sessions.find(
                  (session) => session.profileId === menu.profile.id,
                ),
              ),
              run: () => edit(menu.profile),
            },
            {
              id: "delete",
              label: "Profili sil",
              danger: true,
              disabled: (team && !menu.profile.canRemove) || vpnActive(
                sessions.find(
                  (session) => session.profileId === menu.profile.id,
                ),
              ),
              run: () => remove(menu.profile),
            },
          ]}
        />
      )}
      {sharing && <TeamShare source={{...sharing,kind:"vpn"}} scopes={scopes} privateSources={profiles.filter(p=>p.visibility!=="shared").map(p=>({id:p.id,label:p.label}))}
        rpc={action=>call("team_v2_action",{action:{kind:"vpnShare",context,action}})} onClose={()=>setSharing(undefined)} onSaved={()=>{setSharing(undefined);void reloadTeam();}}/>}
      {revealed!==undefined && <Modal title="VPN yapılandırması" onClose={()=>setRevealed(undefined)}><textarea readOnly value={revealed} aria-label="VPN yapılandırması"/><p>Kişisel giriş bilgileri bu yapılandırmada bulunmaz.</p></Modal>}
      {credentials && <Modal title="Kişisel VPN girişi" onClose={()=>{setCredentials(undefined);setLogin({username:"",password:"",remember:false});}}>
        <p>Bu bilgiler yalnız hesabına aittir; VPN paylaşımıyla aktarılmaz. OTP kaydedilmez.</p>
        <Field label="Kullanıcı adı" value={login.username} onChange={username=>setLogin({...login,username})}/>
        <label><input type="checkbox" checked={login.remember} onChange={e=>setLogin({...login,remember:e.target.checked,password:""})}/>Parolayı hatırla</label>
        {login.remember && <Field label={credentials.hasPassword?"Yeni parola (boş bırakılırsa korunur)":"Parola"} type="password" value={login.password} onChange={password=>setLogin({...login,password})}/>}
        <button disabled={busy.includes("credentials")} onClick={()=>void perform("credentials",async()=>{await teamRpc({kind:"authenticate",id:credentials.id,authentication:login});if(alive.current){setCredentials(undefined);setLogin({username:"",password:"",remember:false});await reloadTeam();}})}>Kaydet</button>
      </Modal>}
      {draft && (
        <Modal
          title="VPN profilini kaydet"
          onClose={() => {
            if (!busy.includes("profile")) {
              importGeneration.current++;
              setDraft(undefined);
              setNotice("");
            }
          }}
          wide
        >
          <form
            className="vpn-form"
            onSubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <p className="vpn-form-intro">
              Profil şifreli kasada tutulur.
            </p>
            <fieldset
              disabled={busy.includes("profile") || busy.includes("import")}
            >
              <Select
                label="VPN türü"
                value={draft.family}
                onChange={(family) =>
                  setDraft({
                    ...draft,
                    family: family as VpnFamily,
                    importToken: undefined,
                    hasConfig: false,
                    authentication: undefined,
                  })
                }
                options={families}
              />
              <div className="vpn-form-grid">
                <Field
                  label="Profil adı"
                  value={draft.label}
                  onChange={(label) => setDraft({ ...draft, label })}
                  required
                />
                <Field
                  label="Klasör"
                  value={draft.groupId || ""}
                  onChange={(groupId) =>
                    setDraft({ ...draft, groupId: groupId || null })
                  }
                />
              </div>
              {["openVpn", "wireGuard"].includes(draft.family) ? (
                <div className="vpn-drop">
                  <FileUp size={24} />
                  <strong>
                    {draft.hasConfig
                      ? draft.source
                      : "VPN dosyasını buraya sürükleyin veya seçin"}
                  </strong>
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => void importFile()}
                  >
                    .ovpn / .conf içe aktar
                  </button>
                  <span>İçerik ve özel anahtarlar arayüze gönderilmez.</span>
                </div>
              ) : (
                <Field
                  label={
                    draft.family === "globalProtect"
                      ? "Portal / gateway adresi"
                      : "VPN sunucusu"
                  }
                  value={draft.server}
                  onChange={(server) => setDraft({ ...draft, server })}
                  placeholder="https://vpn.example"
                  required
                />
              )}
              {draft.family === "openVpn" && draft.authentication && <p className="vpn-file-note">{authenticationText(draft.authentication)}</p>}
              {(draft.family !== "openVpn" || !draft.authentication?.certificate || draft.authentication.usernamePassword) && <>
              <div className="vpn-form-grid">
                <Field
                  label="Kullanıcı adı"
                  value={draft.username}
                  onChange={(username) => setDraft({ ...draft, username })}
                />
                <Field
                  label="Realm / grup"
                  value={draft.realm}
                  onChange={(realm) => setDraft({ ...draft, realm })}
                />
              </div>
              {draft.family === "openVpn" && draft.authentication?.staticChallenge ? <p className="vpn-file-note">Bu statik OTP biçiminde kullanıcı adı/parola her bağlantıda girilir; bu formun yanıtlarından hesap parolası kaydedilmez veya otomatik doldurulmaz.</p> : <>
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.remember}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      remember: e.target.checked,
                      password: e.target.checked ? undefined : "",
                    })
                  }
                />{" "}
                Parolayı şifreli kasada hatırla
              </label>
              {draft.remember && (
                <Field
                  label={
                    draft.hasPassword
                      ? "Yeni parola (boş bırakılırsa mevcut korunur)"
                      : "Parola"
                  }
                  type="password"
                  value={draft.password || ""}
                  onChange={(password) =>
                    setDraft({ ...draft, password: password || undefined })
                  }
                />
              )}
              </>}
              </>}
              <p className="vpn-file-note">
                OTP ve sunucu challenge yanıtları kaydedilmez.
              </p>
            </fieldset>
            {error && (
              <p role="alert" className="vpn-error">
                {error}
              </p>
            )}
            {notice && <p role="status">{notice}</p>}
            <footer>
              <button
                type="button"
                className="secondary"
                disabled={busy.includes("profile") || busy.includes("import")}
                onClick={() => {
                  importGeneration.current++;
                  setDraft(undefined);
                  setNotice("");
                }}
              >
                Vazgeç
              </button>
              <button
                className="primary"
                disabled={busy.includes("profile") || busy.includes("import")}
                type="submit"
              >
                {busy.includes("profile") ? "Kaydediliyor…" : "Kaydet"}
              </button>
            </footer>
          </form>
        </Modal>
      )}
      {challenge?.challenge && (
        <Modal
          title="VPN kimlik doğrulama"
          wide={challenge.challenge.kind === "sso"}
          dismissOnBackdrop={challenge.challenge.kind !== "sso"}
          onClose={() => {
            setValues({});
            setAuth(undefined);
            void disconnect(challenge);
          }}
        >
          {challenge.challenge.kind === "sso" ? (
            <div className="vpn-form">
              <p>{challenge.challenge.stage === "gateway" ? "Seçilen gateway için Microsoft / kurumsal hesabınızla oturum açın." : challenge.challenge.stage === "portal" ? "Portal için Microsoft / kurumsal hesabınızla oturum açın." : "Microsoft / kurumsal hesabınızla bu panelde oturum açın."}</p>
              {challenge.authOnly && <p>Bu giriş testi tünel açmaz; VPN bağlantısı kurulmayacak.</p>}
              <SsoPane session={challenge} onError={setError} />
              <p className="vpn-file-note">Portal ve gateway ayrı giriş isteyebilir. Oturum açma verileri profile kaydedilmez.</p>
              {error && <p className="vpn-error" role="alert">{error}</p>}
              <footer><button type="button" className="secondary" onClick={() => { setAuth(undefined); void disconnect(challenge); }}>Bağlantıyı iptal et</button></footer>
            </div>
          ) : <form
            className="vpn-form"
            onSubmit={(e) => {
              e.preventDefault();
              const answer = values;
              setValues({});
              void perform(challenge.profileId, async () => {
                await call("vpn_answer", {
                  id: challenge.id,
                  generation: challenge.generation,
                  challengeId: challenge.challenge!.id,
                  values: answer,
                });
                if (alive.current) setAuth(undefined);
              });
            }}
          >
            <p>
              {
                profiles.find((profile) => profile.id === challenge.profileId)
                  ?.label
              }
            </p>
            <p>{challenge.challenge.message}</p>
            <fieldset disabled={busy.includes(challenge.profileId)}>
              {challenge.challenge.fields.map((field) =>
                field.choices?.length ? (
                  <Select
                    key={field.name}
                    label={field.label}
                    value={values[field.name] || ""}
                    options={[{ value: "", label: "Seçin" }, ...field.choices]}
                    onChange={(value) =>
                      setValues({ ...values, [field.name]: value })
                    }
                  />
                ) : (
                  <Field
                    key={field.name}
                    label={field.label}
                    type={
                      field.secret ||
                      /otp|password|token|secret/i.test(field.purpose)
                        ? "password"
                        : "text"
                    }
                    value={values[field.name] || ""}
                    onChange={(value) =>
                      setValues({ ...values, [field.name]: value })
                    }
                    required
                  />
                ),
              )}
            </fieldset>
            {error && (
              <p className="vpn-error" role="alert">
                {error}
              </p>
            )}
            {challenge.challenge.error && (
              <p className="vpn-error" role="alert">
                {challenge.challenge.error}
              </p>
            )}
            <p className="vpn-file-note">
              Yanıt yalnız bu bağlantıya gönderilir. OTP kaydedilmez.
            </p>
            <footer>
              {onOpenAuthenticator && <button type="button" className="secondary" disabled={busy.includes(challenge.profileId)} onClick={onOpenAuthenticator}><ShieldCheck size={16} />Authenticator’ı aç</button>}
              <button
                type="button"
                className="secondary"
                disabled={busy.includes(challenge.profileId)}
                onClick={() => {
                  setValues({});
                  setAuth(undefined);
                  void disconnect(challenge);
                }}
              >
                Bağlantıyı iptal et
              </button>
              <button
                type="submit"
                className="primary"
                disabled={
                  busy.includes(challenge.profileId) ||
                  challenge.challenge.fields.some(
                    (field) => !values[field.name],
                  )
                }
              >
                Gönder
              </button>
            </footer>
          </form>}
        </Modal>
      )}
    </section>
  );
}

function SsoPane({session,onError}: {session: VpnSession; onError: (message: string) => void}) {
  const pane = useRef<HTMLDivElement>(null);
  const mountedIdentity = useRef<string | undefined>(undefined);
  useEffect(() => {
    let current = true;
    let queued = false;
    const identity = {id:session.id,generation:session.generation,challengeId:session.challenge!.id};
    const key = `${identity.id}:${identity.generation}:${identity.challengeId}`;
    mountedIdentity.current = key;
    const update = async () => {
      if (!current || !pane.current || queued) return;
      queued = true;
      const {x,y,width,height} = pane.current.getBoundingClientRect();
      try { await call("vpn_sso_open",{...identity,bounds:{x,y,width,height}}); }
      catch (error) { if (current) onError(errorText(error)); }
      finally { queued = false; }
    };
    const observer = new ResizeObserver(() => void update());
    if (pane.current) observer.observe(pane.current);
    window.addEventListener("resize",update);
    void update();
    return () => {
      current = false;
      mountedIdentity.current = undefined;
      observer.disconnect();
      window.removeEventListener("resize",update);
      // React replays effects in development; only cancel a pane that stays closed or changes identity.
      queueMicrotask(() => { if (mountedIdentity.current !== key) void call("vpn_sso_close",identity).catch(() => {}); });
    };
  },[session.id,session.generation,session.challenge?.id]);
  return <div ref={pane} className="vpn-sso-pane" role="region" aria-label="Kurumsal SSO oturum açma"><span>Güvenli oturum açma ekranı hazırlanıyor…</span></div>;
}
