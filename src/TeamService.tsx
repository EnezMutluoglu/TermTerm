import TeamIdentities, { type TeamIdentity } from "./TeamIdentities";
import TeamCompatibility, { useTeamCompatibility } from "./TeamCompatibility";
import TeamUpdatePolicy from "./TeamUpdatePolicy";
import type { MenuAction } from "./ContextMenu";
import TeamDocuments, { type TeamContent, type TeamResource, type DocumentRequest } from "./TeamDocuments";
import TeamSharedRecovery from "./TeamSharedRecovery";
import TeamRotation from "./TeamRotation";
import TeamAgent from "./TeamAgent";
import TeamRouteProfiles from "./TeamRouteProfiles";
import TeamChain from "./TeamChain";
import TeamMove from "./TeamMove";
import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  ArrowLeft,
  Folder,
  KeyRound,
  LogOut,
  Plus,
  RefreshCw,
  Server,
  ShieldCheck,
  X,
  Box,
  ChevronDown,
  ArrowLeftRight,
  Link,
  LockKeyhole,
} from "lucide-react";
import WorkspaceBrand from "./WorkspaceBrand";
import TeamHosts from "./TeamHosts";
import TeamDelegation, { type DelegationState } from "./TeamDelegation";
import TeamShare from "./TeamShare";
import TeamAudit from "./TeamAudit";
import TeamRecovery from "./TeamRecovery";
import { call, errorText, saveFile } from "./api";
import "./team-service.css";
import TeamDecisions from "./TeamDecisions";
import TeamProfile from "./TeamProfile";
import TeamHistory from "./TeamHistory";
import TeamTerminals, { type TeamConnectRequest } from "./TeamTerminals";
import Sftp from "./Sftp";
import TeamTunnels, { type TunnelRequest } from "./TeamTunnels";
import type { Entity } from "./types";

type Member = {
  accountId: string;
  username: string;
  displayName: string;
  role: string;
  status: string;
};
type Content = TeamContent;
type Resource = TeamResource;
type Vault = { id: string; name: string; unavailable?: boolean };
type Profile = { serviceUrl: string; teamId: string; caPem: string | null };
type Preview = { id: string; affectedResources: number; requiredKeys: number };
type Impact = { id: string; kind: string; before: string[]; after: string[] };
const labels: Record<string, string> = {
  owner: "Sahip",
  manager: "Kapsam yöneticisi",
  editor: "Editör",
  operator: "Operatör",
  observer: "Gözlemci",
  read: "Görüntüle",
  connect: "Bağlan",
  edit: "Düzenle",
  allow: "İzin ver",
  deny: "Engelle",
  clear: "Devral",
};

