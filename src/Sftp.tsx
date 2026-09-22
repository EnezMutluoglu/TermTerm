import { useState, useEffect, useRef, memo } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Folder,
  File,
  ArrowUp,
  ArrowRight,
  ArrowLeft,
  RefreshCw,
  FolderPlus,
  ArrowDownToLine,
  HardDrive,
  Server,
  MoreHorizontal,
  X,
  CheckCircle2,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { call, errorText, desktop } from "./api";
import { Modal, Field, Busy, Check } from "./components";
import type { Entity, FileEntry, Endpoint } from "./types";
type Pane = {
  endpoint: Endpoint;
  entries: FileEntry[];
  selected: string[];
  loading: boolean;
  error: string;
};
type Transfer = {
  id: string;
  name: string;
  source: Endpoint;
  dest: Endpoint;
  state:
    | "queued"
    | "running"
    | "paused"
    | "cancelling"
    | "cancelled"
    | "done"
    | "error";
  bytes: number;
  total: number;
  error?: string;
};
const bytes = (n: number) =>
  n < 1024
    ? `${n} B`
    : n < 1048576
      ? `${(n / 1024).toFixed(1)} KB`
      : `${(n / 1048576).toFixed(1)} MB`;
const join = (e: Endpoint, name: string) =>
  e.connection === "local" && /^(?:[A-Za-z]:|\\\\)/.test(e.path)
    ? `${e.path.replace(/[\\/]$/, "")}\\${name}`
    : `${e.path.replace(/\/$/, "")}/${name}`;
