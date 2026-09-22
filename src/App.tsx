import { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Terminal,
  Server,
  Folder,
  KeyRound,
  Code2,
  ArrowLeftRight,
  FileText,
  Settings as SettingsIcon,
  Search,
  Plus,
  LayoutGrid,
  List,
  ChevronDown,
  ChevronRight,
  MoreHorizontal,
  LockKeyhole,
  ShieldCheck,
  HardDrive,
  Link,
  ArrowUpRight,
  Copy,
  Trash2,
  X,
  Play,
  Square,
  Columns2,
  Maximize2,
  Radio,
  Save,
  FolderOpen,
  Upload,
  Download,
  FileArchive,
  Check,
  Monitor,
  Box,
  Wifi,
  ArrowRight,
  RefreshCw,
  Keyboard,
  Info,
} from "lucide-react";
import {
  call,
  desktop,
  errorText,
  prepareEvents,
  onSession,
  sessionState,
  forgetSession,
  clearSessions,
} from "./api";
import {
  newEntity,
  type Entity,
  type EntityKind,
  type Vault,
  type AppInfo,
  type Session,
  type Prompt,
} from "./types";
import {
  Modal,
  Field,
  Select,
  Check as Checkbox,
  Busy,
  Empty,
  Pill,
} from "./components";
import Onboarding from "./Onboarding";
import Editor from "./Editor";
import TerminalPane from "./TerminalPane";
import { InputQueue } from "./inputQueue";
import {
  indexHostFolders,
  hostInFolder,
  storedGroupId,
  UNGROUPED_FOLDER,
} from "./hostFolders";
import { shortcutFor, shortcutLabel } from "./shortcuts";
import Sftp from "./Sftp";
import Settings from "./Settings";
import DataTools, { type DataMode } from "./DataTools";
import { t } from "./i18n";
import SharedTerminal from "./SharedTerminal";
type Nav =
  | "hosts"
  | "sftp"
  | "credential"
  | "knownHost"
  | "snippet"
  | "tunnel"
  | "log"
  | "workspace"
  | "settings"
  | "terminal";
const navItems = [
  { id: "hosts", label: t("nav.hosts"), icon: Server },
  { id: "sftp", label: t("nav.sftp"), icon: ArrowLeftRight },
  { id: "credential", label: t("nav.keychain"), icon: KeyRound },
  { id: "snippet", label: t("nav.snippets"), icon: Code2 },
  { id: "tunnel", label: t("nav.tunnels"), icon: Link },
  { id: "workspace", label: t("nav.workspaces"), icon: Columns2 },
  { id: "log", label: t("nav.logs"), icon: FileText },
] as const;
const navLabels: Record<string, string> = {
  hosts: "Hosts",
  credential: "Keychain",
  knownHost: "Known hosts",
  snippet: "Snippets",
  tunnel: "Port forwarding",
  workspace: "Workspaces",
  log: "Session logs",
  settings: "Settings",
  sftp: "SFTP",
  terminal: "Terminal",
};
const iconFor = (kind: string) =>
  kind === "credential"
    ? KeyRound
    : kind === "snippet"
      ? Code2
      : kind === "tunnel"
        ? Link
        : kind === "workspace"
          ? Columns2
          : kind === "knownHost"
            ? ShieldCheck
            : kind === "log"
              ? FileText
              : Server;
