import { tr } from "./i18n";
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
  Pencil,
  Scissors,
  ClipboardPaste,
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
import { shortcutFor, shortcutLabel, isTextEditing } from "./shortcuts";
import { copyText, pasteTextField } from "./clipboard";
import ContextMenu, {
  menuPosition,
  type MenuAction,
  type MenuPosition,
} from "./ContextMenu";
import RecordDetails from "./RecordDetails";
import {
  containedRecords,
  duplicateRecords,
  moveRecords,
  connectionDetails,
} from "./recordActions";
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
  const [starting, setStarting] = useState(desktop);
  const startup = useRef<Promise<{
    info: AppInfo;
    vault: Vault | null;
    error?: string;
  }> | null>(null);
  const [nav, setNav] = useState<Nav>("hosts");
  const [sftpVisited, setSftpVisited] = useState(false);
  const [settingsTarget, setSettingsTarget] = useState({ tab: "sync", key: 0 });
  const topTabs = useRef<HTMLDivElement>(null);
  const [group, setGroup] = useState("");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [view, setView] = useState<"grid" | "list">("grid");
  const [editor, setEditor] = useState<Entity | null>(null);
  const [details, setDetails] = useState<Entity | null>(null);
  const [context, setContext] = useState<{
    position: MenuPosition;
    title: string;
    actions: MenuAction[];
  } | null>(null);
  const [recordClipboard, setRecordClipboard] = useState<{
    ids: string[];
    cut: boolean;
    vaultId: string;
  } | null>(null);
  const [operationBusy, setOperationBusy] = useState(false);
  const operationLock = useRef(false);
  const vaultId = useRef(vault?.id);
  vaultId.current = vault?.id;
  const [sftpRequest, setSftpRequest] = useState<{
    hostId: string;
    key: number;
  } | null>(null);
  const [backupIds, setBackupIds] = useState<string[]>([]);
  const selectionAnchor = useRef<string | undefined>(undefined);
  const [dataMode, setDataMode] = useState<DataMode | null>(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [menu, setMenu] = useState(false);
  const [shareOpen, setShareOpen] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [deleting, setDeleting] = useState<string[] | null>(null);
  const [moving, setMoving] = useState<{ ids: string[]; copy: boolean } | null>(
    null,
  );
  const [moveTo, setMoveTo] = useState("");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [active, setActive] = useState("");
  const [layout, setLayout] = useState<"focus" | "split">("focus");
  const [broadcast, setBroadcast] = useState(false);
  const [prompts, setPrompts] = useState<Prompt[]>([]);
  const [answers, setAnswers] = useState<string[]>([]);
  const [runningTunnels, setRunningTunnels] = useState<string[]>([]);
  const [fontSize, setFontSize] = useState(15);
  const [themeId, setThemeId] = useState("graphite");
  const [collapsed, setCollapsed] = useState<string[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const sessionsRef = useRef(sessions);
  sessionsRef.current = sessions;
  const previousVault = useRef<string | undefined>(undefined);
  useEffect(() => {
    setContext(null);
    setDetails(null);
    setRecordClipboard(null);
    setMoving(null);
    setDeleting(null);
    setBackupIds([]);
  }, [vault?.id]);
  useEffect(() => {
    setContext(null);
    setDetails(null);
  }, [nav, group]);
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
  const recordsById = useMemo(
    () => new Map(records.map((r) => [r.id, r])),
    [records],
  );
  const notify = useCallback((s: string) => {
    setNotice(s);
    setTimeout(() => setNotice(""), 5500);
  }, []);
  const report = (e: unknown) => setError(errorText(e));
  useEffect(() => {
    if (vault)
      setInfo((current) =>
        current ? { ...current, recent: { path: vault.path } } : current,
      );
  }, [vault?.path]);
  useEffect(() => {
    let current = true;
    if (desktop) {
      // Share the startup request across StrictMode's effect replay. Locking the
      // vault later never reruns this automatic unlock.
      startup.current ??= (async () => {
        const info = await call<AppInfo>("app_info");
        const active = await call<Vault>("vault_info").catch(() => null);
        if (active || !info.recent?.path) return { info, vault: active };
        try {
          return {
            info,
            vault: await call<Vault | null>("vault_try_open_remembered", {
              path: info.recent.path,
            }),
          };
        } catch (e) {
          return {
            info,
            vault: null,
            error:
              tr(
                "Could not use the remembered password. Enter your vault password to continue.",
              ) +
              " " +
              errorText(e),
          };
        }
      })();
      void startup.current
        .then((result) => {
          if (!current) return;
          setInfo(result.info);
          if (result.vault) setVault(result.vault);
          if (result.error) setError(result.error);
        })
        .catch((e) => {
          if (current) report(e);
        })
        .finally(() => {
          if (current) {
            // The resolved promise contains plaintext vault records. Release it
            // after startup so an explicit lock can release that data as well.
            startup.current = null;
            setStarting(false);
          }
        });
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
      current = false;
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
      const target = e.target;
      if (
        e.defaultPrevented ||
        (target instanceof Element && target.closest('[role="menu"]'))
      )
        return;
      const editing =
        isTextEditing(target) && !(target as Element).closest(".xterm");
      const inTerminal = !editing && nav === "terminal";
      if (
        editing &&
        e.code === "Insert" &&
        e.shiftKey &&
        !e.ctrlKey &&
        !e.metaKey &&
        !e.altKey &&
        !e.isComposing &&
        (target instanceof HTMLTextAreaElement ||
          (target instanceof HTMLInputElement &&
            ["text", "search", "url", "tel", "password"].includes(
              target.type,
            ))) &&
        !target.readOnly &&
        !target.disabled
      ) {
        e.preventDefault();
        if (!e.repeat) void pasteTextField(target).catch(report);
        return;
      }
      const action = shortcutFor(e, inTerminal, undefined, editing);
      if (action && e.repeat) {
        e.preventDefault();
        return;
      }
      if (action === "close" && active) {
        e.preventDefault();
        void closeSession(active);
      }
      if (action === "tabNext" || action === "tabPrevious") {
        e.preventDefault();
        const tabs = ["vault", "sftp", ...sessions.map((s) => s.id)];
        const current =
          nav === "terminal" ? active : nav === "sftp" ? "sftp" : "vault";
        const next =
          tabs[
            (tabs.indexOf(current) +
              (action === "tabNext" ? 1 : -1) +
              tabs.length) %
              tabs.length
          ];
        if (next === "vault") navigate("hosts");
        else if (next === "sftp") navigate("sftp");
        else {
          setActive(next);
          setNav("terminal");
        }
      }
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
      if (e.key === "Escape" && !inTerminal && !editing) {
        setEditor(null);
        setDetails(null);
        setMenu(false);
        setSelected([]);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [vault, active, sessions, info, group, nav]);
  useEffect(() => {
    topTabs.current
      ?.querySelector(".active")
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active, nav, sessions.length]);
  useEffect(() => {
    if (
      !import.meta.env.DEV ||
      !new URLSearchParams(location.search).has("preview") ||
      desktop
    )
      return;
    const g = newEntity("group", { label: tr("Production") });
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
      const parent = newEntity("group", { label: tr("Environments") });
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
    setDetails(null);
    setContext(null);
    setMenu(false);
  }
  function create(kind: EntityKind) {
    setDetails(null);
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
    setDetails(r);
    notify(tr("{name} saved.", { name: r.data.label }));
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
        tr("Local terminal");
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
      setDetails(null);
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
    if (active === id) {
      const index = sessions.findIndex((s) => s.id === id);
      const remaining = sessions.filter((s) => s.id !== id);
      setActive(remaining[Math.min(index, remaining.length - 1)]?.id ?? "");
      if (!remaining.length && nav === "terminal") setNav("hosts");
    }
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
    await mutateRecords(
      () => duplicateRecords(records, ids),
      "Copies created.",
    );
  }
  async function remove() {
    if (!deleting || operationLock.current) return;
    operationLock.current = true;
    setOperationBusy(true);
    const currentVault = vaultId.current;
    try {
      const next = await call<Vault>("records_delete", { ids: deleting });
      if (vaultId.current !== currentVault) return;
      setVault(next);
      if (deleting.includes(group)) setGroup("");
      setEditor(null);
      setDetails(null);
      setSelected([]);
      setDeleting(null);
    } catch (e) {
      report(e);
    } finally {
      operationLock.current = false;
      setOperationBusy(false);
    }
  }
  async function mutateRecords(build: () => Entity[], message: string) {
    if (operationLock.current) return false;
    operationLock.current = true;
    setOperationBusy(true);
    const currentVault = vaultId.current;
    try {
      const changes = build();
      if (!changes.length) throw Error("No records to change.");
      const next = await call<Vault>("records_save", { records: changes });
      if (vaultId.current !== currentVault) return false;
      setVault(next);
      setSelected([]);
      setDetails(null);
      setEditor(null);
      setMoving(null);
      notify(message);
      return true;
    } catch (e) {
      report(e);
      return false;
    } finally {
      operationLock.current = false;
      setOperationBusy(false);
    }
  }
  function showDetails(record: Entity) {
    setEditor(null);
    setDetails(record);
  }
  function editRecord(record: Entity) {
    setDetails(null);
    setEditor(record);
  }
  function showMenu(
    e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>,
    title: string,
    actions: MenuAction[],
  ) {
    e.preventDefault();
    e.stopPropagation();
    setContext({
      position: menuPosition(e),
      title,
      actions: actions.map((a) => ({
        ...a,
        disabled: a.disabled || operationLock.current,
      })),
    });
  }
  function openSftp(hostId: string) {
    setSftpRequest({ hostId, key: Date.now() });
    navigate("sftp");
  }
  function chooseRecord(
    e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>,
    r: Entity,
  ) {
    const visible = [...childGroups, ...filtered]
      .filter((item) => item.id !== UNGROUPED_FOLDER)
      .map((item) => item.id);
    if (r.id === UNGROUPED_FOLDER) {
      setSelected([r.id]);
      selectionAnchor.current = r.id;
      showDetails(r);
      return;
    }
    if (
      e.shiftKey &&
      selectionAnchor.current &&
      visible.includes(selectionAnchor.current)
    ) {
      const start = visible.indexOf(selectionAnchor.current),
        end = visible.indexOf(r.id);
      setSelected(
        visible.slice(Math.min(start, end), Math.max(start, end) + 1),
      );
    } else if (e.ctrlKey || e.metaKey) {
      setSelected((s) =>
        s.includes(r.id)
          ? s.filter((id) => id !== r.id)
          : [...s.filter((id) => id !== UNGROUPED_FOLDER), r.id],
      );
      selectionAnchor.current = r.id;
    } else {
      setSelected([r.id]);
      selectionAnchor.current = r.id;
    }
    showDetails(r);
  }
  function recordKey(e: React.KeyboardEvent<HTMLElement>, r: Entity) {
    if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
      recordMenu(e, r);
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      chooseRecord(e, r);
    }
  }
  function sessionMenu(
    e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>,
    session: Session,
  ) {
    const actions: MenuAction[] = [
      {
        id: "focus",
        label: tr("Focus terminal"),
        icon: <Maximize2 size={15} />,
        run: () => {
          setActive(session.id);
          navigate("terminal");
          setLayout("focus");
        },
      },
      {
        id: "split",
        label: tr("Show split view"),
        icon: <Columns2 size={15} />,
        run: () => {
          setActive(session.id);
          navigate("terminal");
          setLayout("split");
        },
      },
    ];
    if (session.hostId)
      actions.push({
        id: "duplicate",
        label: tr("New connection to this host"),
        icon: <Terminal size={15} />,
        run: () => connect(session.hostId),
      });
    actions.push({
      id: "copy",
      label: tr("Copy tab name"),
      icon: <Copy size={15} />,
      separator: true,
      run: () => copyText(session.label),
    });
    actions.push({
      id: "close",
      label: tr("Close terminal"),
      icon: <X size={15} />,
      separator: true,
      run: () => closeSession(session.id),
    });
    actions.push({
      id: "others",
      label: tr("Close other terminals"),
      disabled: sessions.length < 2,
      run: async () => {
        for (const other of sessions.filter((s) => s.id !== session.id))
          await closeSession(other.id);
        setActive(session.id);
        setNav("terminal");
      },
    });
    showMenu(e, session.label, actions);
  }
  async function connectMany(ids: string[]) {
    const hosts = containedRecords(records, ids).filter(
      (r) => r.kind === "host",
    );
    const available = Math.max(0, 16 - sessions.length);
    if (!available) {
      notify(
        tr("Close a terminal before opening another (16 panels maximum)."),
      );
      return;
    }
    for (const host of hosts.slice(0, available)) await connect(host.id);
    if (hosts.length > available)
      notify(
        tr("{count} hosts opened; the workspace limit is 16 terminals.", {
          count: available,
        }),
      );
  }
  async function pasteRecords(destination: string) {
    const clipboard = recordClipboard;
    if (!clipboard || clipboard.vaultId !== vault?.id) return;
    const saved = await mutateRecords(
      () =>
        clipboard.cut
          ? moveRecords(records, clipboard.ids, destination)
          : duplicateRecords(records, clipboard.ids, destination),
      clipboard.cut ? "Records moved." : "Copies created.",
    );
    if (saved && clipboard.cut) setRecordClipboard(null);
  }
  function recordMenu(
    e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>,
    record: Entity,
  ) {
    const ids = selected.includes(record.id) ? selected : [record.id];
    setSelected(ids);
    const chosen = ids
      .map((id) => recordsById.get(id))
      .filter((r): r is Entity => !!r);
    const single = ids.length === 1,
      virtual = record.id === UNGROUPED_FOLDER;
    const movable =
      !virtual && chosen.every((r) => ["host", "group"].includes(r.kind));
    const targets = containedRecords(records, ids),
      hosts = targets.filter((r) => r.kind === "host");
    const data = connectionDetails(record, records);
    const actions: MenuAction[] = [];
    if (single && record.kind === "group")
      actions.push({
        id: "open",
        label: tr("Open group"),
        icon: <FolderOpen size={15} />,
        run: () => openGroup(record.id),
      });
    if (hosts.length)
      actions.push({
        id: "connect",
        label:
          single && record.kind === "host"
            ? tr("Connect")
            : tr("Quick connect hosts"),
        icon: <Terminal size={15} />,
        run: () => connectMany(ids),
      });
    if (
      single &&
      record.kind === "host" &&
      ["ssh", "mosh"].includes(data.protocol ?? "ssh")
    )
      actions.push({
        id: "sftp",
        label: tr("Open SFTP"),
        icon: <ArrowLeftRight size={15} />,
        run: () => openSftp(record.id),
      });
    if (single && record.kind === "snippet")
      actions.push({
        id: "run",
        label: tr("Run in active terminal"),
        icon: <Play size={15} />,
        disabled: !sessions.some((s) => s.connected),
        run: () => runSnippet(record),
      });
    if (single && record.kind === "tunnel")
      actions.push({
        id: "run",
        label: runningTunnels.includes(record.id)
          ? tr("Stop forwarding")
          : tr("Start forwarding"),
        icon: <Link size={15} />,
        run: () => tunnel(record),
      });
    if (single && record.kind === "workspace")
      actions.push({
        id: "open",
        label: tr("Open workspace"),
        icon: <Columns2 size={15} />,
        run: () => openWorkspace(record),
      });
    if (single)
      actions.push({
        id: "details",
        label: tr("Details"),
        icon: <Info size={15} />,
        separator: !!actions.length,
        run: () => showDetails(record),
      });
    if (single && !virtual)
      actions.push({
        id: "edit",
        label: tr("Edit"),
        icon: <Pencil size={15} />,
        run: () => editRecord(record),
      });
    if (!virtual)
      actions.push({
        id: "duplicate",
        label: tr("Duplicate"),
        icon: <Copy size={15} />,
        separator: true,
        run: () => duplicate(ids),
      });
    if (movable) {
      actions.push({
        id: "move",
        label: tr("Move to group…"),
        icon: <Folder size={15} />,
        run: () => {
          setMoveTo("");
          setMoving({ ids, copy: false });
        },
      });
      actions.push({
        id: "copy-to",
        label: tr("Copy to group…"),
        icon: <Copy size={15} />,
        run: () => {
          setMoveTo("");
          setMoving({ ids, copy: true });
        },
      });
      actions.push({
        id: "copy",
        label: tr("Copy"),
        icon: <Copy size={15} />,
        run: () => {
          setRecordClipboard({ ids, cut: false, vaultId: vault!.id });
          notify(
            tr("Records copied. Right-click a destination group to paste."),
          );
        },
      });
      actions.push({
        id: "cut",
        label: tr("Cut"),
        icon: <Scissors size={15} />,
        run: () => {
          setRecordClipboard({ ids, cut: true, vaultId: vault!.id });
          notify(
            tr(
              "Records ready to move. Right-click a destination group to paste.",
            ),
          );
        },
      });
    }
    if (single && record.kind === "group" && recordClipboard)
      actions.push({
        id: "paste",
        label: tr("Paste here"),
        icon: <ClipboardPaste size={15} />,
        run: () => pasteRecords(storedGroupId(record.id)),
      });
    if (single && record.data.address)
      actions.push({
        id: "address",
        label: tr("Copy address"),
        separator: true,
        icon: <Copy size={15} />,
        run: () => copyText(String(record.data.address)),
      });
    if (single && data.username)
      actions.push({
        id: "username",
        label: tr("Copy username"),
        icon: <Copy size={15} />,
        run: () => copyText(String(data.username)),
      });
    if (single && record.kind === "snippet")
      actions.push({
        id: "command",
        label: tr("Copy command"),
        icon: <Copy size={15} />,
        run: () => copyText(record.data.command ?? ""),
      });
    if (single && record.kind === "credential" && record.data.publicKey)
      actions.push({
        id: "key",
        label: tr("Copy public key"),
        icon: <Copy size={15} />,
        run: () => copyText(record.data.publicKey),
      });
    if (single && record.kind === "knownHost")
      actions.push({
        id: "fingerprint",
        label: tr("Copy fingerprint"),
        icon: <Copy size={15} />,
        run: () => copyText(record.data.fingerprint ?? ""),
      });
    if (single && record.kind === "log")
      actions.push({
        id: "log",
        label: tr("Copy log"),
        icon: <Copy size={15} />,
        run: () => copyText(record.data.content ?? ""),
      });
    if (!virtual) {
      actions.push({
        id: "backup",
        label: tr("Export encrypted backup…"),
        icon: <Download size={15} />,
        separator: true,
        run: () => {
          setBackupIds(targets.map((r) => r.id));
          setDataMode("backup");
        },
      });
      actions.push({
        id: "delete",
        label: tr("Delete…"),
        icon: <Trash2 size={15} />,
        danger: true,
        separator: true,
        run: () => setDeleting(targets.map((r) => r.id)),
      });
    }
    showMenu(
      e,
      single
        ? record.data.label
        : tr("{count} selected", { count: ids.length }),
      actions,
    );
  }
  async function tunnel(record: Entity) {
    if (runningTunnels.includes(record.id)) {
      await call("tunnel_stop", { id: record.id });
      setRunningTunnels((ids) => ids.filter((id) => id !== record.id));
    } else {
      notify(tr("Connecting tunnel…"));
      await call("tunnel_start", { id: record.id, tunnel: record.data });
      setRunningTunnels((ids) => [...ids, record.id]);
      notify(tr("Port forwarding is running."));
    }
  }
  function runSnippet(r: Entity) {
    const target =
      sessions.find((s) => s.id === active && s.connected)?.id ||
      sessions.find((s) => s.connected)?.id;
    if (!target) {
      notify(tr("Open a terminal before running a snippet."));
      return;
    }
    sendInput(target, r.data.command + (r.data.newline !== false ? "\r" : ""));
    setActive(target);
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
    setFontSize(settings?.data.fontSize ?? 15);
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
    setNav("hosts");
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
            onContextMenu={(e) => recordMenu(e, g)}
            onKeyDown={(e) => {
              if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey))
                recordMenu(e, g);
            }}
            onClick={() => {
              setGroup(g.id);
              navigate("hosts");
            }}
          >
            <span
              role="button"
              aria-label={tr("Toggle group")}
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
              title={tr("Hosts including subfolders")}
              aria-label={tr("{count} hosts including subfolders", {
                count: folders.totalHosts.get(g.id) ?? 0,
              })}
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
          loading={starting}
          onOpen={setVault}
          onRestore={() => setDataMode("restore")}
        />
        {dataMode && (
          <DataTools
            mode={dataMode}
            recordIds={backupIds}
            vault={vault}
            onClose={() => {
              setDataMode(null);
              setBackupIds([]);
            }}
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
        <div
          className="top-tabs"
          ref={topTabs}
          aria-label={tr("Workspace tabs")}
        >
          <button
            className={nav !== "terminal" && nav !== "sftp" ? "active" : ""}
            onClick={() => navigate("hosts")}
          >
            <HardDrive size={14} />
            {tr("Vault")}
          </button>
          <button
            className={nav === "sftp" ? "active" : ""}
            onClick={() => navigate("sftp")}
          >
            <ArrowLeftRight size={14} />
            SFTP
          </button>
          {sessions.map((s) => (
            <div
              key={s.id}
              onContextMenu={(e) => sessionMenu(e, s)}
              onKeyDown={(e) => {
                if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey))
                  sessionMenu(e, s);
              }}
              className={
                "top-session-tab " +
                (nav === "terminal" && active === s.id ? "active" : "")
              }
            >
              <button
                className="session-tab-label"
                title={s.label}
                aria-pressed={nav === "terminal" && active === s.id}
                onClick={() => {
                  setActive(s.id);
                  setNav("terminal");
                  setEditor(null);
                }}
              >
                <span className={"status-dot " + (s.connected ? "live" : "")} />
                <span>{s.label}</span>
              </button>
              <button
                className="tab-close"
                aria-label={tr("Close tab {name}", { name: s.label })}
                onClick={() => void closeSession(s.id)}
              >
                <X size={13} />
              </button>
            </div>
          ))}
          <button
            className="plus-tab"
            title={tr("New local terminal")}
            onClick={() => void connect()}
          >
            <Plus size={16} />
          </button>
        </div>
        <div className="top-actions">
          <span className="local-badge">
            <span className="status-dot live" />
            {tr("Local vault")}
          </span>
          <button
            className="icon-btn"
            title={tr("Lock vault")}
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
              <small>{tr("Encrypted workspace")}</small>
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
                      setBackupIds([]);
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
                {tr("Shared terminals")}
              </button>
              <button onClick={() => void lock()}>
                <LockKeyhole size={15} />
                {tr("Lock / switch vault")}
              </button>
            </div>
          )}
          <div className="nav-label">{tr("WORKSPACE")}</div>
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
                  <small
                    title={tr(
                      "Total hosts in this vault, including all groups",
                    )}
                  >
                    {folders.hostCount}
                  </small>
                )}
              </button>
            ))}
          </nav>
          <div className="nav-label with-action">
            {tr("GROUPS")}
            <button
              className="icon-btn"
              title={tr("New group")}
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
                {tr("Organize your connections")}
                <br />
                {tr("with groups.")}
              </p>
            )}
          </div>
          <div className="sidebar-bottom">
            <button onClick={() => void connect()}>
              <Terminal size={17} />
              {tr("Local terminal")}
              <kbd>{shortcutLabel("local")}</kbd>
            </button>
            <button
              className={nav === "knownHost" ? "active" : ""}
              onClick={() => navigate("knownHost")}
            >
              <ShieldCheck size={17} />
              {tr("Known hosts")}
            </button>
            <button
              className={nav === "settings" ? "active" : ""}
              onClick={() => {
                setSettingsTarget((s) => ({ tab: "sync", key: s.key + 1 }));
                navigate("settings");
              }}
            >
              <SettingsIcon size={17} />
              {tr("Settings")}
            </button>
            <button
              className="updates-nav"
              onClick={() => {
                setSettingsTarget((s) => ({ tab: "updates", key: s.key + 1 }));
                navigate("settings");
              }}
            >
              <Download size={17} /> {tr("Updates")}{" "}
              <span className="app-version">
                v{info?.version ?? "0.3.3-dev.1"}
              </span>
            </button>
            <div className="vault-status">
              <ShieldCheck size={13} />
              <span>{tr("Saved locally · encrypted")}</span>
              <i />
            </div>
          </div>
        </aside>
        <main
          className={"main-content " + (editor || details ? "with-editor" : "")}
        >
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
                openRequest={sftpRequest}
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
              initialTab={settingsTarget.tab}
              navigationKey={settingsTarget.key}
            />
          )}
          <div
            className="terminal-workspace"
            style={{ display: nav === "terminal" ? "flex" : "none" }}
          >
            {broadcast && (
              <div className="broadcast-bar">
                <Radio size={14} />
                {tr(
                  "Broadcast is on. Your input goes to all connected terminals.",
                )}
                <button
                  className="text-btn"
                  onClick={() => setBroadcast(false)}
                >
                  {tr("Turn off")}
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
                    copyOnSelect={
                      records.find((r) => r.kind === "settings")?.data
                        .copyOnSelect !== false
                    }
                    rightClickPaste={
                      records.find((r) => r.kind === "settings")?.data
                        .rightClickPaste !== false
                    }
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
                  title={tr("A fresh terminal awaits")}
                  detail="Connect to a host or start a local shell."
                >
                  <button className="primary" onClick={() => void connect()}>
                    <Plus size={16} />
                    {tr("Open local terminal")}
                  </button>
                </Empty>
              )}
            </div>
            <div
              className="workspace-toolbar"
              role="toolbar"
              aria-label={tr("Terminal workspace tools")}
            >
              <div className="button-row">
                <button
                  className="icon-btn"
                  title={tr("Shared terminals")}
                  onClick={() => setShareOpen(true)}
                >
                  <Link size={16} />
                </button>
                <button
                  className={"icon-btn " + (broadcast ? "broadcast-on" : "")}
                  title={tr("Broadcast input to all connected terminals")}
                  onClick={() => setBroadcast(!broadcast)}
                >
                  <Radio size={16} />
                </button>
                <button
                  className="icon-btn"
                  title={
                    layout === "split"
                      ? tr("Focus terminal")
                      : tr("Split terminals")
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
                  title={tr("Save workspace")}
                  onClick={() => create("workspace")}
                >
                  <Save size={16} />
                </button>
                <button
                  className="icon-btn"
                  title={tr("Add local terminal ({shortcut})", {
                    shortcut: shortcutLabel("local"),
                  })}
                  onClick={() => void connect()}
                >
                  <Plus size={17} />
                </button>
              </div>
            </div>
          </div>
          {!["terminal", "sftp", "settings"].includes(nav) && (
            <div
              className="records-page"
              onContextMenu={(e) => {
                if (
                  (e.target as HTMLElement).closest(
                    "input,textarea,[data-record]",
                  )
                )
                  return;
                const actions: MenuAction[] =
                  nav === "log"
                    ? []
                    : [
                        {
                          id: "new",
                          label:
                            nav === "hosts" ? tr("New host") : tr("New record"),
                          icon: <Plus size={15} />,
                          run: () =>
                            create(
                              nav === "hosts" ? "host" : (nav as EntityKind),
                            ),
                        },
                      ];
                if (nav === "hosts")
                  actions.push({
                    id: "group",
                    label: tr("New group"),
                    icon: <Folder size={15} />,
                    run: () => create("group"),
                  });
                if (nav === "hosts" && recordClipboard)
                  actions.push({
                    id: "paste",
                    label: tr("Paste here"),
                    icon: <ClipboardPaste size={15} />,
                    run: () => pasteRecords(storedGroupId(group)),
                  });
                actions.push({
                  id: "all",
                  label: tr("Select all"),
                  separator: true,
                  run: () =>
                    setSelected(
                      [
                        ...childGroups.filter((g) => g.id !== UNGROUPED_FOLDER),
                        ...filtered,
                      ].map((r) => r.id),
                    ),
                });
                showMenu(e, "Workspace", actions);
              }}
            >
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
                    <button onClick={() => openGroup("")}>{tr("Hosts")}</button>
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
                  <span>{tr(navLabels[nav])}</span>
                )}
              </div>
              <div className="page-heading">
                <div>
                  <span className="eyebrow">
                    {nav === "hosts"
                      ? tr("YOUR CONNECTIONS")
                      : tr("YOUR WORKSPACE")}
                  </span>
                  <h1>
                    {nav === "hosts"
                      ? (currentGroup?.data.label ?? tr("Hosts"))
                      : tr(navLabels[nav])}
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
                      {tr("New group")}
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
                        ? tr("New host")
                        : nav === "credential"
                          ? tr("New identity")
                          : nav === "tunnel"
                            ? tr("New rule")
                            : nav === "snippet"
                              ? tr("New snippet")
                              : tr("New workspace")}
                    </button>
                  )}
                </div>
              </div>
              <div className="records-toolbar">
                <div className="search-field">
                  <Search size={17} />
                  <input
                    ref={searchRef}
                    aria-label={tr("Search records")}
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    placeholder={
                      nav === "hosts"
                        ? tr("Search this folder…")
                        : tr("Search {type}…", { type: tr(navLabels[nav]) })
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
                        ? tr(
                            "{hosts} hosts in this folder · {folders} folders",
                            {
                              hosts: filtered.length,
                              folders: childGroups.length,
                            },
                          )
                        : tr("{folders} folders · {hosts} hosts total", {
                            folders: childGroups.length,
                            hosts: folders.hostCount,
                          })
                      : tr("{count} items", { count: filtered.length })}
                  </span>
                  <div className="view-toggle">
                    <button
                      className={view === "grid" ? "active" : ""}
                      title={tr("Card view")}
                      onClick={() => setView("grid")}
                    >
                      <LayoutGrid size={16} />
                    </button>
                    <button
                      className={view === "list" ? "active" : ""}
                      title={tr("List view")}
                      onClick={() => setView("list")}
                    >
                      <List size={17} />
                    </button>
                  </div>
                  <button
                    className="icon-btn"
                    title={tr("Import connections")}
                    onClick={() => setDataMode("import")}
                  >
                    <Upload size={17} />
                  </button>
                </div>
              </div>
              {nav === "hosts" && childGroups.length > 0 && (
                <div className="groups-grid">
                  {childGroups.map((g) => (
                    <div
                      className={
                        "group-card " +
                        (selected.includes(g.id) ? "selected" : "")
                      }
                      key={g.id}
                      onContextMenu={(e) => recordMenu(e, g)}
                      onDoubleClick={() => {
                        openGroup(g.id);
                      }}
                    >
                      <button
                        className="group-main"
                        aria-pressed={selected.includes(g.id)}
                        onClick={(e) => chooseRecord(e, g)}
                        onKeyDown={(e) => recordKey(e, g)}
                      >
                        <div className="folder-square">
                          <Folder size={23} />
                        </div>
                        <div>
                          <strong>{g.data.label}</strong>
                          <small
                            title={tr("Host count includes every subfolder")}
                          >
                            {folders.totalHosts.get(g.id) ?? 0} {tr("hosts")}
                            {" · "}
                            {folders.children.get(g.id)?.length ?? 0}{" "}
                            {tr("folders")}
                          </small>
                        </div>
                        <ChevronRight size={16} />
                      </button>
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
                      ? tr("No matching records")
                      : nav === "hosts"
                        ? tr("Your next connection starts here")
                        : tr("No {type} yet", { type: tr(navLabels[nav]) })
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
                        {tr("New host")}
                      </button>
                      <button
                        className="secondary"
                        onClick={() => setDataMode("import")}
                      >
                        <Upload size={16} />
                        {tr("Import hosts")}
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
                        role="button"
                        tabIndex={0}
                        aria-label={r.data.label || r.data.address}
                        aria-pressed={isSelected}
                        onContextMenu={(e) => recordMenu(e, r)}
                        onKeyDown={(e) => recordKey(e, r)}
                        className={
                          "record-card " +
                          (isSelected ? "selected " : "") +
                          (editor?.id === r.id || details?.id === r.id
                            ? "editing"
                            : "")
                        }
                        onClick={(e) => chooseRecord(e, r)}
                        onDoubleClick={() => {
                          if (r.kind === "host") void connect(r.id);
                        }}
                      >
                        <div className="card-top">
                          <div className="host-icon">
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
                                  ? r.data.username || tr("SSH identity")
                                  : r.kind === "tunnel"
                                    ? `${r.data.bindAddress}:${r.data.bindPort}`
                                    : r.kind === "log"
                                      ? r.data.endedAt
                                          ?.slice(0, 16)
                                          .replace("T", " ")
                                      : r.kind === "knownHost"
                                        ? r.data.fingerprint
                                        : r.kind === "workspace"
                                          ? tr("{count} terminals", {
                                              count:
                                                r.data.hostIds?.length ?? 0,
                                            })
                                          : r.data.package ||
                                            tr("Command snippet")}
                            </span>
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
                              {r.data.chain.length} {tr("hop")}
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
                                  ? tr("SSH KEY")
                                  : tr("PASSWORD")
                                : r.kind === "tunnel"
                                  ? tr("Port forwarding")
                                  : r.kind === "knownHost"
                                    ? tr("TRUSTED KEY")
                                    : r.kind === "log"
                                      ? tr("ENCRYPTED LOG")
                                      : r.kind === "workspace"
                                        ? tr("SAVED LAYOUT")
                                        : tr("SHELL COMMAND")}
                          </span>
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
                    {tr("Previous")}
                  </button>
                  <span>
                    {page + 1} / {Math.ceil(filtered.length / 120)} ·{" "}
                    {filtered.length} {tr("records")}
                  </span>
                  <button
                    className="secondary"
                    disabled={(page + 1) * 120 >= filtered.length}
                    onClick={() => setPage((p) => p + 1)}
                  >
                    {tr("Next")}
                  </button>
                </div>
              )}
              {nav === "hosts" && (
                <div className="workspace-hint">
                  <Keyboard size={15} />
                  <span>{tr("Double-click a host to connect")}</span>
                  <i>·</i>
                  <span>
                    {tr(
                      "Right-click for actions · Ctrl / Shift + click to select",
                    )}
                  </span>
                  <button onClick={() => setDataMode("import")}>
                    {tr("Bring your existing hosts")}
                    <ArrowRight size={13} />
                  </button>
                </div>
              )}
            </div>
          )}
        </main>
        {details && !editor && (
          <RecordDetails
            record={records.find((r) => r.id === details.id) ?? details}
            records={records}
            onClose={() => setDetails(null)}
            onContextMenu={(e) => recordMenu(e, details)}
          />
        )}
        {editor && (
          <Editor
            key={editor.id}
            record={editor}
            records={records}
            onClose={() => setEditor(null)}
            onSave={save}
            onConnect={(id) => void connect(id)}
          />
        )}
      </div>
      {dataMode && (
        <DataTools
          mode={dataMode}
          recordIds={backupIds}
          vault={vault}
          onClose={() => {
            setDataMode(null);
            setBackupIds([]);
          }}
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
      {context && (
        <ContextMenu
          key={
            context.position.x + ":" + context.position.y + ":" + context.title
          }
          position={context.position}
          title={context.title}
          actions={context.actions}
          onClose={() => setContext(null)}
          onError={report}
        />
      )}
      {deleting && (
        <Modal
          title={tr("Delete records")}
          onClose={() => {
            if (!operationLock.current) setDeleting(null);
          }}
        >
          <div className="modal-body">
            <p>
              {tr("Delete")} {deleting.length} {tr("record")}
              {deleting.length === 1 ? "" : "s"} {tr("from this vault?")}
            </p>
            <p className="muted small">
              {tr(
                "Selected groups include their hosts and subgroups. Records referenced outside this selection must be unlinked first.",
              )}
            </p>
          </div>
          <footer>
            <button className="secondary" onClick={() => setDeleting(null)}>
              {tr("Cancel")}
            </button>
            <button
              className="danger-button"
              disabled={operationBusy}
              onClick={() => void remove()}
            >
              {tr("Delete")}
            </button>
          </footer>
        </Modal>
      )}
      {moving && (
        <Modal
          title={moving.copy ? tr("Copy to group") : tr("Move to group")}
          onClose={() => {
            if (!operationLock.current) setMoving(null);
          }}
        >
          <div className="modal-body">
            <p className="muted small">
              {moving.ids.length}{" "}
              {tr(
                "selected · subgroups keep their contents and relationships.",
              )}
            </p>
            <Select
              label={tr("Destination group")}
              value={moveTo}
              onChange={setMoveTo}
              options={[
                { value: "", label: tr("No group") },
                ...groups
                  .filter(
                    (g) =>
                      moving.copy ||
                      !containedRecords(records, moving.ids).some(
                        (r) => r.id === g.id,
                      ),
                  )
                  .map((g) => ({ value: g.id, label: g.data.label })),
              ]}
            />
          </div>
          <footer>
            <button
              className="secondary"
              disabled={operationBusy}
              onClick={() => setMoving(null)}
            >
              {tr("Cancel")}
            </button>
            <button
              className="primary"
              disabled={operationBusy}
              onClick={() =>
                void mutateRecords(
                  () =>
                    moving.copy
                      ? duplicateRecords(records, moving.ids, moveTo)
                      : moveRecords(records, moving.ids, moveTo),
                  moving.copy ? "Copies created." : "Records moved.",
                )
              }
            >
              {operationBusy
                ? tr("Saving…")
                : moving.copy
                  ? tr("Copy records")
                  : tr("Move records")}
            </button>
          </footer>
        </Modal>
      )}
      {prompts[0] && (
        <Modal
          title={
            prompts[0].kind === "hostKey"
              ? tr("Verify server identity")
              : prompts[0].detail.name || tr("Authentication required")
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
                  {tr("First connection to")}{" "}
                  <strong>{prompts[0].detail.address}</strong>.
                </p>
                <p className="muted">
                  {tr(
                    "Compare this fingerprint with a trusted source before saving the server key.",
                  )}
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
              {tr("Cancel")}
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
              {prompts[0].kind === "hostKey"
                ? tr("Trust and connect")
                : tr("Continue")}
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
          <button className="icon-btn" aria-label={tr("Dismiss message")}>
            <X size={15} />
          </button>
        </div>
      )}
    </div>
  );
}
