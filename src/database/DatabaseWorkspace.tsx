import SqlLibrary from "./SqlLibrary";
import { sqlAllowed, sqlConnectAllowed } from "./teamPermissions";
import { listen } from "@tauri-apps/api/event";
import DatabaseEvidence from "./DatabaseEvidence";
import ProvisioningDialog from "./ProvisioningDialog";
import MaintenancePlan, { maintenanceActions } from "./MaintenancePlan";
import { useEffect, useRef, useState } from "react";
import {
  Database,
  Plus,
  Play,
  Square,
  X,
  ChevronRight,
  Table2,
  Layers,
  Search,
  RefreshCw,
  ShieldCheck,
  Clock,
  Code2,
  Save,
  Check,
  Undo2,
  FolderOpen,
  Download,
} from "lucide-react";
import {
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import { call, errorText } from "../api";
import { newEntity, type Vault, type Entity } from "../types";
import { Modal } from "../components";
import SqlEditor from "./SqlEditor";
import catalog from "./catalog.json";
import validation from "./validation.json";
import inputs from "./actionInputs.json";
import ActionParameters, {
  defaults,
  type ParameterSpec,
} from "./ActionParameters";
import TableBrowser from "./TableBrowser";
import SchemaHistory from "./SchemaHistory";
import DatabaseDiagnostics from "./DatabaseDiagnostics";
import type { EditorState } from "@codemirror/state";
import {
  defaultConnection,
  engineNames,
  type DatabaseConnection,
  type DatabaseTab,
  type QueryResult,
  type RowSet,
} from "./types";
import "./database.css";
const rpc = <T,>(request: Record<string, unknown>) =>
  call<T>("database_request", { request });
const cell = (r: RowSet, row: (string | null)[], name: string) =>
  row[
    r.columns.findIndex((c) => c.name.toLowerCase() === name.toLowerCase())
  ] ?? "";
export default function DatabaseWorkspace({
  vault,
  onVault,
  visible,
  teamMode,
}: {
  vault: Vault;
  onVault: (v: Vault) => void;
  visible: boolean;
  teamMode: boolean;
}) {
  const connections = vault.records.filter(
    (r) => r.kind === "databaseConnection",
  );
  const [selected, setSelected] = useState(""),
    [tabs, setTabs] = useState<DatabaseTab[]>([]),
    [active, setActive] = useState("");
  const [section, setSection] = useState<
      "editor" | "catalog" | "diagnostics" | "schemaHistory" | "evidence"
    >("editor"),
    [form, setForm] = useState<DatabaseConnection | null>(null),
    [editId, setEditId] = useState("");
  const [error, setError] = useState(""),
    [pending, setPending] = useState(false),
    [schemas, setSchemas] = useState<string[]>([]),
    [schema, setSchema] = useState(""),
    [tables, setTables] = useState<{ name: string; type: string }[]>([]),
    [search, setSearch] = useState(""),
    [resultsIndex, setResultsIndex] = useState(0);
  const [library, setLibrary] = useState<{recordId:string; tabId?:string; sql:string} | null>(null);
  const [closeTarget, setCloseTarget] = useState<DatabaseTab | null>(null);
  const [maintenance, setMaintenance] = useState<
    (typeof catalog)[number] | null
  >(null);
  // Freeze the chosen target while the dialog is open; a late connection result
  // must not silently replace its engine, draft fields or administrative target.
  const [provisioning, setProvisioning] = useState<Entity | null>(null);
  const [checkAction, setCheckAction] = useState<
    (typeof catalog)[number] | null
  >(null);
  const [checkResult, setCheckResult] = useState<QueryResult | null>(null),
    [checkError, setCheckError] = useState(""),
    [checking, setChecking] = useState(false),
    [autoRefresh, setAutoRefresh] = useState(false);
  const [checkParameters, setCheckParameters] = useState<Record<string, any>>(
    {},
  );
  const [browsing, setBrowsing] = useState<{
    schema: string;
    table: string;
    sessionId: string;
  } | null>(null);
  const [planResult, setPlanResult] = useState<RowSet | null>(null);
  const [oraclePlan, setOraclePlan] = useState(false),
    [sqlId, setSqlId] = useState(""),
    [planBusy, setPlanBusy] = useState(false);
  const [routineRows, setRoutineRows] = useState<
    Record<string, unknown>[] | null
  >(null);
  const [columnCompletion, setColumnCompletion] = useState<
    Record<string, string[]>
  >({});
  const editorCache = useRef<
    Record<
      string,
      { state?: EditorState; scrollTop?: number; scrollLeft?: number }
    >
  >({});
  const contracts = inputs as Record<
    string,
    {
      parameters: Record<string, ParameterSpec>;
      resultUnits?: Record<string, string>;
      license?: string | null;
    }
  >;
  const checkPending = useRef(false),
    checkGeneration = useRef(0);
  const checked = validation.actions as Record<
    string,
    { status: string; version: string }
  >;
  const checkLabels: Record<string, string> = {
    passed: "Laboratuvarda doğrulandı",
    "requires-parameters": "Parametre gerekiyor",
    "requires-change-plan": "Değişiklik planı gerekiyor",
    "requires-standby": "Standby ortamı gerekiyor",
    "permission-denied": "Ek DB yetkisi gerekiyor",
    "requires-cloud-environment": "Bulut ortamı gerekiyor",
    "license-gated": "Lisans kapalı",
  };
  const generation = useRef(0),
    tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const tab = tabs.find((t) => t.id === active),
    connection = connections.find((c) => c.id === (tab?.recordId || selected));
  const profile = connection?.data as DatabaseConnection | undefined;
  const target = tab?.target ?? profile;
  const result = tab?.result?.results[resultsIndex];
  async function showPlan() {
    if (!tab || planBusy) return;
    setPlanBusy(true);
    const id = tab.id;
    try {
      const result = await rpc<RowSet>({
        op: "plan",
        sessionId: id,
        sql: tab.sql,
        sqlId,
        childNumber: 0,
      });
      if (tabsRef.current.some((t) => t.id === id)) {
        setPlanResult(result);
        setOraclePlan(false);
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setPlanBusy(false);
    }
  }
  function prepareCheck(action: (typeof catalog)[number]) {
    const specs = contracts[action.id]?.parameters ?? {};
    if (Object.keys(specs).length) {
      setCheckAction(action);
      setCheckResult(null);
      setCheckError("");
      setAutoRefresh(false);
      setCheckParameters(defaults(specs, profile));
    } else {
      setCheckParameters({});
      void runCheck(action, {});
    }
  }
  async function runCheck(
    action: (typeof catalog)[number],
    parameters = checkParameters,
  ) {
    if (!connection || checkPending.current) return;
    const epoch = ++checkGeneration.current;
    checkPending.current = true;
    setChecking(true);
    setCheckAction(action);
    setCheckError("");
    setCheckResult(null);
    try {
      const r = await rpc<QueryResult>({
        op: "check",
        recordId: connection.id,
        actionId: action.id,
        parameters,
      });
      if (epoch === checkGeneration.current) setCheckResult(r);
    } catch (e) {
      if (epoch === checkGeneration.current) setCheckError(errorText(e));
    } finally {
      checkPending.current = false;
      setChecking(false);
    }
  }
  useEffect(() => {
    if (!autoRefresh || !checkAction || !visible) return;
    const timer = setInterval(() => {
      if (!document.hidden) void runCheck(checkAction);
    }, 30000);
    return () => clearInterval(timer);
  }, [autoRefresh, checkAction, visible, connection?.id, checkParameters]);
  useEffect(() => {
    checkGeneration.current++;
    setCheckAction(null);
    setMaintenance(null);
    setAutoRefresh(false);
    setCheckResult(null);
  }, [connection?.id]);
  function patch(id: string, p: Partial<DatabaseTab>) {
    setTabs((ts) => ts.map((t) => (t.id === id ? { ...t, ...p } : t)));
  }
  useEffect(() => {
    if (!teamMode) return;
    let live = true;
    const subscription = listen<{ sessionId: string; message: string }>(
      "database-session-closed",
      ({ payload }) => {
        if (live)
          patch(payload.sessionId, {
            busy: false,
            result: undefined,
            state: "failed",
            error: payload.message,
          });
      },
    );
    return () => {
      live = false;
      void subscription.then((dispose) => dispose());
    };
  }, [teamMode]);
  useEffect(
    () => () => {
      generation.current++;
      for (const t of tabsRef.current)
        void rpc({ op: "close", sessionId: t.id }).catch(() => {});
    },
    [],
  );
  async function saveConnection() {
    if (!form || pending) return;
    setPending(true);
    setError("");
    try {
      if (
        !form.label.trim() ||
        !form.host.trim() ||
        !form.username.trim() ||
        !Number.isInteger(form.port) ||
        form.port < 1 ||
        form.port > 65535
      )
        throw new Error("Ad, sunucu, kullanıcı ve geçerli port gerekli.");
      const record: Entity = {
        ...newEntity("databaseConnection", form),
        ...(editId ? { id: editId } : {}),
      };
      onVault(await call<Vault>("records_save", { records: [record] }));
      setSelected(record.id);
      setForm(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setPending(false);
    }
  }
  async function connect(record: Entity) {
    if (pending || !sqlConnectAllowed(teamMode, record)) return;
    setPending(true);
    setError("");
    const token = generation.current;
    try {
      const r = await rpc<{
        sessionId: string;
        version: string;
        autoCommit: boolean;
        capabilities: Record<string, unknown>;
      }>({ op: "open", recordId: record.id });
      if (token !== generation.current) {
        await rpc({ op: "close", sessionId: r.sessionId });
        return;
      }
      const p = record.data as DatabaseConnection;
      const t: DatabaseTab = {
        id: r.sessionId,
        recordId: record.id,
        title: p.label,
        engine: p.engine,
        sql:
          p.engine === "oracle"
            ? "SELECT current_timestamp FROM dual;"
            : "SELECT current_timestamp;",
        busy: false,
        transaction: false,
        autoCommit: r.autoCommit,
        capabilities: r.capabilities,
        target: {
          host: p.host,
          port: p.port,
          database: p.database,
          environment: p.environment,
        },
      };
      setTabs((ts) => [...ts, t]);
      setActive(t.id);
      setSelected(record.id);
      setSection("editor");
      setResultsIndex(0);
      await loadSchemas(t.id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setPending(false);
    }
  }
  async function loadSchemas(id: string) {
    const epoch = ++generation.current;
    try {
      const r = await rpc<RowSet>({ op: "schemas", sessionId: id });
      if (epoch !== generation.current) return;
      const current = tabsRef.current.find((t) => t.id === id);
      const username = String(
        connections.find((c) => c.id === current?.recordId)?.data.username ??
          "",
      ).toLowerCase();
      const rank = (s: string) =>
        s.toLowerCase() === username
          ? 0
          : ["public", "dbo"].includes(s.toLowerCase())
            ? 1
            : 2;
      setSchemas(
        r.rows
          .map((row) => cell(r, row, "TABLE_SCHEM"))
          .sort((a, b) => rank(a) - rank(b) || a.localeCompare(b)),
      );
      setTables([]);
      setSchema("");
    } catch (e) {
      if (epoch === generation.current) setError(errorText(e));
    }
  }
  async function loadTables(name: string) {
    if (!tab) return;
    setSchema(name);
    const epoch = ++generation.current;
    try {
      const r = await rpc<RowSet>({
        op: "tables",
        sessionId: tab.id,
        schema: name,
      });
      if (epoch !== generation.current) return;
      setTables(
        r.rows.map((row) => ({
          name: cell(r, row, "TABLE_NAME"),
          type: cell(r, row, "TABLE_TYPE"),
        })),
      );
    } catch (e) {
      setError(errorText(e));
    }
  }
  async function execute(sql = tab?.sql) {
    if (
      !sqlAllowed(
        teamMode,
        connections.find((c) => c.id === tab?.recordId),
        "sql_run",
      )
    ) {
      setError("SQL çalıştırma yetkisi yok");
      return;
    }
    if (!tab || tab.busy || !sql?.trim()) return;
    const id = tab.id;
    const operationId = crypto.randomUUID();
    patch(id, { busy: true, error: undefined, operationId, state: "running" });
    setResultsIndex(0);
    try {
      const result = await rpc<QueryResult>({
        op: "query",
        operationId,
        sessionId: id,
        sql,
        parameters: JSON.parse(tab.parameters || "{}"),
        maxRows: 1000,
        timeout: 30,
      });
      patch(id, {
        result,
        state: result.state ?? "succeeded",
        transaction: result.transaction,
        autoCommit: result.autoCommit,
      });
    } catch (e) {
      let failure: { state?: string } = {};
      try {
        failure = JSON.parse(errorText(e));
      } catch {}
      patch(id, { error: errorText(e), state: failure.state ?? "failed" });
      try {
        patch(
          id,
          await rpc<{ transaction: boolean; autoCommit: boolean }>({
            op: "status",
            sessionId: id,
          }),
        );
      } catch {}
    } finally {
      patch(id, { busy: false });
    }
  }
  async function transaction(action: string, enabled?: boolean, id = tab?.id) {
    if (!id || tabsRef.current.find((t) => t.id === id)?.busy) return;
    patch(id, { busy: true });
    try {
      const r = await rpc<{ autoCommit: boolean; transaction: boolean }>({
        op: "transaction",
        sessionId: id,
        action,
        enabled,
      });
      patch(id, r);
      if (action === "autocommit") {
        const record = connections.find(
          (c) => c.id === tabsRef.current.find((t) => t.id === id)?.recordId,
        );
        if (record)
          onVault(
            await call<Vault>("records_save", {
              records: [
                {
                  ...record,
                  data: { ...record.data, autoCommit: r.autoCommit },
                },
              ],
            }),
          );
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      patch(id, { busy: false });
    }
  }
  async function documentFile(kind: "open" | "sql" | "csv") {
    if (!tab) return;
    if (
      !sqlAllowed(
        teamMode,
        connections.find((c) => c.id === tab.recordId),
        kind === "open" ? "sql_run" : "sql_export",
      )
    ) {
      setError("Bu SQL dosya işlemi için yetkiniz yok");
      return;
    }
    try {
      if (kind === "open") {
        const path = await openDialog({
          filters: [{ name: "SQL", extensions: ["sql"] }],
          multiple: false,
        });
        if (typeof path === "string")
          patch(tab.id, {
            sql: await call<string>("database_file", {
              action: "read",
              path,
              sessionId: tab.id,
            }),
          });
        return;
      }
      if (kind === "csv" && !result?.columns) return;
      const path = await saveDialog({
        defaultPath: kind === "sql" ? "query.sql" : "results.csv",
        filters: [{ name: kind.toUpperCase(), extensions: [kind] }],
      });
      if (!path) return;
      const quote = (v: string | null) => {
        let s = v ?? "";
        if (/^[=+\-@\t\r]/.test(s)) s = "'" + s;
        return '"' + s.replaceAll('"', '""') + '"';
      };
      const content =
        kind === "sql"
          ? tab.sql
          : [result!.columns.map((c) => c.name), ...result!.rows]
              .map((row) => row.map(quote).join(","))
              .join("\r\n");
      await call("database_file", {
        action: "write",
        path,
        content,
        sessionId: tab.id,
      });
    } catch (e) {
      setError(errorText(e));
    }
  }
  async function close(
    t: DatabaseTab,
    decision?: "commit" | "rollback" | "discard",
  ) {
    try {
      const current =
        decision === "discard"
          ? { transaction: false }
          : await rpc<{ transaction: boolean }>({
              op: "status",
              sessionId: t.id,
            });
      if (current.transaction && !decision) {
        setCloseTarget(t);
        return;
      }
      if (decision && decision !== "discard")
        await rpc({ op: "transaction", sessionId: t.id, action: decision });
      await rpc({ op: "close", sessionId: t.id }).catch((e) => {
        if (decision !== "discard") throw e;
      });
      setTabs((ts) => {
        const i = ts.findIndex((v) => v.id === t.id),
          next = ts.filter((v) => v.id !== t.id);
        if (active === t.id) {
          const n = next[Math.min(i, next.length - 1)];
          setActive(n?.id ?? "");
          if (n) void loadSchemas(n.id);
          else {
            setSchemas([]);
            setTables([]);
          }
        }
        return next;
      });
      delete editorCache.current[t.id];
      setCloseTarget(null);
    } catch (e) {
      if (!t.busy) setCloseTarget({ ...t, state: "unknown_outcome" });
      setError(errorText(e));
    }
  }
  function browseTable(name: string) {
    if (!tab) return;
    const q = (s: string) =>
      tab.engine === "mssql"
        ? "[" + s.replaceAll("]", "]]") + "]"
        : '"' + s.replaceAll('"', '""') + '"';
    const target = q(schema) + "." + q(name);
    const sql =
      tab.engine === "mssql"
        ? `SELECT TOP (100) * FROM ${target};`
        : `SELECT * FROM ${target} ${tab.engine === "oracle" ? "FETCH FIRST 100 ROWS ONLY" : "LIMIT 100"};`;
    patch(tab.id, { sql });
    setSection("editor");
  }
  const actions = catalog.filter(
    (a) =>
      (!profile || a.engine === profile.engine) &&
      `${a.id} ${a.title}`
        .toLocaleLowerCase("tr")
        .includes(search.toLocaleLowerCase("tr")),
  );
  const completion = {
    ...Object.fromEntries(tables.map((t) => [t.name, [] as string[]])),
    ...columnCompletion,
  };
  return (
    <section
      className="db-workspace"
      style={{ display: visible ? "flex" : "none" }}
    >
      <header className="db-header">
        <div>
          <div className="db-eyebrow">TERMTERM / DATABASE WORKSPACE</div>
          <h1>
            Veritabanları <span>DEVELOPMENT</span>
          </h1>
        </div>
        <button
          className="primary"
          onClick={() => {
            setEditId("");
            setForm(defaultConnection());
          }}
        >
          <Plus size={16} /> Bağlantı ekle
        </button>
      </header>
      {library && connections.find(c => c.id === library.recordId) && <SqlLibrary
        key={`${vault.id}:${library.recordId}:${library.tabId ?? "read"}`}
        vault={vault} connection={connections.find(c => c.id === library.recordId)!} teamMode={teamMode}
        sql={library.sql} canLoad={!!library.tabId && tabs.some(t => t.id === library.tabId && !t.busy)}
        onVault={onVault} onClose={() => setLibrary(null)}
        onLoad={(sql, label) => { if(library.tabId) patch(library.tabId,{sql,parameters:"{}",libraryLabel:label}); }} />}
      {teamMode && (
        <div className="db-warning">
          Team SQL · Her işlem çevrimiçi doğrulanır. SQL çalıştırma, izleme,
          yönetim ve export ayrı izinlerdir. Serbest SQL hedef DB hesabının
          yetkileriyle çalışır. SSH tüneli, çevrimdışı SQL, metrik/şema geçmişi
          ve zorunlu ikinci onay servisi henüz açık değildir.
        </div>
      )}
      {error && (
        <div className="db-error" role="alert">
          {error}
          <button onClick={() => setError("")} aria-label="Hatayı kapat">
            <X size={14} />
          </button>
        </div>
      )}
      <div className="db-layout">
        <aside className="db-explorer">
          <div className="db-section-label">
            BAĞLANTILAR <span>{connections.length}</span>
          </div>
          {connections.map((c) => (
            <div
              key={c.id}
              className={
                "db-connection " + (connection?.id === c.id ? "selected" : "")
              }
            >
              <button
                onClick={() => {
                  setSelected(c.id);
                  const existing = tabs.find((t) => t.recordId === c.id);
                  setActive(existing?.id ?? "");
                  setColumnCompletion({});
                  if (existing) void loadSchemas(existing.id);
                  else {
                    generation.current++;
                    setSchemas([]);
                    setSchema("");
                    setTables([]);
                  }
                }}
                onDoubleClick={() => void connect(c)}
              >
                <Database size={17} />
                <span>
                  <strong>{c.data.label}</strong>
                  <small>
                    {engineNames[c.data.engine as keyof typeof engineNames]} ·{" "}
                    {c.data.host}
                  </small>
                </span>
              </button>
              <button
                title="Bağlan / yeni sorgu"
                disabled={pending || !sqlConnectAllowed(teamMode, c)}
                onClick={() => void connect(c)}
              >
                <Play size={14} />
              </button>
              <button title="Sorgu kitaplığını görüntüle" onClick={() => {
                const session=tabs.find(t=>t.recordId===c.id);
                setLibrary({recordId:c.id,tabId:session?.id,sql:session?.sql ?? ""});
              }}><FolderOpen size={14} /></button>
              <button
                title="Bağlantı bilgileri"
                onClick={() => {
                  setEditId(c.id);
                  setForm(c.data as DatabaseConnection);
                }}
              >
                ⋯
              </button>
            </div>
          ))}
          {!connections.length && (
            <p className="db-muted">
              Kasa içinde şifreli bir bağlantı oluşturun. Çift tıklayarak sorgu
              sekmesi açabilirsiniz.
            </p>
          )}
          <div className="db-section-label">
            NESNE GEZGİNİ
            <button
              title="Yenile"
              disabled={!tab || tab.busy}
              onClick={() => tab && void loadSchemas(tab.id)}
            >
              <RefreshCw size={13} />
            </button>
          </div>
          {schemas.map((s) => (
            <div key={s}>
              <button
                className={"db-tree-row " + (schema === s ? "selected" : "")}
                disabled={tab?.busy}
                onClick={() => void loadTables(s)}
              >
                <ChevronRight size={13} />
                <Layers size={14} />
                {s}
              </button>
              {schema === s &&
                tables.map((t) => (
                  <button
                    key={t.name}
                    className="db-tree-row db-table"
                    title="İlk 100 satır için SQL aç"
                    onClick={() =>
                      tab &&
                      setBrowsing({ schema, table: t.name, sessionId: tab.id })
                    }
                  >
                    <Table2 size={13} />
                    {t.name}
                  </button>
                ))}
            </div>
          ))}
          {tab && schema && (
            <button
              className="db-tree-row"
              onClick={async () => {
                try {
                  setRoutineRows(
                    await rpc<Record<string, unknown>[]>({
                      op: "routines",
                      sessionId: tab.id,
                      schema,
                    }),
                  );
                } catch (e) {
                  setError(errorText(e));
                }
              }}
            >
              Rutinler · {schema}
            </button>
          )}
          {!tab && <p className="db-muted">Nesneleri görmek için bağlanın.</p>}
          <div className="db-explorer-footer">
            <ShieldCheck size={14} /> Kimlik bilgileri şifreli kasada
          </div>
        </aside>
        <div className="db-main">
          <nav className="db-section-tabs">
            <button
              disabled={
                !connection ||
                pending ||
                !sqlAllowed(teamMode, connection, "sql_manage")
              }
              onClick={() => connection && setProvisioning(connection)}
            >
              DB / kullanıcı oluştur
            </button>
            <button
              className={section === "editor" ? "active" : ""}
              onClick={() => setSection("editor")}
            >
              <Code2 size={15} /> SQL çalışma alanı
            </button>
            <button
              className={section === "catalog" ? "active" : ""}
              onClick={() => setSection("catalog")}
            >
              <Layers size={15} /> Kontroller ve yönetim{" "}
              <span>{catalog.length}</span>
            </button>
            <button
              className={section === "diagnostics" ? "active" : ""}
              disabled={!connection || teamMode}
              title={
                teamMode
                  ? "Team metrik geçmişi henüz desteklenmiyor; Kontroller bölümünü kullanın"
                  : undefined
              }
              onClick={() => setSection("diagnostics")}
            >
              Sağlık ve geçmiş
            </button>
            <button
              className={section === "schemaHistory" ? "active" : ""}
              disabled={!connection || teamMode}
              onClick={() => setSection("schemaHistory")}
            >
              Şema geçmişi
            </button>
            <button
              className={section === "evidence" ? "active" : ""}
              disabled={
                !connection || !sqlAllowed(teamMode, connection, "sql_monitor")
              }
              onClick={() => setSection("evidence")}
            >
              İndeks ve yedek
            </button>
          </nav>
          {section === "evidence" && connection ? (
            <DatabaseEvidence
              key={connection.id}
              recordId={connection.id}
              sessionId={tab?.id}
              initialSchema={schema}
            />
          ) : section === "schemaHistory" && connection ? (
            <SchemaHistory
              key={connection.id}
              recordId={connection.id}
              sessionId={tab?.id}
              schema={schema}
            />
          ) : section === "diagnostics" && connection && profile ? (
            <DatabaseDiagnostics
              recordId={connection.id}
              profile={profile}
              visible={visible}
              onPolicy={async (warning, critical) => {
                onVault(
                  await call<Vault>("records_save", {
                    records: [
                      {
                        ...connection,
                        data: {
                          ...connection.data,
                          healthWarning: warning,
                          healthCritical: critical,
                        },
                      },
                    ],
                  }),
                );
              }}
            />
          ) : section === "catalog" ? (
            <div className="db-catalog">
              <div className="db-catalog-heading">
                <div>
                  <h2>İşlem kataloğu</h2>
                  <p>
                    Motor, yetki ve sürüm koşullarıyla birlikte kaynak SQL.
                    Doğrulanmamış şablonlar otomatik çalıştırılmaz.
                  </p>
                </div>
                <label>
                  <Search size={16} />
                  <input
                    placeholder="Kontrol veya işlem ara…"
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                  />
                </label>
              </div>
              <div className="db-action-list">
                {actions.map((a) => (
                  <article className="db-action" key={a.id}>
                    <div>
                      <span className="db-action-id">
                        {a.id} · {a.risk}
                      </span>
                      <h3>{a.title}</h3>
                      <p>{a.permissions}</p>
                      <small>{a.target}</small>
                    </div>
                    <details>
                      <summary>SQL ve açıklama</summary>
                      <p>{a.explanation}</p>
                      <pre>{a.sql.join("\n\n")}</pre>
                      <p>{a.preconditions}</p>
                    </details>
                    <footer>
                      <span
                        className="db-unverified"
                        title={checked[a.id]?.version}
                      >
                        {maintenanceActions.has(a.id)
                          ? "Önizlemeli bakım · geliştirme"
                          : (checkLabels[checked[a.id]?.status] ??
                            "Canlı doğrulama bekliyor")}
                      </span>
                      {a.risk === "R0" && !contracts[a.id]?.license && (
                        <button
                          disabled={
                            !connection ||
                            checking ||
                            !sqlAllowed(teamMode, connection, "sql_monitor")
                          }
                          onClick={() => prepareCheck(a)}
                        >
                          Çalıştır
                        </button>
                      )}
                      {maintenanceActions.has(a.id) && (
                        <button
                          disabled={
                            !connection ||
                            !sqlAllowed(teamMode, connection, "sql_manage")
                          }
                          onClick={() => setMaintenance(a)}
                        >
                          Değişiklik planı
                        </button>
                      )}
                      <button
                        disabled={!tab}
                        onClick={() => {
                          if (tab) patch(tab.id, { sql: a.sql.join("\n\n") });
                          setSection("editor");
                        }}
                      >
                        Editörde aç
                      </button>
                    </footer>
                  </article>
                ))}
              </div>
            </div>
          ) : (
            <>
              <div className="db-query-tabs">
                {tabs.map((t) => (
                  <div key={t.id} className={active === t.id ? "active" : ""}>
                    <button
                      onClick={() => {
                        setActive(t.id);
                        setColumnCompletion({});
                        setSelected(t.recordId);
                        setResultsIndex(0);
                        void loadSchemas(t.id);
                      }}
                    >
                      <i className={t.busy ? "busy" : ""} />
                      {t.title}
                      {t.transaction ? " •" : ""}
                    </button>
                    <button
                      aria-label={`${t.title} sekmesini kapat`}
                      onClick={() => void close(t)}
                    >
                      <X size={13} />
                    </button>
                  </div>
                ))}
              </div>
              {tab ? (
                <>
                  <div className="db-toolbar">
                    <button
                      onClick={() => setLibrary({recordId:tab.recordId, tabId:tab.id, sql:tab.sql})}
                      disabled={tab.busy}
                    >
                      Sorgu kitaplığı
                    </button>
                    <button
                      title="SQL dosyası aç"
                      onClick={() => void documentFile("open")}
                    >
                      <FolderOpen size={14} />
                    </button>
                    <button
                      title="SQL dosyasına kaydet"
                      onClick={() => void documentFile("sql")}
                    >
                      <Save size={14} />
                    </button>
                    <button
                      className="primary"
                      disabled={
                        tab.busy || !sqlAllowed(teamMode, connection, "sql_run")
                      }
                      onClick={() => void execute()}
                    >
                      <Play size={14} /> Çalıştır <kbd>Ctrl ↵</kbd>
                    </button>
                    <button
                      disabled={!tab.busy}
                      onClick={() =>
                        void rpc({
                          op: "cancel",
                          sessionId: tab.id,
                          operationId: tab.operationId,
                        }).catch((e) => setError(errorText(e)))
                      }
                    >
                      <Square size={13} /> İptal
                    </button>
                    <span className="db-divider" />
                    <button
                      disabled={
                        tab.busy ||
                        !tab.transaction ||
                        !sqlAllowed(teamMode, connection, "sql_run")
                      }
                      onClick={() => void transaction("commit")}
                    >
                      <Check size={14} /> Commit
                    </button>
                    <button
                      disabled={
                        tab.busy ||
                        !tab.transaction ||
                        !sqlAllowed(teamMode, connection, "sql_run")
                      }
                      onClick={() => void transaction("rollback")}
                    >
                      <Undo2 size={14} /> Rollback
                    </button>
                    <label>
                      <input
                        type="checkbox"
                        checked={tab.autoCommit}
                        disabled={
                          tab.busy ||
                          !sqlAllowed(teamMode, connection, "sql_run")
                        }
                        onChange={(e) =>
                          void transaction("autocommit", e.target.checked)
                        }
                      />{" "}
                      Otomatik commit
                    </label>
                    <button
                      title="Sorguyu kitaplığa kaydet"
                      onClick={() => setLibrary({recordId:tab.recordId, tabId:tab.id, sql:tab.sql})}
                      disabled={tab.busy}
                    >
                      <Save size={14} />
                    </button>
                  </div>
                  {tab.libraryLabel && (
                    <div className="db-library-note">
                      {tab.libraryLabel} · Kitaplıktan yüklendi. Çalıştırma ve
                      transaction kontrolü bu sekmededir.
                    </div>
                  )}
                  <details className="db-parameters">
                    <summary>Değer parametreleri · :ad biçimi</summary>
                    <textarea
                      aria-label="SQL parametreleri JSON"
                      value={tab.parameters ?? "{}"}
                      spellCheck={false}
                      onChange={(e) =>
                        patch(tab.id, { parameters: e.target.value })
                      }
                    />
                    <small>
                      Örnek: {`{"ad":"İstanbul","limit":100}`} · Değerler SQL'e
                      birleştirilmez; sürücüye bağlanır.
                    </small>
                  </details>
                  <div className="db-target">
                    <button
                      disabled={tab.busy || planBusy}
                      onClick={() =>
                        tab.engine === "oracle"
                          ? setOraclePlan(true)
                          : void showPlan()
                      }
                    >
                      Çalışma planı
                    </button>
                    <span
                      className="db-capability-version"
                      title={JSON.stringify(tab.capabilities, null, 2)}
                    >
                      {tab.state ?? "Hazır"} ·{" "}
                      {String(tab.capabilities?.version ?? "")}
                    </span>
                    <Database size={13} />
                    {engineNames[tab.engine]}
                    <ChevronRight size={12} />
                    {target?.host}:{target?.port}
                    <ChevronRight size={12} />
                    {target?.database}
                    <span
                      className={
                        target?.environment === "production" ? "production" : ""
                      }
                    >
                      {target?.environment === "production"
                        ? "ÜRETİM"
                        : target?.environment === "test"
                          ? "TEST"
                          : "GELİŞTİRME"}
                    </span>
                  </div>
                  <div className="db-editor-container">
                    <SqlEditor
                      key={tab.id}
                      cache={
                        editorCache.current[tab.id] ??
                        (editorCache.current[tab.id] = {})
                      }
                      value={tab.sql}
                      engine={tab.engine}
                      schema={completion}
                      onChange={(sql) => patch(tab.id, { sql })}
                      onExecute={(sql) => void execute(sql)}
                    />
                  </div>
                  <div className="db-results-heading">
                    <strong>Sonuçlar</strong>
                    <button
                      onClick={() => setResultsIndex(-1)}
                      className={resultsIndex === -1 ? "selected" : ""}
                    >
                      Mesajlar ({tab.result?.messages?.length ?? 0})
                    </button>
                    <button
                      disabled={
                        !result?.columns ||
                        !sqlAllowed(teamMode, connection, "sql_export")
                      }
                      title="Görünen sonuçları CSV olarak kaydet"
                      onClick={() => void documentFile("csv")}
                    >
                      <Download size={13} />{" "}
                      {result?.truncated ? "Görünen kısmı CSV" : "CSV"}
                    </button>
                    {tab.result?.results.map((_, i) => (
                      <button
                        key={i}
                        className={resultsIndex === i ? "selected" : ""}
                        onClick={() => setResultsIndex(i)}
                      >
                        Sonuç {i + 1}
                      </button>
                    ))}
                    <span>
                      <Clock size={12} />
                      {tab.busy
                        ? "Çalışıyor…"
                        : tab.result
                          ? `${tab.result.elapsedMs} ms`
                          : "Hazır"}
                    </span>
                  </div>
                  <div className="db-results">
                    {resultsIndex === -1 ? (
                      <pre className="db-query-messages">
                        {tab.result?.messages?.join("\n") ||
                          "Sunucudan ek mesaj yok."}
                      </pre>
                    ) : tab.error ? (
                      <pre className="db-query-error">{tab.error}</pre>
                    ) : result?.columns ? (
                      <table>
                        <thead>
                          <tr>
                            <th>#</th>
                            {result.columns.map((c, i) => (
                              <th key={i}>
                                {c.name}
                                <small>{c.type}</small>
                              </th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {result.rows.map((row, i) => (
                            <tr key={i}>
                              <td>{i + 1}</td>
                              {row.map((v, j) => (
                                <td
                                  key={j}
                                  className={v === null ? "db-null" : ""}
                                >
                                  {v === null ? "NULL" : v}
                                </td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    ) : result?.affectedRows !== undefined ? (
                      <p className="db-muted">
                        {result.affectedRows} satır etkilendi.
                      </p>
                    ) : (
                      <div className="db-result-empty">
                        <Table2 size={25} />
                        <p>Sonuçlar burada görünecek</p>
                        <small>
                          Sorguyu veya seçili SQL’i Ctrl+Enter ile çalıştırın.
                        </small>
                      </div>
                    )}
                  </div>
                  <footer className="db-status">
                    <span>
                      {result?.rows
                        ? `${result.rows.length} satır`
                        : "Sonuç bekleniyor"}
                      {result?.truncated
                        ? " · Görüntüleme sınırına ulaşıldı"
                        : ""}
                    </span>
                    <span>
                      {tab.transaction
                        ? "Açık transaction"
                        : tab.autoCommit
                          ? "Auto-commit açık"
                          : "Manuel transaction"}{" "}
                      · Görüntüleme sınırı 1.000 satır
                    </span>
                  </footer>
                </>
              ) : (
                <div className="db-welcome">
                  <div className="db-welcome-icon">
                    <Database size={38} />
                  </div>
                  <h2>Üç motor. Tek çalışma alanı.</h2>
                  <p>
                    PostgreSQL, SQL Server ve Oracle bağlantılarınız,
                    <br />
                    sorgularınız ve kontrolleriniz TermTerm içinde.
                  </p>
                  <div>
                    <span>PostgreSQL</span>
                    <span>SQL Server</span>
                    <span>Oracle</span>
                  </div>
                  <button
                    className="primary"
                    onClick={() => {
                      setEditId("");
                      setForm(defaultConnection());
                    }}
                  >
                    <Plus size={15} /> İlk bağlantıyı oluştur
                  </button>
                  <small>
                    Mevcut Team/senkronizasyon veritabanından bağımsızdır.
                  </small>
                </div>
              )}
            </>
          )}
        </div>
      </div>
      {browsing && (
        <Modal
          wide
          title={`${browsing.schema}.${browsing.table}`}
          onClose={() => setBrowsing(null)}
        >
          <TableBrowser
            key={`${browsing.sessionId}:${browsing.schema}:${browsing.table}`}
            {...browsing}
            onColumns={(cols) =>
              setColumnCompletion((c) => ({ ...c, [browsing.table]: cols }))
            }
          />
        </Modal>
      )}
      {routineRows && (
        <Modal
          wide
          title="Rutinler / parametreler"
          onClose={() => setRoutineRows(null)}
        >
          <div className="db-browser">
            {routineRows.length === 0 ? (
              <p>Görünür rutin yok.</p>
            ) : (
              routineRows.map((r, i) => (
                <div key={i}>
                  <button
                    disabled={!r.procedure_name}
                    onClick={async () => {
                      try {
                        const detail = await rpc<{
                          parameters: Record<string, unknown>[];
                        }>({
                          op: "routines",
                          sessionId: tab?.id,
                          schema,
                          routine: r.procedure_name,
                        });
                        setRoutineRows(detail.parameters);
                      } catch (e) {
                        setError(errorText(e));
                      }
                    }}
                  >
                    {String(r.procedure_name ?? r.column_name ?? "Sonuç")}
                  </button>
                  <pre>{JSON.stringify(r, null, 2)}</pre>
                </div>
              ))
            )}
          </div>
        </Modal>
      )}
      {oraclePlan && (
        <Modal
          title="Oracle mevcut cursor planı"
          onClose={() => setOraclePlan(false)}
        >
          <div className="db-browser">
            <label>
              SQL_ID
              <input
                value={sqlId}
                onChange={(e) => setSqlId(e.target.value)}
                placeholder="13 karakterli SQL_ID"
              />
            </label>
            <p>Child cursor 0 · DBMS_XPLAN · AWR/ASH kullanılmaz.</p>
            <button
              disabled={planBusy || !/^[0-9a-z]{13}$/.test(sqlId)}
              onClick={() => void showPlan()}
            >
              Planı göster
            </button>
          </div>
        </Modal>
      )}
      {planResult && (
        <Modal wide title="Çalışma planı" onClose={() => setPlanResult(null)}>
          <pre className="db-plan">
            {planResult.rows.map((row) => row.join("\n")).join("\n")}
          </pre>
        </Modal>
      )}
      {provisioning && (
        <ProvisioningDialog
          key={provisioning.id}
          recordId={provisioning.id}
          profile={provisioning.data as DatabaseConnection}
          onClose={() => setProvisioning(null)}
        />
      )}
      {maintenance && connection && (
        <MaintenancePlan
          key={connection.id + maintenance.id}
          actionId={maintenance.id}
          title={maintenance.title}
          recordId={connection.id}
          sessionId={tab?.id}
          initialSchema={schema}
          onClose={() => setMaintenance(null)}
        />
      )}
      {checkAction && (
        <Modal
          wide
          title={checkAction.id + " · " + checkAction.title}
          onClose={() => {
            checkGeneration.current++;
            setCheckAction(null);
            setAutoRefresh(false);
          }}
        >
          <div className="db-check-result">
            <ActionParameters
              specs={contracts[checkAction.id]?.parameters ?? {}}
              values={checkParameters}
              onChange={(values) => {
                checkGeneration.current++;
                setCheckParameters(values);
                setCheckResult(null);
                setCheckError("");
                setAutoRefresh(false);
              }}
              key={connection?.id + ":" + checkAction.id}
              recordId={connection?.id}
              sessionId={tab?.id}
              engine={profile?.engine ?? "postgresql"}
            />
            <button
              className="primary"
              disabled={checking}
              onClick={() => void runCheck(checkAction)}
            >
              Parametrelerle çalıştır
            </button>
            <p>
              {checkResult?.status ?? "Henüz ölçülmedi"}{" "}
              {checkResult?.message ?? ""} {checkResult?.historyWarning ?? ""}
            </p>
            <p>
              {connection?.data.label} · Ayrı izleme bağlantısı ·{" "}
              {checking
                ? "Çalışıyor…"
                : checkResult
                  ? `${checkResult.elapsedMs} ms`
                  : "Hazır"}
            </p>
            {checkAction.cost === "light" &&
              Number(checkAction.poll_seconds) > 0 && (
                <label>
                  <input
                    type="checkbox"
                    checked={autoRefresh}
                    onChange={(e) => setAutoRefresh(e.target.checked)}
                  />{" "}
                  Görünürken 30 saniyede yenile
                </label>
              )}
            {checkError && <pre className="db-query-error">{checkError}</pre>}
            {checkResult?.results.map((r, i) => (
              <div className="db-results" key={i}>
                {r.columns && (
                  <table>
                    <thead>
                      <tr>
                        {r.columns.map((c, j) => (
                          <th key={j}>
                            {c.name}
                            <small>
                              {contracts[checkAction.id]?.resultUnits?.[
                                c.name
                              ] !== "value"
                                ? contracts[checkAction.id]?.resultUnits?.[
                                    c.name
                                  ]
                                : ""}
                            </small>
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {r.rows.map((row, k) => (
                        <tr key={k}>
                          {row.map((v, j) => (
                            <td key={j}>{v === null ? "NULL" : v}</td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            ))}
            <p>{checkAction.explanation}</p>
            <small>
              Boş sonuç veya eksik yetki, sistemin sağlıklı olduğunu kanıtlamaz.
            </small>
          </div>
        </Modal>
      )}
      {form && (
        <Modal
          title={editId ? "Bağlantı bilgileri" : "Yeni veritabanı bağlantısı"}
          onClose={() => !pending && setForm(null)}
        >
          <form
            className="db-form"
            onSubmit={(e) => {
              e.preventDefault();
              void saveConnection();
            }}
          >
            <label>
              Bağlantı adı
              <input
                required
                value={form.label}
                onChange={(e) => setForm({ ...form, label: e.target.value })}
              />
            </label>
            <div className="db-form-pair">
              <label>
                Motor
                <select
                  value={form.engine}
                  onChange={(e) => {
                    const engine = e.target
                      .value as DatabaseConnection["engine"];
                    setForm({
                      ...form,
                      engine,
                      port:
                        engine === "postgresql"
                          ? 5432
                          : engine === "mssql"
                            ? 1433
                            : 1521,
                      database:
                        engine === "postgresql"
                          ? "postgres"
                          : engine === "mssql"
                            ? "master"
                            : "FREEPDB1",
                    });
                  }}
                >
                  {Object.entries(engineNames).map(([k, v]) => (
                    <option key={k} value={k}>
                      {v}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Ortam
                <select
                  value={form.environment}
                  onChange={(e) =>
                    setForm({
                      ...form,
                      environment: e.target
                        .value as DatabaseConnection["environment"],
                    })
                  }
                >
                  <option value="development">Geliştirme</option>
                  <option value="test">Test</option>
                  <option value="production">Üretim</option>
                </select>
              </label>
            </div>
            <div className="db-form-pair">
              <label>
                Sunucu
                <input
                  required
                  value={form.host}
                  onChange={(e) => setForm({ ...form, host: e.target.value })}
                />
              </label>
              <label>
                Port
                <input
                  type="number"
                  min={1}
                  max={65535}
                  required
                  value={form.port}
                  onChange={(e) =>
                    setForm({ ...form, port: Number(e.target.value) })
                  }
                />
              </label>
            </div>
            <label>
              {form.engine === "oracle" ? "Service / PDB" : "Veritabanı"}
              <input
                required
                value={form.database}
                onChange={(e) => setForm({ ...form, database: e.target.value })}
              />
            </label>
            <div className="db-form-pair">
              <label>
                Kullanıcı
                <input
                  required
                  autoComplete="off"
                  value={form.username}
                  onChange={(e) =>
                    setForm({ ...form, username: e.target.value })
                  }
                />
              </label>
              <label>
                Parola
                <input
                  type="password"
                  autoComplete="new-password"
                  disabled={
                    teamMode &&
                    !!editId &&
                    !sqlAllowed(
                      teamMode,
                      connections.find((c) => c.id === editId),
                      "reveal",
                    )
                  }
                  value={form.password ?? ""}
                  onChange={(e) =>
                    setForm({ ...form, password: e.target.value })
                  }
                />
              </label>
            </div>
            <label>
              TLS
              <select
                value={form.tls}
                onChange={(e) =>
                  setForm({
                    ...form,
                    tls: e.target.value as DatabaseConnection["tls"],
                  })
                }
              >
                <option value="verify-full">
                  CA ve sunucu kimliğini doğrula
                </option>
                <option value="disabled-local">
                  Yalnız yerel laboratuvar — TLS kapalı
                </option>
              </select>
            </label>
            <label>
              SSH üzerinden bağlan
              <select
                disabled={teamMode}
                value={form.sshHostId}
                onChange={(e) =>
                  setForm({ ...form, sshHostId: e.target.value })
                }
              >
                <option value="">Doğrudan bağlantı</option>
                {vault.records
                  .filter(
                    (r) =>
                      r.kind === "host" &&
                      (!r.data.protocol || r.data.protocol === "ssh"),
                  )
                  .map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.data.label} · mevcut chain/proxy ayarları
                    </option>
                  ))}
              </select>
            </label>
            {form.engine === "oracle" && (
              <label>
                Oracle bağlantı rolü
                <select
                  aria-label="Oracle bağlantı rolü"
                  value={form.oracleRole ?? "normal"}
                  onChange={(e) =>
                    setForm({
                      ...form,
                      oracleRole: e.target.value as "normal" | "sysdba",
                    })
                  }
                >
                  <option value="normal">Normal</option>
                  <option value="sysdba">SYSDBA · CDB/PDB yönetimi</option>
                </select>
              </label>
            )}
            {form.engine === "oracle" && form.sshHostId && (
              <label>
                TLS sunucu sertifikası DN
                <input
                  value={form.serverDn ?? ""}
                  onChange={(e) =>
                    setForm({ ...form, serverDn: e.target.value })
                  }
                  placeholder="CN=db.example.com, O=Company"
                />
              </label>
            )}
            {teamMode ? (
              <label>
                CA sertifikası (public PEM, isteğe bağlı)
                <textarea
                  aria-label="Team CA PEM"
                  value={form.caPem ?? ""}
                  onChange={(e) =>
                    setForm({ ...form, caPem: e.target.value, caPath: "" })
                  }
                  placeholder="-----BEGIN CERTIFICATE-----"
                />
              </label>
            ) : (
              <label>
                CA dosyası (isteğe bağlı)
                <input
                  value={form.caPath}
                  onChange={(e) => setForm({ ...form, caPath: e.target.value })}
                />
              </label>
            )}
            {teamMode && (
              <label>
                Klasör (izinler bu kapsamdan devralınır)
                <select
                  aria-label="SQL bağlantı klasörü"
                  value={form.groupId ?? ""}
                  disabled={!!editId}
                  onChange={(e) =>
                    setForm({ ...form, groupId: e.target.value })
                  }
                >
                  <option value="">Kasa kökü</option>
                  {vault.records
                    .filter(
                      (r) =>
                        r.kind === "group" && sqlAllowed(teamMode, r, "edit"),
                    )
                    .map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.data.label}
                      </option>
                    ))}
                </select>
              </label>
            )}
            <p>Kimlik bilgileri açık olan kasaya şifreli kaydedilir.</p>
            <button
              className="primary"
              disabled={
                pending ||
                (!!editId &&
                  !sqlAllowed(
                    teamMode,
                    connections.find((c) => c.id === editId),
                    "edit",
                  ))
              }
              type="submit"
            >
              {pending ? "Kaydediliyor…" : "Bağlantıyı kaydet"}
            </button>
          </form>
        </Modal>
      )}
      {closeTarget && (
        <Modal
          title={
            closeTarget.state === "unknown_outcome"
              ? "Bağlantı durumu doğrulanamadı"
              : "Açık transaction var"
          }
          onClose={() => setCloseTarget(null)}
        >
          <p>
            {closeTarget.title} sekmesini kapatmadan önce değişiklikleri
            tamamlayın.{" "}
            {closeTarget.state === "unknown_outcome" &&
              "Sunucudaki değişikliklerin sonucu belirsiz. Kapatma Commit veya Rollback yapıldığını doğrulamaz."}
          </p>
          <div className="db-toolbar">
            <button
              disabled={closeTarget.state === "unknown_outcome"}
              onClick={() => void close(closeTarget, "rollback")}
            >
              Rollback ve kapat
            </button>
            <button
              disabled={closeTarget.state === "unknown_outcome"}
              onClick={() => void close(closeTarget, "commit")}
            >
              Commit ve kapat
            </button>
            {closeTarget.state === "unknown_outcome" && (
              <button onClick={() => void close(closeTarget, "discard")}>
                Bağlantıyı kapat
              </button>
            )}
            <button onClick={() => setCloseTarget(null)}>Vazgeç</button>
          </div>
        </Modal>
      )}
    </section>
  );
}