export default function App() {
  const [vault, setVault] = useState<Vault | null>(null);
  const [info, setInfo] = useState<AppInfo>();
  const [nav, setNav] = useState<Nav>("hosts");
  const [sftpVisited, setSftpVisited] = useState(false);
  const [group, setGroup] = useState("");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [view, setView] = useState<"grid" | "list">("grid");
  const [editor, setEditor] = useState<Entity | null>(null);
  const [dataMode, setDataMode] = useState<DataMode | null>(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [menu, setMenu] = useState(false);
  const [shareOpen, setShareOpen] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [deleting, setDeleting] = useState<string[] | null>(null);
  const [moving, setMoving] = useState(false);
  const [moveTo, setMoveTo] = useState("");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [active, setActive] = useState("");
  const [layout, setLayout] = useState<"focus" | "split">("focus");
  const [broadcast, setBroadcast] = useState(false);
  const [prompts, setPrompts] = useState<Prompt[]>([]);
  const [answers, setAnswers] = useState<string[]>([]);
  const [runningTunnels, setRunningTunnels] = useState<string[]>([]);
  const [fontSize, setFontSize] = useState(14);
  const [themeId, setThemeId] = useState("graphite");
  const [collapsed, setCollapsed] = useState<string[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const sessionsRef = useRef(sessions);
  sessionsRef.current = sessions;
  const previousVault = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (previousVault.current && previousVault.current !== vault?.id) {
      clearSessions();
      setSessions([]);
      setPrompts([]);
      setRunningTunnels([]);
    }
    previousVault.current = vault?.id;
  }, [vault?.id]);
  const records = vault?.records ?? [];
  const notify = useCallback((s: string) => {
    setNotice(s);
    setTimeout(() => setNotice(""), 5500);
  }, []);
  const report = (e: unknown) => setError(errorText(e));
  useEffect(() => {
    if (desktop) {
      void call<AppInfo>("app_info").then(setInfo).catch(report);
      void prepareEvents();
    }
    const un = onSession((e) => {
      if (e.kind !== "data") {
        setSessions((s) =>
          s.map((session) =>
            session.id === e.id
              ? { ...session, ...sessionState.get(e.id) }
              : session,
          ),
        );
        if (e.kind === "closed")
          void call<Vault>("vault_info")
            .then(setVault)
            .catch(() => {});
      }
    });
    let unsub: Promise<() => void> | undefined;
    if (desktop)
      unsub = listen<Prompt>("session-prompt", ({ payload }) => {
        setPrompts((p) => [...p, payload]);
      });
    return () => {
      un();
      void unsub?.then((f) => f());
    };
  }, []);
  useEffect(() => {
    if (!desktop) return;
    const un = listen("vault-changed", () => {
      void call<Vault>("vault_info")
        .then(setVault)
        .catch(() => {});
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);
  useEffect(() => {
    setAnswers([]);
  }, [prompts[0]?.id]);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!vault) return;
      const inTerminal = !!(e.target as HTMLElement)?.closest?.(
        ".terminal-pane",
      );
      const action = shortcutFor(e, inTerminal);
      if (action === "hosts") {
        e.preventDefault();
        setNav("hosts");
        searchRef.current?.focus();
      }
      if (action === "newHost") {
        e.preventDefault();
        create("host");
      }
      if (action === "local") {
        e.preventDefault();
        void connect();
      }
      if (action === "lock") {
        e.preventDefault();
        void lock();
      }
      if (action === "previous" || action === "next") {
        e.preventDefault();
        const index = sessions.findIndex((s) => s.id === active);
        if (sessions.length)
          setActive(
            sessions[
              (index + (action === "next" ? 1 : -1) + sessions.length) %
                sessions.length
            ].id,
          );
        setNav("terminal");
      }
      if (action === "broadcast") {
        e.preventDefault();
        setBroadcast((b) => !b);
      }
      if (e.key === "Escape" && !inTerminal) {
        setEditor(null);
        setMenu(false);
        setSelected([]);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [vault, active, sessions, info, group]);
  useEffect(() => {
    if (
      !import.meta.env.DEV ||
      !new URLSearchParams(location.search).has("preview") ||
      desktop
    )
      return;
    const g = newEntity("group", { label: "Production", color: "purple" });
    const hosts = [
      ["web-01", "10.0.1.12", "ubuntu", ["production", "web"]],
      ["api-server", "10.0.1.24", "deploy", ["production", "api"]],
      ["PostgreSQL", "10.0.2.10", "postgres", ["database"]],
      ["Development", "dev.example.net", "alex", ["development"]],
      ["Bastion", "bastion.example.net", "admin", ["gateway"]],
      ["Raspberry Pi", "192.168.1.42", "pi", ["homelab"]],
    ].map(([label, address, username, tags], i) =>
      newEntity("host", {
        label,
        address,
        username,
        tags,
        port: 22,
        protocol: "ssh",
        groupId: i < 3 ? g.id : "",
      }),
    );
    const count = Math.min(
      10000,
      Number(new URLSearchParams(location.search).get("previewCount") ?? 6),
    );
    for (let i = hosts.length; i < count; i++)
      hosts.push(
        newEntity("host", {
          label: `Benchmark host ${i}`,
          address: `10.${Math.floor(i / 65536)}.${Math.floor(i / 256) % 256}.${i % 256}`,
          username: "benchmark",
          tags: ["performance"],
          protocol: "ssh",
          port: 22,
        }),
      );
    const previewGroups = [g];
    if (new URLSearchParams(location.search).has("previewNested")) {
      const parent = newEntity("group", { label: "Environments" });
      g.data.groupId = parent.id;
      previewGroups.unshift(parent);
    }
    setVault({
      id: "preview",
      name: "Personal vault · UI preview",
      path: "No file is open",
      deviceId: "preview",
      records: [...previewGroups, ...hosts],
    });
    setInfo({ home: "C:\\Users", defaultVaultPath: "", version: "0.1.0" });
  }, []);
  function navigate(n: Nav) {
    if (n === "sftp") setSftpVisited(true);
    setNav(n);
    setSearch("");
    setSelected([]);
    setEditor(null);
    setMenu(false);
  }
  function create(kind: EntityKind) {
    const defaults: Record<string, any> = {
      host: {
        label: "",
        address: "",
        protocol: "ssh",
        groupId: storedGroupId(group),
        tags: [],
      },
      group: { label: "", groupId: storedGroupId(group) },
      credential: { label: "", username: "" },
      snippet: { label: "", command: "", newline: true },
      tunnel: {
        label: "",
        mode: "local",
        bindAddress: "127.0.0.1",
        bindPort: 8080,
        targetAddress: "127.0.0.1",
        targetPort: 80,
      },
      workspace: {
        label: "",
        hostIds: sessions.filter((s) => s.hostId).map((s) => s.hostId),
        statsEnabled: sessions
          .filter((s) => s.hostId)
          .map((s) => s.statsEnabled !== false),
        layout,
      },
    };
    setEditor(newEntity(kind, defaults[kind] ?? {}));
  }
  async function save(r: Entity) {
    setVault(await call<Vault>("records_save", { records: [r] }));
    setEditor(null);
    notify(`${r.data.label} saved.`);
  }
  async function connect(hostId?: string, shell?: string, statsEnabled = true) {
    try {
      await prepareEvents();
      const id = await call<string>("session_start", {
        hostId: hostId ?? null,
        shell:
          shell ??
          (records.find((r) => r.kind === "settings")?.data.localShell ||
            info?.platform?.defaultShell) ??
          null,
      });
      const label =
        records.find((r) => r.id === hostId)?.data.label ??
        shell ??
        "Local terminal";
      setSessions((s) => [
        ...s,
        {
          id,
          label,
          hostId,
          status: "Connecting…",
          connected: false,
          closed: false,
          statsEnabled,
          ...sessionState.get(id),
        },
      ]);
      setActive(id);
      setNav("terminal");
      setEditor(null);
    } catch (e) {
      report(e);
    }
  }
  async function closeSession(id: string) {
    inputQueue.current.cancel(id);
    try {
      await call("session_input", { id, close: true });
    } catch {}
    forgetSession(id);
    setSessions((s) => s.filter((s) => s.id !== id));
    if (active === id) setActive(sessions.find((s) => s.id !== id)?.id ?? "");
  }
  const inputQueue = useRef(
    new InputQueue((id, data) => call("session_input", { id, data })),
  );
  function sendInput(id: string, data: string) {
    const targets = broadcast
      ? sessions.filter((s) => s.connected).map((s) => s.id)
      : [id];
    for (const target of targets) inputQueue.current.send(target, data, report);
  }
  async function lock() {
    inputQueue.current.cancel();
    try {
      await call("vault_lock");
      clearSessions();
      setSessions([]);
      setVault(null);
      setEditor(null);
      setPrompts([]);
      setNav("hosts");
      setRunningTunnels([]);
    } catch (e) {
      report(e);
    }
  }
  async function duplicate(ids: string[]) {
    try {
      const copies = records
        .filter((r) => ids.includes(r.id))
        .map((r) =>
          newEntity(r.kind, { ...r.data, label: `${r.data.label} copy` }),
        );
      setVault(await call<Vault>("records_save", { records: copies }));
      setSelected([]);
      notify(`${copies.length} copies created.`);
    } catch (e) {
      report(e);
    }
  }
  async function remove() {
    if (!deleting) return;
    try {
      setVault(await call<Vault>("records_delete", { ids: deleting }));
      setEditor(null);
      setSelected([]);
      setDeleting(null);
    } catch (e) {
      report(e);
      setDeleting(null);
    }
  }
  async function tunnel(record: Entity) {
    if (runningTunnels.includes(record.id)) {
      await call("tunnel_stop", { id: record.id });
      setRunningTunnels((ids) => ids.filter((id) => id !== record.id));
    } else {
      notify("Connecting tunnel…");
      await call("tunnel_start", { id: record.id, tunnel: record.data });
      setRunningTunnels((ids) => [...ids, record.id]);
      notify("Port forwarding is running.");
    }
  }
  function runSnippet(r: Entity) {
    const target = active || sessions.find((s) => s.connected)?.id;
    if (!target) {
      notify("Open a terminal before running a snippet.");
      return;
    }
    sendInput(target, r.data.command + (r.data.newline !== false ? "\r" : ""));
    setNav("terminal");
  }
  async function openWorkspace(r: Entity) {
    for (const [index, id] of (r.data.hostIds ?? []).slice(0, 16).entries())
      await connect(id, undefined, r.data.statsEnabled?.[index] !== false);
    setLayout(r.data.layout ?? "split");
  }
  const searchIndex = useMemo(
    () =>
      new Map(
        records.map((r) => [
          r.id,
          [
            r.data.label,
            r.data.address,
            r.data.username,
            ...(r.data.tags ?? []),
            r.kind === "log" ? r.data.content : "",
          ]
            .join(" ")
            .toLocaleLowerCase(),
        ]),
      ),
    [records],
  );
  const normalizedSearch = search.toLocaleLowerCase();
  const filtered = useMemo(
    () =>
      records.filter(
        (r) =>
          (nav === "hosts" ? r.kind === "host" : r.kind === nav) &&
          (nav !== "hosts" || hostInFolder(r, group)) &&
          (!search || searchIndex.get(r.id)?.includes(normalizedSearch)),
      ),
    [records, nav, group, normalizedSearch, searchIndex],
  );
  useEffect(() => setPage(0), [search, nav, group]);
  useEffect(() => {
    const settings = records.find((r) => r.kind === "settings");
    if (settings?.data.fontSize) setFontSize(settings.data.fontSize);
    setThemeId(settings?.data.terminalTheme ?? "graphite");
  }, [vault?.id, records.find((r) => r.kind === "settings")?.updatedAt]);
  const folders = useMemo(() => indexHostFolders(records), [records]);
  const groups = folders.groups;
  const currentGroup = folders.byId.get(group);
  const groupTrail: Entity[] = [];
  for (
    let current = currentGroup;
    current && !groupTrail.some((g) => g.id === current?.id);
    current = folders.byId.get(current?.data.groupId)
  ) {
    groupTrail.unshift(current);
  }
  function openGroup(id: string) {
    setGroup(id);
    setSelected([]);
    setEditor(null);
    setSearch("");
    setPage(0);
  }
  const childGroups = (folders.children.get(group) ?? []).filter(
    (r) =>
      !search ||
      String(r.data.label).toLowerCase().includes(search.toLowerCase()),
  );
  function groupTree(
    parent = "",
    depth = 0,
    seen = new Set<string>(),
  ): React.ReactNode {
    if (depth > 15) return null;
    return (folders.children.get(parent) ?? [])
      .filter((g) => !seen.has(g.id))
      .map((g) => (
        <div key={g.id}>
          <button
            className={
              "sidebar-group " +
              (group === g.id && nav === "hosts" ? "active" : "")
            }
            style={{ paddingLeft: 16 + depth * 12 }}
            onClick={() => {
              setGroup(g.id);
              navigate("hosts");
            }}
          >
            <span
              role="button"
              aria-label="Toggle group"
              onClick={(e) => {
                e.stopPropagation();
                setCollapsed((a) =>
                  a.includes(g.id)
                    ? a.filter((id) => id !== g.id)
                    : [...a, g.id],
                );
              }}
            >
              {collapsed.includes(g.id) ? (
                <ChevronRight size={12} />
              ) : (
                <ChevronDown size={12} />
              )}
            </span>
            <Folder size={15} />
            <span className="sidebar-group-label">{g.data.label}</span>
            <small
              className="sidebar-group-count"
              title="Hosts including subfolders"
              aria-label={`${folders.totalHosts.get(g.id) ?? 0} hosts including subfolders`}
            >
              {folders.totalHosts.get(g.id) ?? 0}
            </small>
          </button>
          {!collapsed.includes(g.id) &&
            groupTree(g.id, depth + 1, new Set([...seen, g.id]))}
        </div>
      ));
  }
  if (!vault)
    return (
      <>
        <Onboarding
          info={info}
          onOpen={setVault}
          onRestore={() => setDataMode("restore")}
        />
        {dataMode && (
          <DataTools
            mode={dataMode}
            vault={vault}
            onClose={() => setDataMode(null)}
            onVault={setVault}
            onNotice={notify}
          />
        )}{" "}
        {error && (
          <div className="toast error" onClick={() => setError("")}>
            {error}
            <X size={15} />
          </div>
        )}
      </>
    );
  return (
    <div className="app">
      <header className="topbar">
        <div className="brand compact">
          <div className="brand-mark">
            <Terminal size={20} />
          </div>
          TermTerm
        </div>
        <div className="top-tabs">
          <button
            className={nav !== "terminal" && nav !== "sftp" ? "active" : ""}
            onClick={() => navigate("hosts")}
          >
            <HardDrive size={14} />
            Vault
          </button>
          <button
            className={nav === "sftp" ? "active" : ""}
            onClick={() => navigate("sftp")}
          >
            <ArrowLeftRight size={14} />
            SFTP
          </button>
          {sessions.length > 0 && (
            <button
              className={nav === "terminal" ? "active" : ""}
              onClick={() => setNav("terminal")}
            >
              <Terminal size={14} />
              Terminals<span className="tab-count">{sessions.length}</span>
            </button>
          )}
          <button
            className="plus-tab"
            title="New local terminal"
            onClick={() => void connect()}
          >
            <Plus size={16} />
          </button>
        </div>
        <div className="top-actions">
          <span className="local-badge">
            <span className="status-dot live" />
            Local vault
          </span>
          <button
            className="icon-btn"
            title="Lock vault"
            onClick={() => void lock()}
          >
            <LockKeyhole size={17} />
          </button>
          <div className="avatar tiny">{vault.name[0].toUpperCase()}</div>
        </div>
      </header>
      <div className="app-body">
        <aside className="sidebar">
          <button className="vault-selector" onClick={() => setMenu(!menu)}>
            <div className="vault-icon">
              <Box size={18} />
            </div>
            <span>
              <strong>{vault.name}</strong>
              <small>Encrypted workspace</small>
            </span>
            <ChevronDown size={14} />
          </button>
          {menu && (
            <div className="vault-menu">
              {[
                ["import", "Import connections", Upload],
                ["export", "Export connections", Download],
                ["backup", "Create backup", FileArchive],
                ["portable", "Save portable copy", Copy],
                ["restore", "Restore backup", FolderOpen],
              ].map(([id, label, Icon]) => {
                const I = Icon as typeof Upload;
                return (
                  <button
                    key={id as string}
                    onClick={() => {
                      setDataMode(id as DataMode);
                      setMenu(false);
                    }}
                  >
                    <I size={15} />
                    {label as string}
                  </button>
                );
              })}
              <button
                onClick={() => {
                  setShareOpen(true);
                  setMenu(false);
                }}
              >
                <Link size={15} />
                Shared terminals
              </button>
              <button onClick={() => void lock()}>
                <LockKeyhole size={15} />
                Lock / switch vault
              </button>
            </div>
          )}
          <div className="nav-label">WORKSPACE</div>
          <nav className="main-nav">
            {navItems.map((n) => (
              <button
                key={n.id}
                className={nav === n.id ? "active" : ""}
                onClick={() => {
                  if (n.id === "hosts") setGroup("");
                  navigate(n.id);
                }}
              >
                <n.icon size={17} />
                <span>{n.label}</span>
                {n.id === "hosts" && (
                  <small title="Total hosts in this vault, including all groups">
                    {folders.hostCount}
                  </small>
                )}
              </button>
            ))}
          </nav>
          <div className="nav-label with-action">
            GROUPS
            <button
              className="icon-btn"
              title="New group"
              onClick={() => create("group")}
            >
              <Plus size={13} />
            </button>
          </div>
          <div className="group-tree">
            {(folders.children.get("")?.length ?? 0) > 0 ? (
              groupTree()
            ) : (
              <p className="sidebar-empty">
                Organize your connections
                <br />
                with groups.
              </p>
            )}
          </div>
          <div className="sidebar-bottom">
            <button onClick={() => void connect()}>
              <Terminal size={17} />
              Local terminal<kbd>{shortcutLabel("local")}</kbd>
            </button>
            <button
              className={nav === "knownHost" ? "active" : ""}
              onClick={() => navigate("knownHost")}
            >
              <ShieldCheck size={17} />
              Known hosts
            </button>
            <button
              className={nav === "settings" ? "active" : ""}
              onClick={() => navigate("settings")}
            >
              <SettingsIcon size={17} />
              Settings
            </button>
            <div className="vault-status">
              <ShieldCheck size={13} />
              <span>Saved locally · encrypted</span>
              <i />
            </div>
          </div>
        </aside>
        <main className={"main-content " + (editor ? "with-editor" : "")}>
          <div
            hidden={nav !== "sftp"}
            style={{
              height: "100%",
              display: nav === "sftp" ? "block" : "none",
            }}
          >
            {sftpVisited && (
              <Sftp
                key={vault?.id}
                records={records}
                home={info?.home ?? ""}
                notify={notify}
                visible={nav === "sftp"}
              />
            )}
          </div>
          {nav === "settings" && (
            <Settings
              vault={vault}
              onVault={setVault}
              notify={notify}
              fontSize={fontSize}
              onFontSize={setFontSize}
              themeId={themeId}
              onTheme={setThemeId}
            />
          )}
          <div
            className="terminal-workspace"
            style={{ display: nav === "terminal" ? "flex" : "none" }}
          >
            <div className="workspace-toolbar">
              <div className="terminal-tabs">
                {sessions.map((s) => (
                  <button
                    key={s.id}
                    className={active === s.id ? "active" : ""}
                    onClick={() => setActive(s.id)}
                  >
                    <span
                      className={"status-dot " + (s.connected ? "live" : "")}
                    />
                    {s.label}
                    <span
                      role="button"
                      aria-label={`Close ${s.label}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        void closeSession(s.id);
                      }}
                    >
                      <X size={12} />
                    </span>
                  </button>
                ))}
              </div>
              <div className="button-row">
                <button
                  className="icon-btn"
                  title="Shared terminals"
                  onClick={() => setShareOpen(true)}
                >
                  <Link size={16} />
                </button>
                <button
                  className={"icon-btn " + (broadcast ? "broadcast-on" : "")}
                  title="Broadcast input to all connected terminals"
                  onClick={() => setBroadcast(!broadcast)}
                >
                  <Radio size={16} />
                </button>
                <button
                  className="icon-btn"
                  title={
                    layout === "split" ? "Focus terminal" : "Split terminals"
                  }
                  onClick={() =>
                    setLayout(layout === "split" ? "focus" : "split")
                  }
                >
                  {layout === "split" ? (
                    <Maximize2 size={16} />
                  ) : (
                    <Columns2 size={16} />
                  )}
                </button>
                <button
                  className="icon-btn"
                  title="Save workspace"
                  onClick={() => create("workspace")}
                >
                  <Save size={16} />
                </button>
                <button
                  className="icon-btn"
                  title={`Add local terminal (${shortcutLabel("local")})`}
                  onClick={() => void connect()}
                >
                  <Plus size={17} />
                </button>
              </div>
            </div>
            {broadcast && (
              <div className="broadcast-bar">
                <Radio size={14} />
                Broadcast is on. Your input goes to all connected terminals.
                <button
                  className="text-btn"
                  onClick={() => setBroadcast(false)}
                >
                  Turn off
                </button>
              </div>
            )}
            <div className={"terminal-grid " + layout}>
              {sessions.map((s) => (
                <div
                  className="terminal-cell"
                  key={s.id}
                  style={{
                    display:
                      layout === "focus" && s.id !== active ? "none" : "flex",
                  }}
                >
                  <TerminalPane
                    session={s}
                    focused={s.id === active}
                    onFocus={() => setActive(s.id)}
                    onClose={() => void closeSession(s.id)}
                    onInput={sendInput}
                    fontSize={fontSize}
                    themeId={themeId}
                    visible={
                      nav === "terminal" &&
                      (layout === "split" || s.id === active)
                    }
                    onStats={(enabled) =>
                      setSessions((all) =>
                        all.map((item) =>
                          item.id === s.id
                            ? { ...item, statsEnabled: enabled }
                            : item,
                        ),
                      )
                    }
                    onError={report}
                  />
                </div>
              ))}
              {!sessions.length && (
                <Empty
                  icon={<Terminal size={30} />}
                  title="A fresh terminal awaits"
                  detail="Connect to a host or start a local shell."
                >
                  <button className="primary" onClick={() => void connect()}>
                    <Plus size={16} />
                    Open local terminal
                  </button>
                </Empty>
              )}
            </div>
          </div>
          {!["terminal", "sftp", "settings"].includes(nav) && (
            <div className="records-page">
              <div className="breadcrumb">
                <button
                  onClick={() => {
                    setGroup("");
                    navigate("hosts");
                  }}
                >
                  <HardDrive size={13} />
                  {vault.name}
                </button>
                <ChevronRight size={12} />
                {nav === "hosts" ? (
                  <>
                    <button onClick={() => openGroup("")}>Hosts</button>
                    {groupTrail.map((g) => (
                      <span className="folder-crumb" key={g.id}>
                        <ChevronRight size={12} />
                        <button onClick={() => openGroup(g.id)}>
                          {g.data.label}
                        </button>
                      </span>
                    ))}
                  </>
                ) : (
                  <span>{navLabels[nav]}</span>
                )}
              </div>
              <div className="page-heading">
                <div>
                  <span className="eyebrow">
                    {nav === "hosts" ? "YOUR CONNECTIONS" : "YOUR WORKSPACE"}
                  </span>
                  <h1>
                    {nav === "hosts"
                      ? (currentGroup?.data.label ?? "Hosts")
                      : navLabels[nav]}
                  </h1>
                  <p>
                    {
                      {
                        hosts: currentGroup
                          ? "Connections and subfolders in this group."
                          : "Open a folder to view its hosts. Connections without a group are in Ungrouped.",
                        credential:
                          "Your identities and keys. Encrypted and ready.",
                        knownHost: "The servers you have chosen to trust.",
                        snippet: "Useful commands, always at hand.",
                        tunnel: "Secure paths to the services you need.",
                        workspace: "Pick up where you left off.",
                        log: "A record of your terminal sessions.",
                      }[nav as string]
                    }
                  </p>
                </div>
                <div className="button-row">
                  {nav === "hosts" && group !== UNGROUPED_FOLDER && (
                    <button
                      className="secondary"
                      onClick={() => create("group")}
                    >
                      <Folder size={16} />
                      New group
                    </button>
                  )}
                  {!["log", "knownHost"].includes(nav) && (
                    <button
                      className="primary"
                      onClick={() =>
                        create(nav === "hosts" ? "host" : (nav as EntityKind))
                      }
                    >
                      <Plus size={17} />
                      {nav === "hosts"
                        ? "New host"
                        : nav === "credential"
                          ? "New identity"
                          : nav === "tunnel"
                            ? "New rule"
                            : nav === "snippet"
                              ? "New snippet"
                              : "New workspace"}
                    </button>
                  )}
                </div>
              </div>
              <div className="records-toolbar">
                <div className="search-field">
                  <Search size={17} />
                  <input
                    ref={searchRef}
                    aria-label="Search records"
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    placeholder={
                      nav === "hosts"
                        ? "Search this folder…"
                        : `Search ${navLabels[nav].toLowerCase()}…`
                    }
                  />
                  <kbd>{shortcutLabel("hosts")}</kbd>
                  {search && (
                    <button className="icon-btn" onClick={() => setSearch("")}>
                      <X size={14} />
                    </button>
                  )}
                </div>
                <div className="toolbar-right">
                  <span>
                    {nav === "hosts"
                      ? group
                        ? `${filtered.length} hosts in this folder · ${childGroups.length} folders`
                        : `${childGroups.length} folders · ${folders.hostCount} hosts total`
                      : `${filtered.length} items`}
                  </span>
                  <div className="view-toggle">
                    <button
                      className={view === "grid" ? "active" : ""}
                      title="Card view"
                      onClick={() => setView("grid")}
                    >
                      <LayoutGrid size={16} />
                    </button>
                    <button
                      className={view === "list" ? "active" : ""}
                      title="List view"
                      onClick={() => setView("list")}
                    >
                      <List size={17} />
                    </button>
                  </div>
                  <button
                    className="icon-btn"
                    title="Import connections"
                    onClick={() => setDataMode("import")}
                  >
                    <Upload size={17} />
                  </button>
                </div>
              </div>
              {selected.length > 0 && (
                <div className="selection-bar">
                  <Checkbox
                    checked={selected.length === filtered.length}
                    onChange={(v) =>
                      setSelected(v ? filtered.map((r) => r.id) : [])
                    }
                  >
                    {selected.length} selected
                  </Checkbox>
                  <button
                    className="text-btn"
                    onClick={() => void duplicate(selected)}
                  >
                    <Copy size={14} />
                    Duplicate
                  </button>
                  {nav === "hosts" && (
                    <button
                      className="text-btn"
                      onClick={() => {
                        setMoveTo("");
                        setMoving(true);
                      }}
                    >
                      <Folder size={14} />
                      Move
                    </button>
                  )}
                  <button
                    className="text-btn danger"
                    onClick={() => setDeleting(selected)}
                  >
                    <Trash2 size={14} />
                    Delete
                  </button>
                  <button className="icon-btn" onClick={() => setSelected([])}>
                    <X size={15} />
                  </button>
                </div>
              )}
              {nav === "hosts" && childGroups.length > 0 && (
                <div className="groups-grid">
                  {childGroups.map((g) => (
                    <div
                      className="group-card"
                      key={g.id}
                      onDoubleClick={() => {
                        openGroup(g.id);
                      }}
                    >
                      <button
                        className="group-main"
                        onClick={() => {
                          openGroup(g.id);
                        }}
                      >
                        <div className="folder-square">
                          <Folder size={23} />
                        </div>
                        <div>
                          <strong>{g.data.label}</strong>
                          <small title="Host count includes every subfolder">
                            {folders.totalHosts.get(g.id) ?? 0} hosts
                            {" · "}
                            {folders.children.get(g.id)?.length ?? 0} folders
                          </small>
                        </div>
                        <ChevronRight size={16} />
                      </button>
                      {g.id !== UNGROUPED_FOLDER && (
                        <button
                          className="icon-btn"
                          title={`Edit ${g.data.label}`}
                          onClick={() => setEditor(g)}
                        >
                          <MoreHorizontal size={18} />
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              )}
              {!filtered.length && (nav !== "hosts" || !childGroups.length) ? (
                <Empty
                  icon={(() => {
                    const I = iconFor(nav);
                    return <I size={32} />;
                  })()}
                  title={
                    search
                      ? "No matching records"
                      : nav === "hosts"
                        ? "Your next connection starts here"
                        : `No ${navLabels[nav].toLowerCase()} yet`
                  }
                  detail={
                    search
                      ? "Try a different name, address or tag."
                      : nav === "hosts"
                        ? "Add your first host, or import the connections you already use."
                        : nav === "log"
                          ? "Session output is encrypted and saved when a terminal closes."
                          : "Create a record to start organizing your workspace."
                  }
                >
                  {!search && nav === "hosts" && (
                    <div className="button-row">
                      <button
                        className="primary"
                        onClick={() => create("host")}
                      >
                        <Plus size={16} />
                        New host
                      </button>
                      <button
                        className="secondary"
                        onClick={() => setDataMode("import")}
                      >
                        <Upload size={16} />
                        Import hosts
                      </button>
                    </div>
                  )}
                </Empty>
              ) : (
                <div className={"records-grid " + view}>
                  {filtered.slice(page * 120, (page + 1) * 120).map((r) => {
                    const Icon = iconFor(r.kind);
                    const isSelected = selected.includes(r.id);
                    return (
                      <article
                        key={r.id}
                        className={
                          "record-card " +
                          (isSelected ? "selected " : "") +
                          (editor?.id === r.id ? "editing" : "")
                        }
                        onClick={(e) => {
                          if (e.ctrlKey || e.metaKey) {
                            setSelected((s) =>
                              s.includes(r.id)
                                ? s.filter((id) => id !== r.id)
                                : [...s, r.id],
                            );
                          } else setEditor(r);
                        }}
                        onDoubleClick={() => {
                          if (r.kind === "host") void connect(r.id);
                        }}
                      >
                        <div className="card-top">
                          <div
                            className={
                              "host-icon " +
                              (r.kind === "host"
                                ? "tone-" + (r.data.label?.length % 5)
                                : "")
                            }
                          >
                            <Icon size={23} />
                          </div>
                          <div className="card-title">
                            <h3 title={r.data.label}>
                              {r.data.label || r.data.address}
                            </h3>
                            <span>
                              {r.kind === "host"
                                ? `${r.data.username ? r.data.username + "@" : ""}${r.data.address}`
                                : r.kind === "credential"
                                  ? r.data.username || "SSH identity"
                                  : r.kind === "tunnel"
                                    ? `${r.data.bindAddress}:${r.data.bindPort}`
                                    : r.kind === "log"
                                      ? r.data.endedAt
                                          ?.slice(0, 16)
                                          .replace("T", " ")
                                      : r.kind === "knownHost"
                                        ? r.data.fingerprint
                                        : r.kind === "workspace"
                                          ? `${r.data.hostIds?.length ?? 0} terminals`
                                          : r.data.package || "Command snippet"}
                            </span>
                          </div>
                          <div className="card-menu">
                            <input
                              type="checkbox"
                              aria-label={`Select ${r.data.label}`}
                              checked={isSelected}
                              onClick={(e) => e.stopPropagation()}
                              onChange={() =>
                                setSelected((s) =>
                                  isSelected
                                    ? s.filter((id) => id !== r.id)
                                    : [...s, r.id],
                                )
                              }
                            />
                            <button
                              className="icon-btn"
                              title={`Edit ${r.data.label}`}
                              onClick={(e) => {
                                e.stopPropagation();
                                setEditor(r);
                              }}
                            >
                              <MoreHorizontal size={18} />
                            </button>
                          </div>
                        </div>
                        {r.kind === "snippet" && (
                          <code className="snippet-preview">
                            {r.data.command?.split("\n")[0]}
                          </code>
                        )}
                        <div className="card-tags">
                          {(r.data.tags ?? []).slice(0, 3).map((t: string) => (
                            <span className="tag" key={t}>
                              {t}
                            </span>
                          ))}
                          {r.data.chain?.length > 0 && (
                            <span className="chain-tag">
                              <Link size={11} />
                              {r.data.chain.length} hop
                              {r.data.chain.length > 1 ? "s" : ""}
                            </span>
                          )}
                        </div>
                        <footer>
                          <span className="protocol-label">
                            {r.kind === "host"
                              ? `${(r.data.protocol ?? "SSH").toUpperCase()} · ${r.data.port ?? 22}`
                              : r.kind === "credential"
                                ? r.data.privateKey
                                  ? "SSH KEY"
                                  : "PASSWORD"
                                : r.kind === "tunnel"
                                  ? `${r.data.mode?.toUpperCase()} FORWARDING`
                                  : r.kind === "knownHost"
                                    ? "TRUSTED KEY"
                                    : r.kind === "log"
                                      ? "ENCRYPTED LOG"
                                      : r.kind === "workspace"
                                        ? "SAVED LAYOUT"
                                        : "SHELL COMMAND"}
                          </span>
                          {["host", "snippet", "tunnel", "workspace"].includes(
                            r.kind,
                          ) && (
                            <button
                              className="card-connect"
                              onClick={(e) => {
                                e.stopPropagation();
                                if (r.kind === "host") void connect(r.id);
                                if (r.kind === "snippet") runSnippet(r);
                                if (r.kind === "tunnel")
                                  void tunnel(r).catch(report);
                                if (r.kind === "workspace")
                                  void openWorkspace(r);
                              }}
                            >
                              {r.kind === "tunnel" &&
                              runningTunnels.includes(r.id) ? (
                                <>
                                  <Square size={12} />
                                  Stop
                                </>
                              ) : (
                                <>
                                  {r.kind === "host"
                                    ? "Connect"
                                    : r.kind === "workspace"
                                      ? "Open"
                                      : "Run"}
                                  <ArrowUpRight size={15} />
                                </>
                              )}
                            </button>
                          )}
                        </footer>
                      </article>
                    );
                  })}
                </div>
              )}
              {filtered.length > 120 && (
                <div className="button-row pagination">
                  <button
                    className="secondary"
                    disabled={page === 0}
                    onClick={() => setPage((p) => p - 1)}
                  >
                    Previous
                  </button>
                  <span>
                    {page + 1} / {Math.ceil(filtered.length / 120)} ·{" "}
                    {filtered.length} records
                  </span>
                  <button
                    className="secondary"
                    disabled={(page + 1) * 120 >= filtered.length}
                    onClick={() => setPage((p) => p + 1)}
                  >
                    Next
                  </button>
                </div>
              )}
              {nav === "hosts" && (
                <div className="workspace-hint">
                  <Keyboard size={15} />
                  <span>Double-click a host to connect</span>
                  <i>·</i>
                  <span>Ctrl + click to select multiple</span>
                  <button onClick={() => setDataMode("import")}>
                    Bring your existing hosts
                    <ArrowRight size={13} />
                  </button>
                </div>
              )}
            </div>
          )}
        </main>
        {editor && (
          <Editor
            key={editor.id}
            record={editor}
            records={records}
            onClose={() => setEditor(null)}
            onSave={save}
            onDelete={(id) => setDeleting([id])}
            onConnect={(id) => void connect(id)}
          />
        )}
      </div>
      {dataMode && (
        <DataTools
          mode={dataMode}
          vault={vault}
          onClose={() => setDataMode(null)}
          onVault={setVault}
          onNotice={notify}
        />
      )}
      {shareOpen && (
        <SharedTerminal
          records={records}
          sessions={sessions}
          active={active}
          onClose={() => setShareOpen(false)}
          onJoin={(session) => {
            setSessions((a) => [...a, session]);
            setActive(session.id);
            setNav("terminal");
          }}
        />
      )}
      {deleting && (
        <Modal title="Delete records" onClose={() => setDeleting(null)}>
          <div className="modal-body">
            <p>
              Delete {deleting.length} selected record
              {deleting.length === 1 ? "" : "s"} from this vault?
            </p>
            <p className="muted small">
              Records used by a host, group or chain must be unlinked first.
            </p>
          </div>
          <footer>
            <button className="secondary" onClick={() => setDeleting(null)}>
              Cancel
            </button>
            <button className="danger-button" onClick={() => void remove()}>
              Delete
            </button>
          </footer>
        </Modal>
      )}
      {moving && (
        <Modal title="Move hosts" onClose={() => setMoving(false)}>
          <div className="modal-body">
            <Select
              label="Destination group"
              value={moveTo}
              onChange={setMoveTo}
              options={[
                { value: "", label: "No group" },
                ...groups.map((g) => ({ value: g.id, label: g.data.label })),
              ]}
            />
          </div>
          <footer>
            <button className="secondary" onClick={() => setMoving(false)}>
              Cancel
            </button>
            <button
              className="primary"
              onClick={async () => {
                try {
                  setVault(
                    await call<Vault>("records_save", {
                      records: records
                        .filter((r) => selected.includes(r.id))
                        .map((r) => ({
                          ...r,
                          data: { ...r.data, groupId: moveTo },
                        })),
                    }),
                  );
                  setMoving(false);
                  setSelected([]);
                } catch (e) {
                  report(e);
                }
              }}
            >
              Move hosts
            </button>
          </footer>
        </Modal>
      )}
      {prompts[0] && (
        <Modal
          title={
            prompts[0].kind === "hostKey"
              ? "Verify server identity"
              : prompts[0].detail.name || "Authentication required"
          }
          onClose={() => {
            void call("prompt_answer", { id: prompts[0].id, answers: [] });
            setPrompts((p) => p.slice(1));
          }}
        >
          <div className="modal-body">
            {prompts[0].kind === "hostKey" ? (
              <>
                <div className="trust-icon">
                  <ShieldCheck size={32} />
                </div>
                <p>
                  First connection to{" "}
                  <strong>{prompts[0].detail.address}</strong>.
                </p>
                <p className="muted">
                  Compare this fingerprint with a trusted source before saving
                  the server key.
                </p>
                <div className="fingerprint">
                  {prompts[0].detail.fingerprint}
                </div>
                <span className="pill">{prompts[0].detail.algorithm}</span>
              </>
            ) : (
              <>
                <p>{prompts[0].detail.instructions}</p>
                {prompts[0].detail.prompts?.map((p, i) => (
                  <Field
                    key={i}
                    label={p.prompt}
                    value={answers[i] ?? ""}
                    onChange={(v) =>
                      setAnswers((a) => {
                        const n = [...a];
                        n[i] = v;
                        return n;
                      })
                    }
                    type={p.echo ? "text" : "password"}
                  />
                ))}
              </>
            )}
          </div>
          <footer>
            <button
              className="secondary"
              onClick={() => {
                void call("prompt_answer", { id: prompts[0].id, answers: [] });
                setPrompts((p) => p.slice(1));
              }}
            >
              Cancel
            </button>
            <button
              className="primary"
              onClick={() => {
                void call("prompt_answer", {
                  id: prompts[0].id,
                  answers:
                    prompts[0].kind === "hostKey"
                      ? ["trust"]
                      : (prompts[0].detail.prompts ?? []).map(
                          (_, i) => answers[i] ?? "",
                        ),
                });
                setAnswers([]);
                setPrompts((p) => p.slice(1));
              }}
            >
              {prompts[0].kind === "hostKey" ? "Trust and connect" : "Continue"}
            </button>
          </footer>
        </Modal>
      )}
      {(error || notice) && (
        <div
          className={"toast " + (error ? "error" : "")}
          role="status"
          onClick={() => {
            setError("");
            setNotice("");
          }}
        >
          {error ? <Info size={18} /> : <Check size={18} />}
          <span>{error || notice}</span>
          <button className="icon-btn" aria-label="Dismiss message">
            <X size={15} />
          </button>
        </div>
      )}
    </div>
  );
}