function Sftp({
  records,
  home,
  notify,
  visible = true,
}: {
  visible?: boolean;
  records: Entity[];
  home: string;
  notify: (s: string) => void;
}) {
  const [panes, setPanes] = useState<Pane[]>([
    {
      endpoint: { connection: "local", path: home },
      entries: [],
      selected: [],
      loading: false,
      error: "",
    },
    {
      endpoint: { connection: "", path: "" },
      entries: [],
      selected: [],
      loading: false,
      error: "",
    },
  ]);
  const [paths, setPaths] = useState([home, ""]);
  const [hosts, setHosts] = useState(["local", ""]);
  const [queue, setQueue] = useState<Transfer[]>([]);
  const [edits, setEdits] = useState<
    { id: string; path: string; name: string }[]
  >([]);
  const [hidden, setHidden] = useState(false);
  const [overwrite, setOverwrite] = useState(false);
  const [action, setAction] = useState<{
    pane: number;
    type: string;
    entry?: FileEntry;
  } | null>(null);
  const [value, setValue] = useState("");
  const epochs = useRef([0, 0]);
  const live = useRef(true);
  const active = useRef(new Set<string>());
  const cancelled = useRef(new Set<string>());
  const queueRef = useRef(queue);
  queueRef.current = queue;
  const actionBusy = useRef(false);
  const [actionPending, setActionPending] = useState(false);
  const dropRef = useRef<(i: number, paths: string[]) => Promise<void>>(
    async () => {},
  );
  const visibleRef = useRef(visible);
  visibleRef.current = visible;
  const paneRef = useRef(panes);
  paneRef.current = panes;
  const patch = (i: number, p: Partial<Pane>) =>
    setPanes((a) => a.map((old, n) => (n === i ? { ...old, ...p } : old)));
  async function load(i: number, endpoint = paneRef.current[i].endpoint) {
    if (!endpoint.connection || !live.current) return;
    const epoch = ++epochs.current[i];
    patch(i, { loading: true, error: "", endpoint, selected: [] });
    setPaths((a) => a.map((p, n) => (n === i ? endpoint.path : p)));
    try {
      const entries = await call<FileEntry[]>("file_list", { endpoint });
      if (live.current && epochs.current[i] === epoch)
        patch(i, { entries, loading: false });
    } catch (e) {
      if (live.current && epochs.current[i] === epoch)
        patch(i, { error: errorText(e), loading: false });
    }
  }
  useEffect(() => {
    live.current = true;
    if (home) void load(0, { connection: "local", path: home });
  }, [home]);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
      epochs.current = epochs.current.map((n) => n + 1);
      for (const id of active.current)
        void call("transfer_control", { id, action: "cancel" }).catch(() => {});
      for (const p of paneRef.current)
        if (p.endpoint.connection && p.endpoint.connection !== "local")
          void call("sftp_disconnect", { id: p.endpoint.connection }).catch(
            () => {},
          );
    };
  }, []);
  useEffect(() => {
    if (!desktop) return;
    const un = listen<{ id: string; transferred: number; total: number }>(
      "transfer-progress",
      ({ payload }) =>
        setQueue((q) =>
          q.map((t) =>
            t.id === payload.id
              ? { ...t, bytes: payload.transferred, total: payload.total }
              : t,
          ),
        ),
    );
    const states = listen<{
      id: string;
      state: Transfer["state"];
      error?: string;
    }>("transfer-state", ({ payload }) => {
      setQueue((q) =>
        q.map((t) =>
          t.id === payload.id
            ? { ...t, state: payload.state, error: payload.error }
            : t,
        ),
      );
    });
    const drops = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type !== "drop" || !visibleRef.current) return;
      const element = document.elementFromPoint(
        payload.position.x / window.devicePixelRatio,
        payload.position.y / window.devicePixelRatio,
      );
      const panel = element?.closest<HTMLElement>("[data-file-pane]");
      if (panel)
        void dropRef.current(Number(panel.dataset.filePane), payload.paths);
    });
    return () => {
      void un.then((f) => f());
      void states.then((f) => f());
      void drops.then((f) => f());
    };
  }, []);
  async function connect(i: number, hostId: string) {
    const previous = paneRef.current[i].endpoint.connection;
    if (
      queueRef.current.some(
        (t) =>
          active.current.has(t.id) &&
          (t.source.connection === previous || t.dest.connection === previous),
      )
    ) {
      notify("Finish or cancel transfers using this connection first.");
      return;
    }
    const epoch = ++epochs.current[i];
    setHosts((a) => a.map((h, n) => (i === n ? hostId : h)));
    if (previous && previous !== "local")
      void call("sftp_disconnect", { id: previous }).catch((e) =>
        notify(errorText(e)),
      );
    if (hostId === "local") {
      void load(i, { connection: "local", path: home });
      return;
    }
    if (!hostId) {
      patch(i, { endpoint: { connection: "", path: "" }, entries: [] });
      return;
    }
    patch(i, { loading: true, error: "" });
    const id = crypto.randomUUID();
    try {
      const path = await call<string>("sftp_connect", { id, hostId });
      if (!live.current || epochs.current[i] !== epoch) {
        await call("sftp_disconnect", { id });
        return;
      }
      await load(i, { connection: id, path });
    } catch (e) {
      if (live.current && epochs.current[i] === epoch)
        patch(i, { loading: false, error: errorText(e) });
    }
  }
  async function control(t: Transfer, action: string) {
    if (t.state === "queued" && action === "cancel") {
      cancelled.current.add(t.id);
      setQueue((q) =>
        q.map((row) =>
          row.id === t.id ? { ...row, state: "cancelled" } : row,
        ),
      );
      return;
    }
    try {
      const accepted = await call<boolean>("transfer_control", {
        id: t.id,
        action,
      });
      if (!accepted) notify("Transfer is finishing or has already ended.");
    } catch (e) {
      notify(errorText(e));
    }
  }
  async function runTransfer(t: Transfer) {
    if (
      !live.current ||
      cancelled.current.has(t.id) ||
      active.current.has(t.id)
    )
      return;
    active.current.add(t.id);
    setQueue((q) =>
      q.map((old) =>
        old.id === t.id ? { ...old, state: "running", error: undefined } : old,
      ),
    );
    try {
      const count = await call<number>("file_transfer", {
        id: t.id,
        source: t.source,
        dest: t.dest,
        overwrite,
      });
      setQueue((q) =>
        q.map((old) =>
          old.id === t.id
            ? { ...old, state: "done", bytes: count, total: count }
            : old,
        ),
      );
    } catch (e) {
      setQueue((q) =>
        q.map((old) =>
          old.id === t.id
            ? {
                ...old,
                state: errorText(e).includes("Transfer cancelled")
                  ? "cancelled"
                  : "error",
                error: errorText(e),
              }
            : old,
        ),
      );
    } finally {
      active.current.delete(t.id);
    }
  }
  async function transfer(i: number, entries?: FileEntry[]) {
    const source = paneRef.current[i],
      dest = paneRef.current[1 - i];
    if (!dest.endpoint.connection) {
      notify("Connect the destination panel first.");
      return;
    }
    const selected =
      entries ?? source.entries.filter((e) => source.selected.includes(e.path));
    const transfers: Transfer[] = selected
      .filter(
        (e) =>
          !queueRef.current.some(
            (t) =>
              (active.current.has(t.id) || t.state === "queued") &&
              t.source.path === e.path &&
              t.source.connection === source.endpoint.connection &&
              t.dest.path === join(dest.endpoint, e.name),
          ),
      )
      .map((e) => ({
        id: crypto.randomUUID(),
        name: e.name,
        source: { ...source.endpoint, path: e.path },
        dest: { ...dest.endpoint, path: join(dest.endpoint, e.name) },
        state: "queued",
        bytes: 0,
        total: e.size,
      }));
    setQueue((q) => [...q, ...transfers]);
    for (const t of transfers) await runTransfer(t);
    if (
      paneRef.current[1 - i].endpoint.connection === dest.endpoint.connection &&
      paneRef.current[1 - i].endpoint.path === dest.endpoint.path
    )
      await load(1 - i);
  }
  dropRef.current = async (i, paths) => {
    const endpoint = { ...paneRef.current[i].endpoint };
    if (!endpoint.connection) {
      notify("Connect the destination panel first.");
      return;
    }
    const transfers: Transfer[] = paths.map((path) => ({
      id: crypto.randomUUID(),
      name: path.split(/[\\/]/).pop() || path,
      source: { connection: "local", path },
      dest: {
        ...endpoint,
        path: join(endpoint, path.split(/[\\/]/).pop() || ""),
      },
      state: "queued",
      bytes: 0,
      total: 0,
    }));
    setQueue((q) => [...q, ...transfers]);
    for (const t of transfers) await runTransfer(t);
    if (
      paneRef.current[i].endpoint.connection === endpoint.connection &&
      paneRef.current[i].endpoint.path === endpoint.path
    )
      await load(i);
  };
  async function doAction() {
    if (!action || actionBusy.current) return;
    if (
      ["mkdir", "rename"].includes(action.type) &&
      (!value.trim() || value === "." || value === ".." || /[\\/]/.test(value))
    ) {
      notify("Enter a single file or folder name.");
      return;
    }
    if (action.type === "chmod" && !/^[0-7]{3,4}$/.test(value)) {
      notify("Enter 3 or 4 octal permission digits.");
      return;
    }
    actionBusy.current = true;
    setActionPending(true);
    const pane = panes[action.pane];
    try {
      const endpoint = {
        ...pane.endpoint,
        path:
          action.type === "mkdir"
            ? join(pane.endpoint, value)
            : action.entry!.path,
      };
      await call("file_action", {
        endpoint,
        action: action.type,
        target: action.type === "rename" ? join(pane.endpoint, value) : null,
        permissions: action.type === "chmod" ? parseInt(value, 8) : null,
      });
      setAction(null);
      await load(action.pane);
    } catch (e) {
      notify(errorText(e));
    } finally {
      actionBusy.current = false;
      setActionPending(false);
    }
  }
  return (
    <div className="sftp-page">
      <div className="page-heading">
        <div>
          <span className="eyebrow">FILE TRANSFER</span>
          <h1>SFTP</h1>
          <p>Two panels. Every file within reach.</p>
        </div>
        <div className="button-row">
          <Check checked={hidden} onChange={setHidden}>
            Hidden files
          </Check>
          <Check checked={overwrite} onChange={setOverwrite}>
            Overwrite existing files
          </Check>
        </div>
      </div>
      {edits.map((edit) => (
        <div className="notice" key={edit.id}>
          <span>
            Editing {edit.name} in your external editor. Save there, then
            upload. A temporary plain-text copy is kept while editing.
          </span>
          <button
            className="secondary"
            onClick={async () => {
              try {
                await call("file_edit_save", {
                  id: edit.id,
                  overwrite,
                  close: false,
                });
                notify("Remote file updated.");
              } catch (e) {
                notify(errorText(e));
              }
            }}
          >
            Upload saved changes
          </button>
          <button
            className="text-btn"
            onClick={async () => {
              await call("file_edit_save", {
                id: edit.id,
                overwrite: false,
                close: true,
              });
              setEdits((a) => a.filter((e) => e.id !== edit.id));
            }}
          >
            Finish editing
          </button>
        </div>
      ))}
      <div className="sftp-panels">
        {panes.map((p, i) => (
          <section
            className="file-pane"
            data-file-pane={i}
            key={i}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault();
              const raw = e.dataTransfer.getData("application/termterm");
              if (raw) {
                try {
                  const data = JSON.parse(raw) as {
                    pane: number;
                    path: string;
                  };
                  if (data.pane !== i) {
                    const entry = panes[data.pane].entries.find(
                      (f) => f.path === data.path,
                    );
                    if (entry) void transfer(data.pane, [entry]);
                  }
                } catch {}
              }
            }}
          >
            <header>
              {hosts[i] === "local" ? (
                <HardDrive size={17} />
              ) : (
                <Server size={17} />
              )}
              <select
                aria-label={i ? "Right connection" : "Left connection"}
                value={hosts[i]}
                onChange={(e) => void connect(i, e.target.value)}
              >
                <option value="">Select a connection…</option>
                <option value="local">Local computer</option>
                {records
                  .filter(
                    (r) =>
                      r.kind === "host" &&
                      (!r.data.protocol || r.data.protocol === "ssh"),
                  )
                  .map((h) => (
                    <option key={h.id} value={h.id}>
                      {h.data.label}
                    </option>
                  ))}
              </select>
              <button
                className="icon-btn"
                title="Refresh directory"
                disabled={p.loading || !p.endpoint.connection}
                onClick={() => void load(i)}
              >
                <RefreshCw size={16} />
              </button>
            </header>
            <div className="path-bar">
              <button
                className="icon-btn"
                title="Parent directory"
                disabled={!p.endpoint.connection}
                onClick={() => {
                  const path =
                    p.endpoint.path
                      .replace(/[\\/]$/, "")
                      .split(/[\\/]/)
                      .slice(0, -1)
                      .join(
                        p.endpoint.connection === "local" &&
                          /^(?:[A-Za-z]:|\\\\)/.test(p.endpoint.path)
                          ? "\\"
                          : "/",
                      ) +
                    (p.endpoint.connection === "local" &&
                    /^[A-Za-z]:/.test(p.endpoint.path)
                      ? "\\"
                      : "");
                  void load(i, { ...p.endpoint, path: path || "/" });
                }}
              >
                <ArrowUp size={16} />
              </button>
              <input
                aria-label={i ? "Right path" : "Left path"}
                placeholder="Directory path"
                value={paths[i]}
                onChange={(e) =>
                  setPaths((a) =>
                    a.map((v, n) => (n === i ? e.target.value : v)),
                  )
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter")
                    void load(i, { ...p.endpoint, path: paths[i] });
                }}
              />
              {p.endpoint.connection === "local" && (
                <button
                  className="icon-btn"
                  title="Choose local directory"
                  onClick={async () => {
                    const path = await open({
                      directory: true,
                      multiple: false,
                    });
                    if (typeof path === "string")
                      void load(i, { connection: "local", path });
                  }}
                >
                  <Folder size={16} />
                </button>
              )}
            </div>
            <div className="file-actions">
              <button
                className="text-btn"
                disabled={!p.endpoint.connection}
                onClick={() => {
                  setAction({ pane: i, type: "mkdir" });
                  setValue("");
                }}
              >
                <FolderPlus size={15} />
                New folder
              </button>
              <button
                className="text-btn"
                disabled={!p.selected.length}
                onClick={() => void transfer(i)}
              >
                {i ? <ArrowLeft size={15} /> : <ArrowRight size={15} />}Transfer{" "}
                {p.selected.length || ""}
              </button>
              <span>{p.entries.length} items</span>
            </div>
            <div className="file-columns">
              <span>Name</span>
              <span>Size</span>
              <span>Modified</span>
            </div>
            <div className="file-list">
              {p.loading ? (
                <div className="file-empty">
                  <Busy label="Loading files…" />
                </div>
              ) : p.error ? (
                <div className="notice error">{p.error}</div>
              ) : !p.endpoint.connection ? (
                <div className="file-empty">
                  <Server size={35} />
                  <h3>Connect to a server</h3>
                  <p>Choose an SSH host above to browse its files.</p>
                </div>
              ) : (
                p.entries
                  .filter((e) => hidden || !e.name.startsWith("."))
                  .map((e) => (
                    <div
                      key={e.path}
                      className={
                        "file-row " +
                        (p.selected.includes(e.path) ? "selected" : "")
                      }
                      draggable
                      onDragStart={(ev) =>
                        ev.dataTransfer.setData(
                          "application/termterm",
                          JSON.stringify({ pane: i, path: e.path }),
                        )
                      }
                      onClick={(ev) =>
                        patch(i, {
                          selected:
                            ev.ctrlKey || ev.metaKey
                              ? p.selected.includes(e.path)
                                ? p.selected.filter((x) => x !== e.path)
                                : [...p.selected, e.path]
                              : [e.path],
                        })
                      }
                      onDoubleClick={() => {
                        if (e.directory)
                          void load(i, { ...p.endpoint, path: e.path });
                      }}
                    >
                      <div className="file-name">
                        {e.directory ? (
                          <Folder size={17} className="folder-icon" />
                        ) : (
                          <File size={16} />
                        )}
                        <span title={e.name}>{e.name}</span>
                      </div>
                      <span>{e.directory ? "—" : bytes(e.size)}</span>
                      <span>
                        {e.modified
                          ? new Date(e.modified * 1000).toLocaleDateString()
                          : "—"}
                      </span>
                      <button
                        className="icon-btn"
                        title="Rename"
                        onClick={(ev) => {
                          ev.stopPropagation();
                          setAction({ pane: i, type: "rename", entry: e });
                          setValue(e.name);
                        }}
                      >
                        <MoreHorizontal size={14} />
                      </button>
                    </div>
                  ))
              )}
            </div>
            <footer>
              <span>
                {p.selected.length ? "Selected " + p.selected.length : "Ready"}
              </span>
              {p.selected.length === 1 && (
                <div className="button-row">
                  {p.endpoint.connection !== "local" &&
                    !p.entries.find((e) => e.path === p.selected[0])
                      ?.directory && (
                      <button
                        className="text-btn"
                        onClick={async () => {
                          try {
                            setEdits((a) => a);
                            const edit = await call<{
                              id: string;
                              path: string;
                              name: string;
                            }>("file_edit", {
                              endpoint: { ...p.endpoint, path: p.selected[0] },
                            });
                            setEdits((a) => [...a, edit]);
                          } catch (e) {
                            notify(errorText(e));
                          }
                        }}
                      >
                        Edit in your external editor
                      </button>
                    )}
                  {p.endpoint.connection !== "local" && (
                    <button
                      className="text-btn"
                      onClick={() => {
                        setAction({
                          pane: i,
                          type: "chmod",
                          entry: p.entries.find(
                            (e) => e.path === p.selected[0],
                          ),
                        });
                        setValue("644");
                      }}
                    >
                      Permissions
                    </button>
                  )}
                  <button
                    className="text-btn danger"
                    onClick={() => {
                      setAction({
                        pane: i,
                        type: "remove",
                        entry: p.entries.find((e) => e.path === p.selected[0]),
                      });
                      setValue("");
                    }}
                  >
                    <Trash2 size={13} />
                    Delete
                  </button>
                </div>
              )}
            </footer>
          </section>
        ))}
      </div>
      <section className="transfer-queue">
        <header>
          <ArrowDownToLine size={16} />
          <h3>Transfer queue</h3>
          <span>
            {queue.filter((t) => t.state === "done").length} completed
          </span>
          <button
            className="text-btn"
            onClick={() => setQueue((q) => q.filter((t) => t.state !== "done"))}
          >
            Clear completed
          </button>
        </header>
        {!queue.length ? (
          <div className="queue-empty">
            Drag files between panels or select files and click Transfer.
          </div>
        ) : (
          queue.map((t) => (
            <div className="transfer-row" key={t.id}>
              {t.state === "done" ? (
                <CheckCircle2 size={17} className="green" />
              ) : (
                <File size={17} />
              )}
              <div>
                <strong>{t.name}</strong>
                <small title={t.error ?? t.dest.path}>
                  {t.error ?? t.dest.path}
                </small>
              </div>
              <span>
                {bytes(t.bytes)} / {bytes(t.total)}
              </span>
              <div className="progress-track">
                <i
                  style={{
                    width: `${t.total ? Math.min(100, (t.bytes / t.total) * 100) : 0}%`,
                  }}
                />
              </div>
              <span className={t.state === "error" ? "danger" : "muted"}>
                {t.state}
              </span>
              {["queued", "running", "paused"].includes(t.state) && (
                <div className="button-row">
                  {t.state !== "queued" && (
                    <button
                      className="text-btn"
                      onClick={() =>
                        void control(
                          t,
                          t.state === "paused" ? "resume" : "pause",
                        )
                      }
                    >
                      {t.state === "paused" ? "Resume" : "Pause"}
                    </button>
                  )}
                  <button
                    className="icon-btn"
                    title="Cancel transfer"
                    onClick={() => void control(t, "cancel")}
                  >
                    <X size={16} />
                  </button>
                </div>
              )}
              {(t.state === "error" || t.state === "cancelled") && (
                <button
                  className="icon-btn"
                  title="Retry transfer"
                  onClick={() => {
                    cancelled.current.delete(t.id);
                    void runTransfer(t);
                  }}
                >
                  <RotateCcw size={16} />
                </button>
              )}
            </div>
          ))
        )}
      </section>
      {action && (
        <Modal
          title={
            {
              mkdir: "New folder",
              rename: "Rename",
              chmod: "Change permissions",
              remove: "Delete file",
            }[action.type]!
          }
          onClose={() => {
            if (!actionBusy.current) setAction(null);
          }}
        >
          <div className="modal-body">
            {action.type === "remove" ? (
              <p>
                Delete <strong>{action.entry?.name}</strong>? Directories must
                be empty.
              </p>
            ) : (
              <Field
                label={action.type === "chmod" ? "Permissions (octal)" : "Name"}
                value={value}
                onChange={setValue}
              />
            )}
          </div>
          <footer>
            <button className="secondary" onClick={() => setAction(null)}>
              Cancel
            </button>
            <button
              className={action.type === "remove" ? "danger-button" : "primary"}
              disabled={actionPending}
              onClick={() => void doAction()}
            >
              {action.type === "remove" ? "Delete" : "Apply"}
            </button>
          </footer>
        </Modal>
      )}
    </div>
  );
}
export default memo(Sftp);