export default function TeamService({
  onBack,
}: {
  onBack: () => void;
}) {
  const live = useRef(true),
    gate = useRef(false),
    journalGate = useRef(false),
    epoch = useRef(0);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [profile, setProfile] = useState<Profile>({
    serviceUrl: "",
    teamId: "",
    caPem: null,
  });
  const loginEdited = useRef(false);
  const [hasSavedLogin, setHasSavedLogin] = useState(false);
  const [remember, setRemember] = useState(false);
  const [savedLogin, setSavedLogin] = useState(false);
  const [username, setUsername] = useState(""),
    [password, setPassword] = useState(""),
    [confirm, setConfirm] = useState("");
  const [phase, setPhase] = useState<"login" | "activate" | "workspace">(
    "login",
  );
  const requiredUpdate = useTeamCompatibility(profile, phase);
  useEffect(() => {
    if (!requiredUpdate) return;
    epoch.current++;
    setPhase("login");
    setBusy(false);
    setPassword("");
    setConfirm("");
    setSecret("");
    setTemporary("");
    setSharedSecret(null);
    setKeyPassphrase("");
    setKeyCertificate("");
    setRecoveryPassword("");
    setReauthentication("");
    setRecoveryOpen(false);
    setRotationOpen(false);
    setProfileEditor(null);
    setConnectionRequest(null);
    setDocumentRequest(null);
    setTerminalActive(false);
    setVault(null);
    setVaults([]);
    setRows([]);
    setMembers([]);
    setOverview(null);
  }, [requiredUpdate]);
  const [journal, setJournal] = useState<{
    pending: number;
    conflicts: number;
    quarantined: number;
    oldestPendingMs?: number;
    editRemainingMs: number;
  } | null>(null);
  const [overview, setOverview] = useState<{
    teamName: string;
    displayName: string;
    role: string;
    accountId: string;
    offline?: boolean;
  } | null>(null);
  const [tab, setTab] = useState<
    | "vaults"
    | "documents"
    | "members"
    | "identities"
    | "decisions"
    | "sftp"
    | "tunnels"
    | "audit"
    | "updates"
  >("vaults");
  const [documentRequest,setDocumentRequest]=useState<DocumentRequest|null>(null);
  const [localHome, setLocalHome] = useState(""),
    [sftpRequest, setSftpRequest] = useState<{
      hostId: string;
      key: number;
    } | null>(null);
  const [tunnelRequest, setTunnelRequest] = useState<TunnelRequest | null>(
    null,
  );
  const [vaults, setVaults] = useState<Vault[]>([]),
    [vault, setVault] = useState<string | null>(null),
    [rows, setRows] = useState<Resource[]>([]);
  const [path, setPath] = useState<string[]>([]),
    [selected, setSelected] = useState<Resource | null>(null),
    [members, setMembers] = useState<Member[]>([]);
  const [identities, setIdentities] = useState<TeamIdentity[]>([]);
  const [sharedRecoveryPicker, setSharedRecoveryPicker] = useState(false);
  const [shareSource, setShareSource] = useState<{
    id: string;
    label: string;
    visibility?: string;
    kind?: string;
  } | null>(null);
  const [sharedSecret, setSharedSecret] = useState<{
    label: string;
    secret: string;
  } | null>(null);
  const [removeShared, setRemoveShared] = useState<{
    id: string;
    label: string;
    revision: number;
  } | null>(null);
  const [dialog, setDialog] = useState<
    "member" | "vault" | "resource" | "identity" | null
  >(null);
  const [name, setName] = useState(""),
    [userId, setUserId] = useState(""),
    [role, setRole] = useState("operator"),
    [temporary, setTemporary] = useState("");
  const [draft, setDraft] = useState<Content | null>(null),
    [editing, setEditing] = useState<Resource | null>(null);
  const [credentialKind, setCredentialKind] = useState("ssh_key"),
    [secret, setSecret] = useState("");
  const [accessResource, setAccessResource] = useState<Resource | null>(null),
    [target, setTarget] = useState(""),
    [effects, setEffects] = useState<Record<string, string>>({
      read: "allow",
      connect: "allow",
      edit: "clear",
    });
  const [preview, setPreview] = useState<Preview | null>(null),
    [impact, setImpact] = useState<Impact[]>([]),
    [staged, setStaged] = useState(0);
  const [draftId, setDraftId] = useState(() => crypto.randomUUID());
  const [routeHost, setRouteHost] = useState<string | null>(null);
  const [chainHost, setChainHost] = useState<(typeof rows)[number] | null>(null);
  const [movingRow,setMovingRow]=useState<(typeof rows)[number]|null>(null);
  const [profileEditor, setProfileEditor] = useState<
    Resource | "default" | null
  >(null);
  const [historyResource, setHistoryResource] = useState<Resource | null>(null);
  const [historySelections, setHistorySelections] = useState<
    { resourceId: string; revision: number }[]
  >([]);
  const [memberAction, setMemberAction] = useState<{
    member: Member;
    action: "resetMember" | "disableMember";
  } | null>(null);
  const [connectionRequest, setConnectionRequest] =
      useState<TeamConnectRequest | null>(null),
    [terminalActive, setTerminalActive] = useState(false),
    [keyPassphrase, setKeyPassphrase] = useState(""),
    [keyCertificate, setKeyCertificate] = useState("");
  function connect(row: Resource) {
    if (row.content?.kind === "host" && row.permissions?.includes("connect"))
      setConnectionRequest({
        requestId: crypto.randomUUID(),
        hostId: row.id,
        label:
          row.content.name +
          (historySelections.some((p) => p.resourceId === row.id)
            ? " · Geçmiş sürüm"
            : ""),
      });
  }
  const [sharedRecoveryId,setSharedRecoveryId] = useState<string|null>(null);
  const [rotationOpen, setRotationOpen] = useState(false);
  const [recoveryOpen, setRecoveryOpen] = useState(false),
    [recoveryPassword, setRecoveryPassword] = useState(""),
    [reauthentication, setReauthentication] = useState(""),
    [recoveryPath, setRecoveryPath] = useState("");
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    row: Resource;
  } | null>(null);
  const owner = overview?.role === "owner" && !overview?.offline;
  const [recoveryMember, setRecoveryMember] = useState<
    Member | "verify" | null
  >(null);
  const [delegations, setDelegations] = useState<DelegationState>({
    aclRevision: 0,
    delegations: [],
  });
  const [delegationResource, setDelegationResource] = useState<Resource | null>(
    null,
  );
  function manageable(row: Resource) {
    if (overview?.offline) return [];
    if (owner) return ["read", "connect", "edit"];
    if (overview?.role !== "manager") return [];
    const ancestors = new Set(folderPath(row.id));
    return ["read", "connect", "edit"].filter(
      (p) =>
        row.permissions?.includes(p) &&
        delegations.delegations.some(
          (d) =>
            d.accountId === overview.accountId &&
            ancestors.has(d.resourceId) &&
            d.permissions.includes(p),
        ),
    );
  }
  const parent = path.at(-1) ?? null;
  const [tabsTarget, setTabsTarget] = useState<HTMLElement | null>(null),
    [panelsTarget, setPanelsTarget] = useState<HTMLElement | null>(null);
  const [navigation, setNavigation] = useState(0),
    [vaultMenu, setVaultMenu] = useState(false);
  function navigate(next: typeof tab) {
    setTab(next);
    setNavigation((v) => v + 1);
    setSelected(null);
  }
  function rowMenu(e: React.MouseEvent, row: Resource) {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    setMenu({
      x: Math.min(e.clientX || rect.left, window.innerWidth - 225),
      y: Math.min(e.clientY || rect.bottom, window.innerHeight - 330),
      row,
    });
  }
  function folderPath(id: string) {
    const result: string[] = [];
    let current: string | null = id;
    const seen = new Set<string>();
    while (current && !seen.has(current)) {
      seen.add(current);
      result.unshift(current);
      current = rows.find((r) => r.id === current)?.content?.parentId ?? null;
    }
    return result;
  }

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
      epoch.current++;
      void call("team_v2_logout").catch(() => {});
    };
  }, []);
  async function rpc<T = any>(action: Record<string, unknown>): Promise<T> {
    if (!live.current) throw new Error("Oturum kapatıldı");
    const stamp = epoch.current;
    const result = await call<T>("team_v2_action", { action });
    if (!live.current || stamp !== epoch.current)
      throw new Error("Oturum kapatıldı");
    return result;
  }
  async function run(task: () => Promise<void>) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    const stamp = epoch.current;
    try {
      await task();
    } catch (e) {
      if (live.current && stamp === epoch.current) setError(errorText(e));
    } finally {
      gate.current = false;
      if (live.current && stamp === epoch.current) setBusy(false);
    }
  }
  useEffect(() => {
    if (phase !== "workspace") {
      setJournal(null);
      return;
    }
    let cancelled = false;
    const tick = async () => {
      if (cancelled || gate.current || journalGate.current || document.hidden) return;
      journalGate.current = true;
      try {
        const state = await rpc({ kind: "journalStatus" });
        if (cancelled) return;
        setJournal(state);
        if (state.pending > 0 || state.quarantined > 0 || state.conflicts > 0) {
          const synced = await rpc({ kind: "syncJournal" });
          if (!cancelled) setJournal(synced.journal);
        }
      } catch {
        /* Pending counts remain visible; an outage is not a saved receipt. */
      } finally {
        journalGate.current = false;
      }
    };
    void tick();
    const timer = setInterval(() => void tick(), 15000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [phase]);
  async function leave(next = onBack) {
    epoch.current++;
    setTemporary("");
    setSecret("");
    setPassword("");
    setConfirm("");
    await call("team_v2_logout").catch(() => {});
    next();
  }
  async function refresh() {
    const me = await rpc({ kind: "overview" });
    setOverview(me);
    if (!localHome)
      setLocalHome((await call<{ home: string }>("app_info")).home);
    const available: Vault[] = await rpc({ kind: "vaults" });
    setVaults(available);
    if (!me.offline && (me.role === "owner" || me.role === "manager")) {
      setMembers(await rpc({ kind: "members" }));
      setDelegations(await rpc({ kind: "delegations" }));
    } else if (me.offline) {
      setMembers([]); setDelegations({aclRevision:0,delegations:[]});
    }
    return available;
  }
  async function finishLogin(session: { activationRequired: boolean; rememberWarning?: string }) {
    const stamp = epoch.current;
    if (session.activationRequired) setPhase("activate");
    else {
      const available = await refresh();
      if (!live.current || stamp !== epoch.current) return;
      if (available.length === 1 && !available[0].unavailable) await loadVault(available[0].id);
      if (!live.current || stamp !== epoch.current) return;
      setPhase("workspace");
      if (session.rememberWarning) setNotice(`Giriş yapıldı, ancak parola kaydedilemedi: ${session.rememberWarning}`);
    }
  }
  useEffect(() => {
    let stopped = false;
    const initialEpoch = epoch.current;
    void call<{ profile: Profile; username: string; remembered: boolean; warning?: string } | null>("team_v2_saved_login")
      .then(saved => {
        if (stopped || initialEpoch !== epoch.current || loginEdited.current || !saved) return;
        setHasSavedLogin(true);
        setProfile(saved.profile); setUsername(saved.username);
        setRemember(saved.remembered); setSavedLogin(saved.remembered);
        if (saved.warning) setNotice(`Kayıtlı parola açılamadı: ${saved.warning}`);
      }).catch(e => { if (!stopped) setNotice(errorText(e)); });
    return () => { stopped = true; };
  }, []);
  async function login(e: FormEvent) {
    e.preventDefault();
    const value = password;
    setPassword("");
    await run(async () => {
      const stamp = epoch.current;
      const session = await call<{ activationRequired: boolean; rememberWarning?: string }>(
        "team_v2_login", { profile, username, password: value, remember },
      );
      if (!live.current || stamp !== epoch.current) return;
      await finishLogin(session);
    });
  }
  async function rememberedLogin() {
    await run(async () => {
      const stamp = epoch.current;
      const session = await call<{ activationRequired: boolean }>("team_v2_login_remembered");
      if (!live.current || stamp !== epoch.current) return;
      await finishLogin(session);
    });
  }
  async function activate(e: FormEvent) {
    e.preventDefault();
    if (password !== confirm) {
      setError("Parolalar eşleşmiyor");
      return;
    }
    const value = password;
    setPassword("");
    setConfirm("");
    await run(async () => {
      await rpc({ kind: "activate", password: value });
      setPhase("login");
      setNotice("Hesabın etkinleşti. Yeni parolanla giriş yap.");
    });
  }
  function identityActions(item: TeamIdentity): MenuAction[] {
    const shared = item.visibility === "shared";
    const actions: MenuAction[] = [{
      id: "default", label: "Varsayılan yap",
      disabled: item.unavailable || (shared && !item.permissions?.includes("connect")),
      run: () => run(async () => {
        const p = await rpc({ kind: "profiles" });
        await rpc({ kind: "setDefault", credentialId: item.id, expectedRevision: p.default?.revision ?? 0 });
        setNotice("Varsayılan kişisel kimliğin kaydedildi.");
      }),
    }];
    if (!shared && ["ssh_key", "password"].includes(item.kind))
      actions.push({ id: "share", label: "Ortak kimlik olarak paylaş", disabled: item.unavailable, run: () => setShareSource(item) });
    if (shared && item.permissions?.includes("reveal"))
      actions.push({ id: "reveal", label: "Sırrı göster", disabled: item.unavailable, run: () => run(async () => {
        const value = await rpc({ kind: "revealShared", id: item.id });
        setSharedSecret({ label: item.label, secret: value.secret });
      }) });
    if (shared && item.permissions?.includes("export"))
      actions.push({ id: "export", label: "Dışa aktar", disabled: item.unavailable, run: () => run(async () => {
        const file = await saveFile(item.kind === "ssh_key" ? "key" : "txt", item.kind === "ssh_key" ? "shared.key" : "shared.txt");
        if (file) {
          await rpc({ kind: "exportShared", id: item.id, path: file });
          setNotice("Ortak kimlik seçtiğin dosyaya aktarıldı.");
        }
      }) });
    if (shared && item.canUpdate)
      actions.push({ id: "update", label: "Alıcıları / kimliği güncelle", disabled: item.unavailable, run: () => setShareSource(item) });
    if (shared && item.canRemove)
      actions.push({ id: "remove", label: "Paylaşımı kaldır", danger: true, run: () => setRemoveShared(item) });
    return actions;
  }
  async function loadVault(id: string) {
    setHistorySelections(await rpc({ kind: "historySelections" }));
    const all: Resource[] = [];
    let after: string | null = null;
    do {
      const page: { records: Resource[]; nextAfter: string | null } = await rpc(
        { kind: "resources", vaultId: id, after },
      );
      all.push(...page.records);
      after = page.nextAfter;
    } while (after);
    setRows(all);
    setVault(id);
    setSelected(null);
  }
  function newResource(kind: "host" | "folder") {
    setDraftId(crypto.randomUUID());
    setEditing(null);
    setDraft({
      kind,
      parentId: parent,
      name: "",
      address: kind === "host" ? "" : null,
      protocol: kind === "host" ? "ssh" : null,
      port: kind === "host" ? 22 : null,
      usernamePolicy: kind === "host" ? "personal" : null,
      username: null,
      deleted: false,
    });
    setDialog("resource");
  }
  function openAccess(row: Resource) {
    setMenu(null);
    setAccessResource(row);
    setTarget("");
    setPreview(null);
    setImpact([]);
    setStaged(0);
  }
  async function prepareAccess() {
    const p = await rpc<Preview>({
      kind: "prepareAccess",
      accountId: target,
      resourceId: accessResource!.id,
      changes: Object.entries(effects)
        .filter(([p]) => manageable(accessResource!).includes(p))
        .map(([permission, effect]) => ({
          permission,
          effect,
        })),
    });
    setPreview(p);
    const changes: Impact[] = [];
    let after: string | null = null;
    do {
      const page: { records: Impact[]; nextAfter: string | null } = await rpc({
        kind: "accessImpact",
        planId: p.id,
        after,
      });
      changes.push(...page.records);
      after = page.nextAfter;
    } while (after);
    setImpact(changes);
  }
  async function applyAccess() {
    let after: number | null = 0;
    let total = 0;
    do {
      const page: { staged: number; nextAfter: number | null } = await rpc({
        kind: "stageAccess",
        planId: preview!.id,
        after,
      });
      total += page.staged;
      setStaged(total);
      after = page.nextAfter;
    } while (after !== null);
    await rpc({ kind: "finishAccess", planId: preview!.id, cancel: false });
    setAccessResource(null);
    setPreview(null);
    setNotice("İzinler ve anahtar paylaşımı kaydedildi.");
  }
  async function closeAccess() {
    if (preview)
      await rpc({ kind: "finishAccess", planId: preview.id, cancel: true });
    setAccessResource(null);
    setPreview(null);
  }
  const closeDialog = () => {
    setDialog(null);
    setSecret("");
    setKeyPassphrase("");
  };

  if (requiredUpdate) return <TeamCompatibility policy={requiredUpdate} onBack={() => void leave()}/>;
  return (
    <div
      className={
        "team-service " + (phase === "workspace" ? "app ts-desktop" : "")
      }
      onClick={() => menu && setMenu(null)}
    >
      {phase === "workspace" ? (
        <header className="topbar">
          <WorkspaceBrand />
          <div className="ts-tabs-mount" ref={setTabsTarget} />
          <div className="top-actions">
            <span className="local-badge">
              <span className="status-dot live" />
              Team kasası
            </span>
            <button
              className="icon-btn"
              aria-label="Team oturumunu kapat"
              title="Team oturumunu kapat"
              onClick={() => void leave()}
            >
              <LockKeyhole size={17} />
            </button>
            <div className="avatar tiny" title={overview?.displayName}>
              {overview?.displayName?.[0]}
            </div>
          </div>
        </header>
      ) : (
        <header className="ts-header">
          <div>
            <ShieldCheck size={24} />
            <strong>TermTerm Team</strong>
            <span className="ts-chip">Geliştirme</span>
          </div>
          <button onClick={() => void leave()}>
            <ArrowLeft size={16} />
            Kişisel kasaya dön
          </button>
        </header>
      )}
      {phase === "workspace" && (
        <TeamTerminals
          request={connectionRequest}
          documentRequest={documentRequest}
          onActive={setTerminalActive}
          tabsTarget={tabsTarget}
          panelsTarget={panelsTarget}
          workspace={tab}
          navigation={navigation}
          onWorkspace={navigate}
          onLock={() => void leave()}
        />
      )}
      {error && (
        <div className="ts-message ts-error" role="alert">
          {error}
        </div>
      )}
      {notice && (
        <div className="ts-message" role="status">
          {notice}
        </div>
      )}
      {phase === "workspace" &&
        journal &&
        journal.pending + journal.conflicts + journal.quarantined > 0 && (
          <div className="ts-message ts-journal" role="status">
            Yerel günlük: {journal.pending} bekleyen · {journal.conflicts} karar
            bekleyen · {journal.quarantined} yetki sonrası teslim bekleyen.
            {journal.oldestPendingMs
              ? ` En eski: ${new Date(journal.oldestPendingMs).toLocaleString("tr-TR")}.`
              : ""}
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const result = await rpc({ kind: "syncJournal" });
                  setJournal(result.journal);
                  if (vault) await loadVault(vault);
                })
              }
            >
              Şimdi eşitle
            </button>
          </div>
        )}
      {phase === "workspace" && overview?.offline && <div className="ts-message" role="status">
        Çevrimdışı önbellek açık. Yalnız daha önce indirilen kayıtlar ve doğrulanan bağlantılar kullanılabilir.
        Yönetim işlemleri çevrimiçi bağlantı gerektirir. Merkezdeki yeni yetki iptalleri henüz doğrulanamıyor.
        <button disabled={busy} onClick={() => void run(async () => { await refresh(); if (vault) await loadVault(vault); })}>Bağlantıyı yeniden doğrula</button>
      </div>}
      {phase !== "workspace" ? (
        <section className="ts-login">
          <h1>
            {phase === "activate"
              ? "Hesabını etkinleştir"
              : "Team hesabına giriş"}
          </h1>
          <p>
            {phase === "activate"
              ? "Liderinin verdiği geçici parola yerine kendi parolanı belirle. Şifreleme kimliğin bu bilgisayarda oluşturulur."
              : "Liderinin verdiği servis adresini, takım kimliğini ve kullanıcı bilgilerini kullan."}
          </p>
          {phase === "login" && hasSavedLogin && <div>
            {savedLogin && <button type="button" className="ts-primary" disabled={busy} onClick={() => void rememberedLogin()}>Kayıtlı hesabımla devam et</button>}
            <button type="button" disabled={busy} onClick={() => void run(async () => {
              await call("team_v2_forget_login"); setSavedLogin(false); setHasSavedLogin(false); setRemember(false);
              setNotice("Bu cihazdaki Team giriş kaydı silindi. Şifreli kasa ve bekleyen işlemler korunur.");
            })}>Bu cihazda beni unut</button>
          </div>}
          <form onSubmit={phase === "activate" ? activate : login}>
            {phase === "login" && (
              <>
                <label>
                  Servis adresi
                  <input
                    required
                    type="url"
                    placeholder="https://team.firma.com"
                    value={profile.serviceUrl}
                    onChange={(e) =>
                      { loginEdited.current = true; setSavedLogin(false); setProfile({ ...profile, serviceUrl: e.target.value }); }
                    }
                  />
                </label>
                <label>
                  Takım kimliği
                  <input
                    required
                    placeholder="Takım UUID"
                    value={profile.teamId}
                    onChange={(e) =>
                      { loginEdited.current = true; setSavedLogin(false); setProfile({ ...profile, teamId: e.target.value }); }
                    }
                  />
                </label>
                <label>
                  Kullanıcı adı
                  <input
                    required
                    autoComplete="username"
                    value={username}
                    onChange={(e) => { loginEdited.current = true; setSavedLogin(false); setUsername(e.target.value); }}
                  />
                </label>
                <details>
                  <summary>Şirket CA sertifikası (isteğe bağlı)</summary>
                  <textarea
                    aria-label="CA sertifikası"
                    rows={4}
                    value={profile.caPem ?? ""}
                    onChange={(e) =>
                      { loginEdited.current = true; setSavedLogin(false); setProfile({ ...profile, caPem: e.target.value || null }); }
                    }
                    placeholder="-----BEGIN CERTIFICATE-----"
                  />
                </details>
              </>
            )}
            <label>
              {phase === "activate" ? "Yeni parola" : "Parola"}
              <input
                required
                type="password"
                autoComplete={
                  phase === "activate" ? "new-password" : "current-password"
                }
                minLength={phase === "activate" ? 12 : 1}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </label>
            {phase === "activate" && (
              <label>
                Yeni parola tekrar
                <input
                  required
                  type="password"
                  autoComplete="new-password"
                  value={confirm}
                  onChange={(e) => setConfirm(e.target.value)}
                />
              </label>
            )}
            {phase === "login" && <label>
              <input type="checkbox" checked={remember} onChange={e => setRemember(e.target.checked)} />
              Beni hatırla (bu cihazın güvenli deposu)
            </label>}
            <button className="ts-primary" disabled={busy}>
              {busy
                ? "İşlem sürüyor…"
                : phase === "activate"
                  ? "Parolayı belirle"
                  : "Giriş yap"}
            </button>
          </form>
          {phase === "login" && <details className="ts-draft-delivery">
            <summary>Hesabım kapatıldı; bekleyen taslaklarım var</summary>
            <p>Servis adresi, takım ve kullanıcı adını doldur. Parola alanına bu cihazdaki şifreli önbelleği açan eski parolanı gir. Bu işlem oturum açmaz ve hostlara erişim vermez; yalnız önceden yetkili olduğun bekleyen değişiklikleri takımın karar kuyruğuna iletir.</p>
            <button type="button" disabled={busy || !password || !username || !profile.teamId || !profile.serviceUrl} onClick={() => {
              const value=password;setPassword("");
              void run(async()=>{
                const result=await call<{delivered:number;remaining:number;awaitingDecision:number}>("team_v2_submit_drafts",{profile,username,password:value});
                if(live.current)setNotice(`${result.delivered} işlem teslim edildi. ${result.remaining} işlem gönderilmeyi, ${result.awaitingDecision} işlem yetkili kararını bekliyor. Oturum açılmadı.`);
              });
            }}>Bekleyen taslakları ilet</button>
          </details>}
          <small>
            Bu ekranda PostgreSQL kullanıcı adı veya parolası istenmez. Kayıt
            işlemini takım liderin yapar.
          </small>
        </section>
      ) : (
        <div className="ts-workspace app-body">
          <aside className="sidebar">
            <button
              className="vault-selector"
              onClick={() => setVaultMenu((v) => !v)}
            >
              <div className="vault-icon">
                <Box size={18} />
              </div>
              <span>
                <strong>
                  {vaults.find((v) => v.id === vault)?.name ??
                    overview?.teamName}
                </strong>
                <small>Team</small>
              </span>
              <ChevronDown size={14} />
            </button>
            {vaultMenu && (
              <div className="vault-menu">
                {vaults.map((v) => (
                  <button
                    key={v.id}
                    disabled={v.unavailable || busy}
                    onClick={() =>
                      void run(async () => {
                        await loadVault(v.id);
                        setPath([]);
                        navigate("vaults");
                        setVaultMenu(false);
                      })
                    }
                  >
                    {v.name}
                  </button>
                ))}
                {owner && (
                  <button
                    onClick={() => {
                      setVault(null);
                      navigate("vaults");
                      setVaultMenu(false);
                    }}
                  >
                    Kasalar
                  </button>
                )}
              </div>
            )}
            <div className="nav-label">ÇALIŞMA ALANI</div>
            <nav className="main-nav">
              <button
                className={tab === "vaults" && !terminalActive ? "active" : ""}
                onClick={() => {
                  setPath([]);
                  navigate("vaults");
                }}
              >
                <Server size={17} />
                <span>Hosts</span>
                <small>
                  {rows.filter((r) => r.content?.kind === "host").length}
                </small>
              </button>
              <button
                className={tab === "sftp" && !terminalActive ? "active" : ""}
                onClick={() => navigate("sftp")}
              >
                <ArrowLeftRight size={17} />
                <span>SFTP</span>
              </button>
              <button
                className={
                  tab === "identities" && !terminalActive ? "active" : ""
                }
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    setKeyCertificate("");
                    setIdentities(await rpc({ kind: "credentials" }));
                    navigate("identities");
                  })
                }
              >
                <KeyRound size={17} />
                <span>Kimliklerim</span>
              </button>
              <button className={tab === "documents" && !terminalActive ? "active" : ""} onClick={()=>navigate("documents")}><Box size={17}/><span>Komutlar / Workspaces</span></button>
              <button
                className={tab === "tunnels" && !terminalActive ? "active" : ""}
                onClick={() => navigate("tunnels")}
              >
                <Link size={17} />
                <span>Tüneller</span>
              </button>
              <button
                className={
                  ["members", "decisions", "audit", "updates"].includes(tab) &&
                  !terminalActive
                    ? "active"
                    : ""
                }
                onClick={() => navigate(owner ? "members" : "decisions")}
              >
                <ShieldCheck size={17} />
                <span>Team</span>
              </button>
            </nav>
            <div className="nav-label">GRUPLAR</div>
            <div className="group-tree">
              {rows
                .filter((r) => r.content?.kind === "folder")
                .map((row) => (
                  <button
                    key={row.id}
                    className={
                      "sidebar-group " + (parent === row.id ? "active" : "")
                    }
                    style={{
                      paddingLeft:
                        8 + Math.min(folderPath(row.id).length - 1, 5) * 12,
                    }}
                    onClick={() => {
                      setPath(folderPath(row.id));
                      navigate("vaults");
                    }}
                    onContextMenu={(e) => rowMenu(e, row)}
                  >
                    <Folder size={15} />
                    <span className="sidebar-group-label">
                      {row.content!.name}
                    </span>
                  </button>
                ))}
            </div>
            <div className="sidebar-bottom">
              <small>
                {overview?.displayName} · {labels[overview?.role ?? ""]}
              </small>
              <button onClick={() => void leave()}>
                <LogOut size={17} />
                Kişisel kasaya dön
              </button>
            </div>
          </aside>
          <main className="main-content" aria-busy={busy}>
            <div
              className="ts-terminal-mount"
              ref={setPanelsTarget}
              style={{ display: terminalActive ? "flex" : "none" }}
            />
            <div
              className="ts-browse records-page"
              data-view={tab}
              style={{ display: terminalActive ? "none" : undefined }}
            >
              {["members", "decisions", "audit", "updates"].includes(tab) && (
                <nav className="ts-admin-nav">
                  {owner&&<button className={tab==="updates"?"active":""} onClick={()=>navigate("updates")}>Sürüm politikası</button>}
                  {owner && (
                    <button
                      className={tab === "members" ? "active" : ""}
                      onClick={() => navigate("members")}
                    >
                      Üyeler ve yetkiler
                    </button>
                  )}
                  <button
                    className={tab === "decisions" ? "active" : ""}
                    onClick={() => navigate("decisions")}
                  >
                    Bekleyen kararlar
                  </button>
                  {owner && (
                    <button
                      className={tab === "audit" ? "active" : ""}
                      onClick={() => navigate("audit")}
                    >
                      İşlem geçmişi
                    </button>
                  )}
                  {owner && (
                    <button
                      onClick={() => {
                        setRecoveryPassword("");
                        setReauthentication("");
                        setRecoveryPath("");
                        setRecoveryOpen(true);
                      }}
                    >
                      Kurtarma anahtarı kur
                    </button>
                  )}
                  {owner && <button onClick={() => setRotationOpen(true)}>Kurtarma anahtarını değiştir</button>}
                  {owner && <button disabled={busy} onClick={() => void run(async () => {
                    setIdentities(await rpc({kind:"credentials"}));
                    setSharedRecoveryPicker(true);
                  })}>Ortak kimliği kurtar</button>}
                </nav>
              )}

              {historySelections.length > 0 && (
                <div className="ts-history-return" role="status">
                  <span>
                    Geçmiş sürüm kullanılıyor ({historySelections.length}{" "}
                    kayıt). Yeni bağlantılar seçtiğin eski sürümü kullanır.
                  </span>
                  <button
                    disabled={busy}
                    onClick={() =>
                      void run(async () => {
                        for (const selected of historySelections)
                          await rpc({
                            kind: "selectHistory",
                            id: selected.resourceId,
                            revision: null,
                          });
                        setHistorySelections([]);
                      })
                    }
                  >
                    Güncele dön
                  </button>
                </div>
              )}
              <div
                className="ts-toolbar"
                style={{ display: tab === "sftp" ? "none" : undefined }}
              >
                <h1>
                  {tab === "updates" ? "Team güncelleme yönetimi" : tab === "documents" ? "Komutlar ve çalışma alanları" : tab === "audit"
                    ? "İşlem geçmişi"
                    : tab === "tunnels"
                      ? "Tüneller"
                      : tab === "sftp"
                        ? "SFTP"
                        : tab === "decisions"
                          ? "Bekleyen kararlar"
                          : tab === "members"
                            ? "Üyeler ve yetkiler"
                            : tab === "identities"
                              ? "Kimliklerim"
                              : "Hosts"}
                </h1>
                <button
                  disabled={busy}
                  aria-label="Yenile"
                  onClick={() =>
                    void run(async () => {
                      await refresh();
                      if (vault) await loadVault(vault);
                      if (tab === "identities") {
                        setKeyCertificate("");
                        setIdentities(await rpc({ kind: "credentials" }));
                      }
                    })
                  }
                >
                  <RefreshCw size={17} />
                </button>
                {tab === "members" && (
                  <button
                    disabled={busy}
                    className="ts-primary"
                    onClick={() => {
                      setName("");
                      setUserId("");
                      setDialog("member");
                    }}
                  >
                    <Plus size={17} />
                    Kullanıcı oluştur
                  </button>
                )}
                {tab === "identities" && (
                  <button
                    disabled={busy}
                    onClick={() => setProfileEditor("default")}
                  >
                    Varsayılan bağlantı profilim
                  </button>
                )}
                {tab === "identities" && (
                  <button
                    disabled={busy}
                    className="ts-primary"
                    onClick={() => {
                      setName("");
                      setSecret("");
                      setDialog("identity");
                    }}
                  >
                    <Plus size={17} />
                    Özel kimlik ekle
                  </button>
                )}
                {tab === "vaults" && owner && !vault && (
                  <button
                    disabled={busy}
                    className="ts-primary"
                    onClick={() => {
                      setName("");
                      setDialog("vault");
                    }}
                  >
                    <Plus size={17} />
                    Kasa oluştur
                  </button>
                )}
              </div>
              {busy && (
                <div className="ts-progress" role="status">
                  İşlem sürüyor… {staged > 0 && `${staged} anahtar hazırlandı`}
                </div>
              )}
              {tab === "documents" && <TeamDocuments key={vault??"none"} rows={rows} vault={vault} owner={owner || overview?.role==="owner"} parent={parent}
                onFolder={id=>setPath(id?folderPath(id):[])} onSaved={async()=>{if(vault)await loadVault(vault);}}
                onStart={setDocumentRequest} onHistory={setHistoryResource} onManage={rowMenu}/>}
              {tab === "decisions" && <TeamDecisions />}
              {tab === "audit" && owner && <TeamAudit />}
              {tab === "updates" && owner && <TeamUpdatePolicy />}
              <div style={{ display: tab === "tunnels" ? "block" : "none" }}>
                <TeamTunnels request={tunnelRequest} />
              </div>
              {phase === "workspace" && localHome && (
                <div style={{ display: tab === "sftp" ? "block" : "none" }}>
                  <Sftp
                    key={`${profile.serviceUrl}:${profile.teamId}:${overview?.accountId}:${vault}`}
                    home={localHome}
                    visible={tab === "sftp" && !terminalActive}
                    records={rows
                      .filter(
                        (r) =>
                          r.content && !r.content.deleted && !r.unavailable &&
                          (r.content.kind === "folder" ||
                           (r.content.kind === "host" && r.permissions?.includes("connect"))),
                      )
                      .map(
                        (r) =>
                          ({
                            id: r.id,
                            kind: r.content!.kind === "folder" ? "group" : "host",
                            updatedAt: 0,
                            data: {
                              label: r.content!.name,
                              protocol: r.content!.protocol,
                              address: r.content!.address,
                              groupId: r.content!.parentId,
                            },
                          }) as Entity,
                      )}
                    openRequest={sftpRequest}
                    notify={setError}
                    connectRemote={(id, hostId) =>
                      call<string>("team_v2_sftp", { id, hostId })
                    }
                  />
                </div>
              )}
              {tab === "members" && (
                <>
                  <p>
                    Hesap oluşturmak kaynak erişimi vermez. Üye ilk girişini
                    tamamladıktan sonra ortak klasör veya hostun sağ tık
                    menüsünden izinlerini ayarla.
                  </p>
                  <table>
                    <thead>
                      <tr>
                        <th>Kullanıcı</th>
                        <th>Görünen ad</th>
                        <th>Rol</th>
                        <th>Durum</th>
                        <th />
                      </tr>
                    </thead>
                    <tbody>
                      {members.map((m) => (
                        <tr key={m.accountId}>
                          <td>{m.username}</td>
                          <td>{m.displayName}</td>
                          <td>{labels[m.role]}</td>
                          <td>
                            {m.status === "active"
                              ? "Etkin"
                              : m.status === "pending"
                                ? "İlk giriş bekliyor"
                                : "Devre dışı"}
                          </td>
                          <td>
                            {m.status === "active" && (
                              <button
                                disabled={busy}
                                onClick={() => setRecoveryMember(m)}
                              >
                                Kimlikleri kurtar
                              </button>
                            )}
                            {m.accountId !== overview?.accountId && (
                              <button
                                disabled={busy}
                                onClick={() =>
                                  setMemberAction({
                                    member: m,
                                    action: "resetMember",
                                  })
                                }
                              >
                                Parolayı sıfırla
                              </button>
                            )}
                            {m.accountId !== overview?.accountId &&
                              m.status !== "disabled" && (
                                <button
                                  disabled={busy}
                                  onClick={() =>
                                    setMemberAction({
                                      member: m,
                                      action: "disableMember",
                                    })
                                  }
                                >
                                  Hesabı devre dışı bırak
                                </button>
                              )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </>
              )}
              {tab === "identities" && (
                <>
                  <div className="ts-actions">
                    <button
                      disabled={busy}
                      onClick={() => setRecoveryMember("verify")}
                    >
                      Kurtarmayı doğrula
                    </button>
                  </div>
                  <p>
                    Kişisel anahtar ve parolaların şifrelenerek takım sunucusuna
                    kaydedilir. Liderin şirket kurtarma anahtarını kurmuş olması
                    gerekir.
                  </p>
                  <TeamIdentities items={identities} busy={busy} actions={identityActions} onError={e => setError(errorText(e))}/>

                </>
              )}
              {tab === "vaults" && !vault && (
                <div className="ts-cards">
                  {vaults.map((v) => (
                    <button
                      key={v.id}
                      disabled={busy || v.unavailable}
                      onClick={() =>
                        void run(async () => {
                          setPath([]);
                          await loadVault(v.id);
                        })
                      }
                    >
                      <Folder />
                      <strong>{v.name}</strong>
                      <span>Kasayı aç</span>
                    </button>
                  ))}
                  {vaults.length === 0 && (
                    <p className="ts-empty">
                      Erişebildiğin ortak kasa yok.
                      {owner
                        ? " Yeni kasa oluşturabilirsin."
                        : " Takım liderinden erişim iste."}
                    </p>
                  )}
                </div>
              )}
              {tab === "vaults" && vault && (
                <>
                  <nav className="ts-breadcrumb">
                    <button
                      disabled={busy}
                      onClick={() => {
                        setVault(null);
                        setPath([]);
                        setSelected(null);
                      }}
                    >
                      Kasalar
                    </button>
                    <span>/</span>
                    <button
                      onClick={() => {
                        setPath([]);
                        setSelected(null);
                      }}
                    >
                      {vaults.find((v) => v.id === vault)?.name}
                    </button>
                    {path.map((id, index) => (
                      <span key={id}>
                        {" "}
                        /{" "}
                        <button
                          onClick={() => {
                            setPath(path.slice(0, index + 1));
                            setSelected(null);
                          }}
                        >
                          {rows.find((r) => r.id === id)?.content?.name ??
                            "Klasör"}
                        </button>
                      </span>
                    ))}
                  </nav>
                  {(owner || (overview?.offline && overview.role === "owner") ||
                    rows
                      .find((r) => r.id === parent)
                      ?.permissions?.includes("edit")) && (
                    <div className="ts-actions">
                      <button
                        disabled={busy}
                        onClick={() => newResource("folder")}
                      >
                        <Plus size={15} />
                        Klasör
                      </button>
                      <button
                        disabled={busy}
                        onClick={() => newResource("host")}
                      >
                        <Plus size={15} />
                        Host
                      </button>
                    </div>
                  )}
                  {rows.some((r) => r.unavailable) && (
                    <p role="alert">
                      Bazı kayıtların anahtarı açılamadı. Yetki/kimlik
                      kurtarması gerekiyor.
                    </p>
                  )}
                  <div className="ts-content">
                    <TeamHosts
                      rows={rows}
                      parent={parent}
                      selected={selected?.id ?? null}
                      busy={busy}
                      onFolder={(id) => {
                        setPath([...path, id]);
                        setSelected(null);
                      }}
                      onSelect={setSelected}
                      onConnect={connect}
                      onMenu={rowMenu}
                    />
                    {selected && (
                      <section className="ts-details">
                        <button
                          aria-label="Bilgileri kapat"
                          onClick={() => setSelected(null)}
                        >
                          <X size={16} />
                        </button>
                        <h2>{selected.content?.name}</h2>
                        <dl>
                          <dt>Adres</dt>
                          <dd>{selected.content?.address}</dd>
                          <dt>Port</dt>
                          <dd>{selected.content?.port}</dd>
                          <dt>Kullanıcı politikası</dt>
                          <dd>
                            {selected.content?.usernamePolicy ===
                            "shared_required"
                              ? selected.content?.username
                              : "Kişisel profil"}
                          </dd>
                        </dl>
                        <small>
                          Bağlantı kimliği kişisel profilinde seçilir. Çift
                          tıklayarak SSH terminali açabilirsin.
                        </small>
                      </section>
                    )}
                  </div>
                </>
              )}
            </div>
          </main>
        </div>
      )}
      {sharedRecoveryPicker && <div className="ts-overlay"><section className="ts-dialog" role="dialog" aria-modal="true" aria-label="Kurtarılacak ortak kimlik">
        <h2>Ortak kimliği kurtar</h2><p>Şirket kurtarma dosyası ve yeniden doğrulama gereklidir.</p>
        {identities.filter(item => item.visibility === "shared").map(item => <button key={item.id} onClick={() => {setSharedRecoveryPicker(false);setSharedRecoveryId(item.id);}}>{item.label}</button>)}
        {!identities.some(item => item.visibility === "shared") && <p>Kurtarılabilir ortak kimlik yok.</p>}
        <button onClick={() => setSharedRecoveryPicker(false)}>Vazgeç</button>
      </section></div>}
      {routeHost && <TeamRouteProfiles id={routeHost} rpc={rpc} onClose={() => setRouteHost(null)} />}
      {chainHost && <TeamChain host={chainHost} rows={rows} rpc={rpc} onClose={() => setChainHost(null)} onSaved={() => setNotice("Host chain kaydedildi.")} />}
      {movingRow && <TeamMove row={movingRow} rows={rows} rpc={rpc} onClose={()=>setMovingRow(null)} onSaved={()=>void run(async()=>{await refresh();if(vault)await loadVault(vault);setNotice("Kayıt taşındı; erişimler ve chain anahtarları yenilendi.");})} />}
      {menu && (
        <div
          className="ts-context"
          role="menu"
          style={{ left: menu.x, top: menu.y }}
        >
          {menu.row.content?.kind === "host" &&
            menu.row.permissions?.includes("connect") && (
              <button
                role="menuitem"
                onClick={() => {
                  connect(menu.row);
                  setMenu(null);
                }}
              >
                SSH terminali aç
              </button>
            )}
          {menu.row.content?.kind === "host" &&
            menu.row.permissions?.includes("connect") && (
              <>
                <button
                  role="menuitem"
                  onClick={() => {
                    setSftpRequest({ hostId: menu.row.id, key: Date.now() });
                    setTab("sftp");
                    setMenu(null);
                  }}
                >
                  SFTP ile aç
                </button>
                <button
                  role="menuitem"
                  onClick={() => {
                    setTunnelRequest({
                      key: crypto.randomUUID(),
                      hostId: menu.row.id,
                      label: menu.row.content!.name,
                    });
                    setTab("tunnels");
                    setMenu(null);
                  }}
                >
                  Tünel aç
                </button>
              </>
            )}
          {!menu.row.content?.document && <button
            role="menuitem"
            onClick={() => {
              setSelected(menu.row);
              setMenu(null);
            }}
          >
            Bilgiler
          </button>}
          {menu.row.permissions?.includes("edit") && !menu.row.content?.document && (
            <button
              role="menuitem"
              onClick={() => {
                setEditing(menu.row);
                setDraft(menu.row.content!);
                setDialog("resource");
                setMenu(null);
              }}
            >
              Düzenle
            </button>
          )}
          {menu.row.permissions?.includes("read") && (
            <button
              role="menuitem"
              onClick={() => {
                setHistoryResource(menu.row);
                setMenu(null);
              }}
            >
              Kayıt geçmişi
            </button>
          )}
          {menu.row.content?.kind === "host" &&
            menu.row.permissions?.includes("connect") && (
              <button
                role="menuitem"
                onClick={() => {
                  setProfileEditor(menu.row);
                  setMenu(null);
                }}
              >
                Kişisel bağlantı profilim
              </button>
            )}
          {menu.row.content?.kind === "host" && menu.row.permissions?.includes("connect") && <button role="menuitem" onClick={() => {setRouteHost(menu.row.id);setMenu(null);}}>Zincirdeki kişisel profillerim</button>}
          {manageable(menu.row).length > 0 && (
            <button role="menuitem" onClick={() => openAccess(menu.row)}>
              Üye izinleri
            </button>
          )}
          {(owner || (overview?.role === "manager" && menu.row.permissions?.includes("edit"))) && menu.row.content?.kind === "host" && <button role="menuitem" onClick={() => { setChainHost(menu.row); setMenu(null); }}>Host chain</button>}
          {(owner||overview?.role==="manager")&&menu.row.permissions?.includes("edit")&&<button role="menuitem" onClick={()=>{setMovingRow(menu.row);setMenu(null);}}>Taşı</button>}
          {owner && (
            <button
              role="menuitem"
              onClick={() => {
                setDelegationResource(menu.row);
                setMenu(null);
              }}
            >
              Yetki devri
            </button>
          )}
        </div>
      )}
      {recoveryMember && (
        <TeamRecovery
          member={recoveryMember === "verify" ? null : recoveryMember}
          onClose={() => setRecoveryMember(null)}
          onSaved={() => {
            setNotice(
              "Kurtarma adımı kaydedildi. Kimlik listesini yenileyebilirsin.",
            );
          }}
        />
      )}
      {shareSource && (
        <TeamShare
          source={shareSource}
          privateSources={identities
            .filter(
              (i) =>
                i.visibility !== "shared" &&
                !i.unavailable &&
                i.kind === shareSource.kind,
            )
            .map((i) => ({ id: i.id, label: i.label }))}
          scopes={rows
            .filter(
              (r) =>
                r.content &&
                !r.content.deleted &&
                r.permissions?.includes("edit"),
            )
            .map((r) => ({ id: r.id, name: r.content!.name }))}
          rpc={rpc}
          onClose={() => setShareSource(null)}
          onSaved={() => {
            setShareSource(null);
            void run(async () => {
              setIdentities(await rpc({ kind: "credentials" }));
              setNotice("Ortak kimlik kaydedildi. Özel kimliğin korunuyor.");
            });
          }}
        />
      )}
      {sharedSecret && (
        <div className="ts-overlay">
          <section
            className="ts-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Ortak kimlik sırrı"
          >
            <h2>{sharedSecret.label}</h2>
            <textarea
              readOnly
              value={sharedSecret.secret}
              aria-label="Sır"
              spellCheck={false}
            />
            <button onClick={() => setSharedSecret(null)}>Kapat</button>
          </section>
        </div>
      )}
      {removeShared && (
        <div className="ts-overlay">
          <section
            className="ts-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Paylaşımı kaldır"
          >
            <h2>{removeShared.label}</h2>
            <p>
              Ortak kopya bağlantılarda artık kullanılamaz. Özel kaynak korunur.
              Daha önce dışarı alınmış kopyalar geri çekilemez.
            </p>
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await rpc({
                    kind: "removeShared",
                    id: removeShared.id,
                    expectedRevision: removeShared.revision,
                  });
                  setRemoveShared(null);
                  setIdentities(await rpc({ kind: "credentials" }));
                })
              }
            >
              Paylaşımı kaldır
            </button>
            <button disabled={busy} onClick={() => setRemoveShared(null)}>
              Vazgeç
            </button>
          </section>
        </div>
      )}
      {delegationResource?.content && (
        <TeamDelegation
          resource={{
            id: delegationResource.id,
            name: delegationResource.content.name,
          }}
          members={members}
          rpc={rpc}
          onClose={() => setDelegationResource(null)}
          onSaved={() => {
            setDelegationResource(null);
            void run(async () => {
              await refresh();
            });
          }}
        />
      )}
      {dialog && (
        <div className="ts-overlay">
          <section
            role="dialog"
            aria-modal="true"
            aria-label={
              dialog === "member"
                ? "Kullanıcı oluştur"
                : dialog === "vault"
                  ? "Kasa oluştur"
                  : dialog === "identity"
                    ? "Özel kimlik ekle"
                    : "Kaynak düzenle"
            }
            className="ts-dialog"
          >
            <button
              className="ts-close"
              disabled={busy}
              onClick={closeDialog}
              aria-label="Kapat"
            >
              <X />
            </button>
            {busy && (
              <button type="button" onClick={() => void leave()}>
                İşlemi beklemeden oturumu kapat
              </button>
            )}
            <h2>
              {dialog === "member"
                ? "Kullanıcı oluştur"
                : dialog === "vault"
                  ? "Kasa oluştur"
                  : dialog === "identity"
                    ? "Özel kimlik ekle"
                    : draft?.kind === "folder"
                      ? "Klasör"
                      : editing ? "Makineyi düzenle" : "Makine ekle"}
            </h2>
            <form className={dialog === "resource" && draft?.kind === "host" ? "ts-host-form" : undefined}
              onSubmit={(e) => {
                e.preventDefault();
                void run(async () => {
                  if (dialog === "member") {
                    const result = await rpc({
                      kind: "createMember",
                      username: userId,
                      displayName: name,
                      role,
                    });
                    setTemporary(result.temporaryPassword);
                    await refresh();
                  }
                  if (dialog === "vault") {
                    await rpc({
                      kind: "createVault",
                      id: crypto.randomUUID(),
                      name,
                    });
                    await refresh();
                  }
                  if (dialog === "resource") {
                    const receipt = await rpc({
                      kind: "saveResource",
                      vaultId: vault,
                      id: editing?.id ?? draftId,
                      expectedRevision: editing?.revision ?? 0,
                      localGeneration: editing?.localGeneration ?? null,
                      clientAt: new Date().toISOString(),
                      content: draft,
                    });
                    if (receipt.status === "conflict") {
                      setDialog(null);
                      setNotice(
                        "Eşzamanlı değişiklik bulundu. Kayıt üzerine yazılmadı; aday değişiklik Bekleyen kararlar ekranına alındı.",
                      );
                      setTab("decisions");
                      return;
                    }
                    if (receipt.status === "queued") {
                      setNotice(
                        "Değişiklik şifreli yerel günlüğe kaydedildi; sunucuya gönderilmeyi bekliyor.",
                      );
                    } else if (receipt.status !== "applied")
                      throw new Error(
                        "Kayıt sonucu doğrulanamadı; yeniden denemeden önce sunucudaki durumu kontrol et.",
                      );
                    await loadVault(vault!);
                  }
                  if (dialog === "identity") {
                    const value = secret;
                    const passphrase = keyPassphrase;
                    setKeyPassphrase("");
                    setSecret("");
                    await rpc({
                      kind: "saveCredential",
                      id: crypto.randomUUID(),
                      expectedRevision: 0,
                      credentialKind,
                      certificate: credentialKind === "ssh_key" ? keyCertificate : null,
                      label: name,
                      secret: value,
                      passphrase:
                        credentialKind === "ssh_key" ? passphrase : null,
                    });
                    setKeyCertificate("");
                    setIdentities(await rpc({ kind: "credentials" }));
                  }
                  closeDialog();
                });
              }}
            >
              {dialog === "resource" ? (
                <>
                  <label>
                    Ad
                    <input
                      required
                      value={draft!.name}
                      onChange={(e) =>
                        setDraft({ ...draft!, name: e.target.value })
                      }
                    />
                  </label>
                  {draft!.kind === "host" && (
                    <>
                      <label>
                        Adres
                        <input
                          required
                          value={draft!.address ?? ""}
                          onChange={(e) =>
                            setDraft({ ...draft!, address: e.target.value })
                          }
                        />
                      </label>
                      <label>
                        Protokol
                        <select value={draft!.protocol ?? "ssh"} onChange={e => setDraft({...draft!, protocol: e.target.value})}>
                          <option value="ssh">SSH</option>
                          <option value="mosh">Mosh (SSH + UDP)</option>
                        </select>
                      </label>
                      {draft!.protocol === "mosh" && <small>Mosh sunucusu ve hedefe doğrudan UDP erişimi gerekir. Chain yalnız SSH kontrol bağlantısını taşır.</small>}
                      <label>
                        SSH portu
                        <input
                          required
                          type="number"
                          min={1}
                          max={65535}
                          value={draft!.port ?? 22}
                          onChange={(e) =>
                            setDraft({
                              ...draft!,
                              port: Number(e.target.value),
                            })
                          }
                        />
                      </label>
                      <fieldset className="ts-proxy">
                        <legend>Proxy bağlantısı</legend>
                        <label>Proxy türü<select aria-label="Proxy türü" value={draft!.proxy?.kind ?? "none"} onChange={e => setDraft({...draft!,proxy:e.target.value === "none" ? null : {kind:e.target.value as "http"|"socks5",host:draft!.proxy?.host ?? "",port:draft!.proxy?.port ?? 1080,authenticate:draft!.proxy?.authenticate ?? false}})}>
                          <option value="none">Doğrudan bağlantı</option><option value="socks5">SOCKS5</option><option value="http">HTTP CONNECT</option>
                        </select></label>
                        {draft!.proxy && <>
                          <label>Proxy adresi<input aria-label="Proxy adresi" required value={draft!.proxy.host} onChange={e=>setDraft({...draft!,proxy:{...draft!.proxy!,host:e.target.value}})}/></label>
                          <label>Proxy portu<input aria-label="Proxy portu" type="number" required min={1} max={65535} value={draft!.proxy.port} onChange={e=>setDraft({...draft!,proxy:{...draft!.proxy!,port:Number(e.target.value)}})}/></label>
                          <label className="ts-proxy-auth"><input type="checkbox" checked={draft!.proxy.authenticate} onChange={e=>setDraft({...draft!,proxy:{...draft!.proxy!,authenticate:e.target.checked}})}/>Proxy kullanıcı adı ve parolasını bağlantıda sor</label>
                          <small>Proxy bu makineye giden adımda kullanılır. Önceki jump host varsa proxy bağlantısı o host üzerinden açılır.</small>
                        </>}
                      </fieldset>
                      <label>
                        Kullanıcı adı politikası
                        <select
                          value={draft!.usernamePolicy ?? "personal"}
                          onChange={(e) =>
                            setDraft({
                              ...draft!,
                              usernamePolicy: e.target.value,
                              username: null,
                            })
                          }
                        >
                          <option value="personal">
                            Kişisel kullanıcı adına izin ver
                          </option>
                          <option value="shared_required">
                            Ortak kullanıcı adını zorunlu tut
                          </option>
                        </select>
                      </label>
                      {draft!.usernamePolicy === "shared_required" && (
                        <label>
                          Ortak kullanıcı adı
                          <input
                            required
                            value={draft!.username ?? ""}
                            onChange={(e) =>
                              setDraft({ ...draft!, username: e.target.value })
                            }
                          />
                        </label>
                      )}
                    </>
                  )}
                </>
              ) : (
                <label>
                  {dialog === "member" ? "Görünen ad" : "Ad"}
                  <input
                    required
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                  />
                </label>
              )}
              {dialog === "member" && (
                <>
                  <label>
                    Kullanıcı adı
                    <input
                      required
                      pattern="[a-zA-Z0-9_.@-]{1,120}"
                      value={userId}
                      onChange={(e) => setUserId(e.target.value)}
                    />
                  </label>
                  <label>
                    Başlangıç rolü
                    <select
                      value={role}
                      onChange={(e) => setRole(e.target.value)}
                    >
                      {["observer", "operator", "editor"].map((r) => (
                        <option value={r} key={r}>
                          {labels[r]}
                        </option>
                      ))}
                    </select>
                  </label>
                  <small>
                    Rol tek başına host erişimi vermez. İzinleri üye
                    etkinleştikten sonra ata.
                  </small>
                </>
              )}
              {dialog === "identity" && (
                <>
                  <label>
                    Yöntem
                    <select aria-label="Yöntem"
                      value={credentialKind}
                      onChange={(e) => { setCredentialKind(e.target.value); setSecret(""); }}
                    >
                      <option value="ssh_key">SSH özel anahtarı</option>
                      <option value="password">Parola</option>
                      <option value="agent">Cihazdaki SSH agent</option>
                    </select>
                  </label>
                  {credentialKind === "agent" ? <TeamAgent onChange={setSecret} /> : <label>
                    {credentialKind === "ssh_key"
                      ? "Özel anahtar (PEM/OpenSSH)"
                      : "Parola"}
                    {credentialKind === "ssh_key" ? (
                      <textarea
                        required
                        rows={6}
                        spellCheck={false}
                        autoComplete="off"
                        value={secret}
                        onChange={(e) => setSecret(e.target.value)}
                      />
                    ) : (
                      <input
                        required
                        type="password"
                        autoComplete="new-password"
                        value={secret}
                        onChange={(e) => setSecret(e.target.value)}
                      />
                    )}
                  </label>}
                  {credentialKind === "ssh_key" && <label>SSH kullanıcı sertifikası (varsa)<textarea rows={3} spellCheck={false} aria-label="SSH kullanıcı sertifikası" value={keyCertificate} onChange={e=>setKeyCertificate(e.target.value)} /></label>}
                  {credentialKind === "ssh_key" && (
                    <label>
                      Anahtar parolası (varsa)
                      <input
                        type="password"
                        autoComplete="off"
                        value={keyPassphrase}
                        onChange={(e) => setKeyPassphrase(e.target.value)}
                      />
                    </label>
                  )}
                </>
              )}
              <button disabled={busy} className="ts-primary">
                Kaydet
              </button>
            </form>
          </section>
        </div>
      )}
      {sharedRecoveryId && <TeamSharedRecovery id={sharedRecoveryId} onClose={() => setSharedRecoveryId(null)} onSaved={() => void run(async () => {setIdentities(await rpc({kind:"credentials"}));setNotice("Ortak kimlik yeni revizyon olarak kurtarıldı.");})} />}
      {rotationOpen && <TeamRotation onClose={() => setRotationOpen(false)} />}
      {recoveryOpen && (
        <div className="ts-overlay">
          <section
            className="ts-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Şirket kurtarma anahtarı"
          >
            <h2>Şirket kurtarma anahtarı</h2>
            <p>
              Özel anahtar parola ile şifreli bir dosyada sende kalır. Sunucuya
              yalnız public key gönderilir. Bu dosyayı güvenli biçimde yedekle.
            </p>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const password = recoveryPassword,
                  reauth = reauthentication;
                setRecoveryPassword("");
                setReauthentication("");
                void run(async () => {
                  await rpc({
                    kind: "createRecovery",
                    path: recoveryPath,
                    password,
                    reauthentication: reauth,
                  });
                  setRecoveryOpen(false);
                  setNotice(
                    "Şirket kurtarma dosyası doğrulandı ve public key sunucuya kaydedildi.",
                  );
                });
              }}
            >
              <label>
                Dosya
                <input required readOnly value={recoveryPath} />
              </label>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void saveFile("ttrecovery", "Team.ttrecovery")
                    .then((p) => {
                      if (p) setRecoveryPath(p);
                    })
                    .catch((e) => setError(errorText(e)))
                }
              >
                Dosya konumu seç
              </button>
              <label>
                Kurtarma dosyası parolası
                <input
                  required
                  minLength={12}
                  type="password"
                  autoComplete="new-password"
                  value={recoveryPassword}
                  onChange={(e) => setRecoveryPassword(e.target.value)}
                />
              </label>
              <label>
                Team giriş parolan (tekrar doğrulama)
                <input
                  required
                  type="password"
                  autoComplete="current-password"
                  value={reauthentication}
                  onChange={(e) => setReauthentication(e.target.value)}
                />
              </label>
              <button disabled={busy || !recoveryPath} className="ts-primary">
                Dosyayı oluştur ve kaydet
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  setRecoveryOpen(false);
                  setRecoveryPassword("");
                  setReauthentication("");
                }}
              >
                İptal
              </button>
            </form>
          </section>
        </div>
      )}
      {profileEditor && (
        <TeamProfile
          host={
            profileEditor === "default"
              ? null
              : {
                  id: profileEditor.id,
                  name: profileEditor.content!.name,
                  proxy: profileEditor.content!.proxy,
                  usernamePolicy: profileEditor.content!.usernamePolicy,
                  username: profileEditor.content!.username,
                }
          }
          onClose={() => setProfileEditor(null)}
        />
      )}
      {historyResource && (
        <TeamHistory
          id={historyResource.id}
          name={historyResource.content!.name}
          onClose={() => setHistoryResource(null)}
          onChanged={() => {
            if (vault) void run(() => loadVault(vault));
          }}
        />
      )}
      {memberAction && (
        <div className="ts-overlay">
          <section
            className="ts-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Hesap işlemi"
          >
            <h2>{memberAction.member.username}</h2>
            <p>
              {memberAction.action === "resetMember"
                ? "Geçici parola üretilecek ve açık Team oturumları kapatılacak. Eski şifreli kimlikler korunur; yeni girişten sonra ayrıca kurtarma gerekir."
                : "Hesap devre dışı bırakılacak; Team oturumları iptal edilecek. Şifreli kayıtlar silinmez."}
            </p>
            <div className="ts-actions">
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const result = await rpc({
                      kind: memberAction.action,
                      accountId: memberAction.member.accountId,
                    });
                    const reset = memberAction.action === "resetMember";
                    setMemberAction(null);
                    if (reset) setTemporary(result.temporaryPassword);
                    await refresh();
                  })
                }
              >
                İşlemi uygula
              </button>
              <button disabled={busy} onClick={() => setMemberAction(null)}>
                Vazgeç
              </button>
            </div>
          </section>
        </div>
      )}
      {temporary && (
        <div className="ts-overlay">
          <section
            className="ts-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Geçici parola"
          >
            <h2>Geçici parola</h2>
            <p>
              24 saat geçerli. Kullanıcı ilk girişte değiştirecek. Bu pencere
              kapandığında parola tekrar gösterilmez.
            </p>
            <input
              aria-label="Geçici parola"
              readOnly
              value={temporary}
              onFocus={(e) => e.currentTarget.select()}
            />
            <button onClick={() => setTemporary("")}>Kaydettim, kapat</button>
          </section>
        </div>
      )}
      {accessResource && (
        <div className="ts-overlay">
          <section
            className="ts-dialog ts-wide"
            role="dialog"
            aria-modal="true"
            aria-label="Üye izinleri"
          >
            <h2>{accessResource.content?.name} · Üye izinleri</h2>
            {!preview ? (
              <>
                <label>
                  Üye
                  <select
                    value={target}
                    onChange={(e) => setTarget(e.target.value)}
                  >
                    <option value="">Üye seç</option>
                    {members
                      .filter(
                        (m) =>
                          m.status === "active" &&
                          m.role !== "owner" &&
                          m.accountId !== overview?.accountId,
                      )
                      .map((m) => (
                        <option value={m.accountId} key={m.accountId}>
                          {m.displayName} ({m.username})
                        </option>
                      ))}
                  </select>
                </label>
                <div className="ts-permissions">
                  {manageable(accessResource).map((p) => (
                    <label key={p}>
                      {labels[p]}
                      <select
                        value={effects[p]}
                        onChange={(e) =>
                          setEffects({ ...effects, [p]: e.target.value })
                        }
                      >
                        {["allow", "deny", "clear"].map((effect) => (
                          <option key={effect} value={effect}>
                            {labels[effect]}
                          </option>
                        ))}
                      </select>
                    </label>
                  ))}
                </div>
                <p>
                  Klasör izni alt kaynaklara yayılır. Açık engel devralınan
                  izinden önceliklidir.
                </p>
                <button
                  className="ts-primary"
                  disabled={busy || !target}
                  onClick={() => void run(prepareAccess)}
                >
                  Değişikliği önizle
                </button>
              </>
            ) : (
              <>
                <p>
                  {preview.affectedResources} kayıt etkileniyor;{" "}
                  {preview.requiredKeys} şifreleme anahtarı paylaşım için
                  hazırlanacak.
                </p>
                <table>
                  <thead>
                    <tr>
                      <th>Kaynak</th>
                      <th>Önce</th>
                      <th>Sonra</th>
                    </tr>
                  </thead>
                  <tbody>
                    {impact.map((i) => (
                      <tr key={i.id}>
                        <td>
                          {rows.find((r) => r.id === i.id)?.content?.name ??
                            i.id}
                        </td>
                        <td>
                          {i.before.map((p) => labels[p]).join(", ") ||
                            "Erişim yok"}
                        </td>
                        <td>
                          {i.after.map((p) => labels[p]).join(", ") ||
                            "Erişim yok"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <button
                  disabled={busy}
                  className="ts-primary"
                  onClick={() => void run(applyAccess)}
                >
                  Anahtarları hazırla ve izinleri uygula
                </button>
              </>
            )}
            <button disabled={busy} onClick={() => void run(closeAccess)}>
              İptal
            </button>
          </section>
        </div>
      )}
    </div>
  );
}
