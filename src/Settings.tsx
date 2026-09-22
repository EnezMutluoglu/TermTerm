import { tr } from "./i18n";
import Integrations from "./Integrations";
import Updates from "./Updates";

import { terminalThemes } from "./terminalThemes";
import TerminalPreview from "./TerminalPreview";

import { shortcuts, shortcutLabel } from "./shortcuts";

import { listen } from "@tauri-apps/api/event";

import { useState, useEffect, useRef } from "react";

import {
  Database,
  ShieldCheck,
  HardDrive,
  RefreshCw,
  Users,
  FolderOpen,
  CheckCircle2,
  Settings2,
  Keyboard,
  Download,
  Upload,
  PlugZap,
} from "lucide-react";

import { Field, Select, Busy, Check, Modal } from "./components";

import { call, chooseFile, saveFile, errorText, desktop } from "./api";

import {
  defaultProfile,
  newEntity,
  type SyncProfile,
  type Vault,
  type Entity,
} from "./types";

export default function Settings({
  vault,

  onVault,

  notify,

  fontSize,

  onFontSize,

  themeId,

  onTheme,
  initialTab = "sync",
  navigationKey = 0,
}: {
  vault: Vault;

  onVault: (v: Vault) => void;

  notify: (s: string) => void;

  fontSize: number;

  onFontSize: (n: number) => void;

  themeId: string;

  onTheme: (id: string) => void;
  initialTab?: string;
  navigationKey?: number;
}) {
  const preferences = vault.records.find((r) => r.kind === "settings");

  const [logging, setLogging] = useState(preferences?.data.logging !== false);
  const [copyOnSelect, setCopyOnSelect] = useState(
    preferences?.data.copyOnSelect !== false,
  );
  const [rightClickPaste, setRightClickPaste] = useState(
    preferences?.data.rightClickPaste !== false,
  );

  const [externalEditor, setExternalEditor] = useState(
    preferences?.data.externalEditor ?? "",
  );

  const [localShell, setLocalShell] = useState(
    preferences?.data.localShell ?? "",
  );

  const [retention, setRetention] = useState(
    preferences?.data.logRetentionDays ?? 30,
  );

  const profiles = vault.records.filter((r) => r.kind === "syncProfile");

  const [profileId, setProfileId] = useState(profiles[0]?.id ?? "");

  const existing = profiles.find((r) => r.id === profileId);

  const [tab, setTab] = useState(initialTab);
  useEffect(() => setTab(initialTab), [initialTab, navigationKey]);

  const [profile, setProfile] = useState<SyncProfile>({
    ...defaultProfile,

    ...existing?.data,
  });

  const [busy, setBusy] = useState(false);

  const working = useRef(false);

  const [migrationUser, setMigrationUser] = useState("");

  const [migrationPassword, setMigrationPassword] = useState("");

  const [syncPreview, setSyncPreview] = useState<{
    token: string;
    registered: boolean;
    targetChanged: boolean;
    localRecords: number;
    pending: number;
    changes: { id: string; label: string; action: string }[];
  } | null>(null);

  const [result, setResult] = useState("");

  const [error, setError] = useState("");

  const [conflicts, setConflicts] = useState<any[]>([]);

  const [members, setMembers] = useState<any[]>([]);

  const [username, setUsername] = useState("");

  const [role, setRole] = useState("viewer");

  const [memberPassword, setMemberPassword] = useState("");

  const [remote, setRemote] = useState<any[] | null>(null);

  const [remoteId, setRemoteId] = useState("");

  const [remotePassword, setRemotePassword] = useState("");

  const [auto, setAuto] = useState(false);

  const update = (k: keyof SyncProfile, v: any) => {
    setSyncPreview(null);
    setProfile((p) => ({ ...p, [k]: v }));
  };

  async function execute(f: () => Promise<void>) {
    if (working.current) return;

    working.current = true;

    setBusy(true);

    setError("");

    try {
      await f();
    } catch (e) {
      setError(errorText(e));
    } finally {
      working.current = false;

      setBusy(false);
    }
  }

  async function save() {
    const record = {
      ...(existing ?? newEntity("syncProfile")),

      data: { ...profile, enabled: false },
    };

    onVault(
      await call<Vault>("records_save", {
        records: [record],
      }),
    );

    setProfileId(record.id);

    setResult(tr("Profile saved in your encrypted local vault."));
  }

  async function sync() {
    const report = await call<{
      uploaded: number;

      downloaded: number;

      conflicts: any[];

      revision: number;
    }>("sync_run", { profile });

    setConflicts(report.conflicts);

    setResult(
      tr("Synced · {uploaded} uploaded · {downloaded} downloaded", {
        uploaded: report.uploaded,
        downloaded: report.downloaded,
      }) +
        " · " +
        tr("revision {revision}", { revision: report.revision }),
    );

    onVault(await call<Vault>("vault_info"));
  }

  useEffect(() => {
    if (!desktop) return;

    void call<boolean>("sync_status")
      .then(setAuto)

      .catch(() => {});

    const un = listen<any>("sync-event", ({ payload }) => {
      if (payload.error) {
        setError(payload.error);
      } else {
        const r = payload.report;

        setConflicts(r.conflicts);

        setResult(
          tr("Synced · {uploaded} uploaded · {downloaded} downloaded", r),
        );

        void call<Vault>("vault_info").then(onVault);
      }
    });

    return () => {
      void un.then((f) => f());
    };
  }, []);

  async function openRemote() {
    const path = await saveFile("ttvault", "Remote-vault.ttvault");

    if (path) {
      onVault(
        await call<Vault>("sync_download", {
          profile,

          id: remoteId,

          password: remotePassword,

          path,
        }),
      );

      setRemote(null);

      setResult(tr("Remote vault opened as an encrypted local file."));
    }
  }

  return (
    <div className="settings-page">
      <div className="page-heading">
        <div>
          <span className="eyebrow">{tr("MAKE IT YOURS")}</span>

          <h1>{tr("Settings")}</h1>

          <p>{tr("Your workspace, on your terms.")}</p>
        </div>

        <span className="local-badge">
          <ShieldCheck size={15} />
          {tr("Local vault active")}
        </span>
      </div>

      <div className="settings-layout">
        <nav className="settings-nav">
          {[
            { id: "general", icon: Settings2, label: tr("General") },

            { id: "sync", icon: Database, label: tr("PostgreSQL sync") },

            { id: "team", icon: Users, label: tr("Team vault") },

            { id: "integrations", icon: PlugZap, label: tr("Integrations") },

            {
              id: "shortcuts",
              icon: Keyboard,
              label: tr("Keyboard shortcuts"),
            },
            { id: "updates", icon: Download, label: tr("Updates") },
          ].map((t) => (
            <button
              key={t.id}

              className={tab === t.id ? "active" : ""}

              onClick={() => {
                setTab(t.id);

                setError("");
              }}
            >
              <t.icon size={16} />

              {t.label}
            </button>
          ))}
        </nav>

        <div className="settings-content">
          {tab === "updates" && <Updates />}
          {tab === "general" && (
            <>
              <h2>{tr("General")}</h2>

              <p className="muted">
                {tr("Your files stay available without a database connection.")}
              </p>

              <div className="settings-card">
                <h3>
                  <HardDrive size={18} />
                  {tr("Local storage")}
                </h3>

                <Field
                  label={tr("Vault file")}

                  value={vault.path}

                  readOnly

                  onChange={() => {}}
                />

                <div className="settings-facts">
                  <span>{tr("Encryption")}</span>

                  <strong>XChaCha20-Poly1305</strong>

                  <span>{tr("Password derivation")}</span>

                  <strong>{tr("Argon2id · 64 MiB · 3 passes")}</strong>

                  <span>{tr("Device identity")}</span>

                  <code>{vault.deviceId}</code>
                </div>
              </div>

              <div className="settings-card">
                <h3>{tr("Terminal appearance")}</h3>

                <Select
                  label={tr("Terminal color theme")}

                  value={themeId}

                  onChange={onTheme}

                  options={Object.entries(terminalThemes).map(([value, t]) => ({
                    value,

                    label: t.label,
                  }))}
                />

                {/win/i.test(navigator.platform) ? (
                  <Select
                    label={tr("Local terminal shell")}

                    value={localShell}

                    onChange={setLocalShell}

                    options={[
                      { value: "", label: tr("System default (PowerShell)") },

                      { value: "powershell.exe", label: "Windows PowerShell" },

                      {
                        value: "pwsh.exe",

                        label: tr("PowerShell 7 (if installed)"),
                      },

                      { value: "cmd.exe", label: tr("Command Prompt") },

                      {
                        value: "wsl.exe",
                        label: tr("WSL default distribution"),
                      },
                    ]}
                  />
                ) : (
                  <Field
                    label={tr(
                      "Local shell executable (blank uses login shell)",
                    )}

                    value={localShell}

                    onChange={setLocalShell}
                  />
                )}

                <Field
                  label={tr("Font size")}

                  type="number"

                  value={fontSize}

                  onChange={(v) => onFontSize(Math.max(10, Math.min(24, v)))}
                />

                <TerminalPreview themeId={themeId} fontSize={fontSize} />
                <Check checked={copyOnSelect} onChange={setCopyOnSelect}>
                  {tr("Copy text when mouse selection finishes")}
                </Check>
                <Check checked={rightClickPaste} onChange={setRightClickPaste}>
                  {tr("Right-click pastes into the terminal")}
                </Check>
                <p className="muted small">
                  {tr(
                    "Shift+Insert also pastes. Hold Shift to select or paste while a remote application is using the mouse.",
                  )}
                </p>

                <Check checked={logging} onChange={setLogging}>
                  {tr("Save terminal output in the encrypted vault")}
                </Check>

                <Field
                  label={tr(
                    "External editor executable (blank uses system default)",
                  )}

                  value={externalEditor}

                  onChange={setExternalEditor}
                />

                <Field
                  label={tr("Log retention (days; 0 keeps all logs)")}

                  type="number"

                  value={retention}

                  onChange={(v) => setRetention(Math.max(0, Math.min(3650, v)))}
                />

                <button
                  className="primary"

                  onClick={() =>
                    void execute(async () => {
                      onVault(
                        await call<Vault>("records_save", {
                          records: [
                            {
                              ...(preferences ?? newEntity("settings")),

                              data: {
                                ...preferences?.data,

                                label: tr("Preferences"),

                                fontSize,

                                terminalTheme: themeId,
                                copyOnSelect,
                                rightClickPaste,

                                logging,

                                externalEditor,

                                localShell,

                                logRetentionDays: retention,
                              },
                            },
                          ],
                        }),
                      );

                      notify(tr("Preferences saved."));
                    })
                  }
                >
                  {tr("Save preferences")}
                </button>
              </div>
            </>
          )}

          {tab === "sync" && (
            <>
              <h2>{tr("PostgreSQL sync")}</h2>

              <p className="muted">
                {tr(
                  "Connect directly to your PostgreSQL server. Only encrypted vault records leave this computer.",
                )}
              </p>

              <div className="settings-card">
                <div className="section-header">
                  <h3>
                    <Database size={18} />
                    {tr("Connection profile")}
                  </h3>

                  {import.meta.env.DEV && (
                    <button
                      className="text-btn"

                      onClick={() =>
                        void execute(async () => {
                          setProfile(await call<SyncProfile>("lab_profile"));

                          setResult(
                            tr(
                              "WSL lab profile loaded. Save it to keep it in this vault.",
                            ),
                          );
                        })
                      }
                    >
                      {tr("Load WSL test profile")}
                    </button>
                  )}
                </div>

                <Select
                  label={tr("Saved profile")}

                  value={profileId}

                  onChange={(id) =>
                    void execute(async () => {
                      if (auto)
                        await call("sync_watch", { profile, enabled: false });

                      setAuto(false);

                      setProfileId(id);

                      setProfile({
                        ...defaultProfile,

                        ...profiles.find((p) => p.id === id)?.data,
                      });

                      setConflicts([]);

                      setResult("");
                    })
                  }

                  options={[
                    { value: "", label: tr("New profile") },

                    ...profiles.map((p) => ({
                      value: p.id,

                      label: p.data.label,
                    })),
                  ]}
                />

                <Field
                  label={tr("Profile name")}

                  value={profile.label}

                  onChange={(v) => update("label", v)}
                />

                <div className="form-row wide-left">
                  <Field
                    label={tr("Server address")}

                    value={profile.host}

                    onChange={(v) => update("host", v)}
                  />

                  <Field
                    label="Port"

                    value={profile.port}

                    onChange={(v) => update("port", v)}

                    type="number"
                  />
                </div>

                <div className="form-row">
                  <Field
                    label={tr("Database")}

                    value={profile.database}

                    onChange={(v) => update("database", v)}
                  />

                  <Field
                    label={tr("Schema")}

                    value={profile.schema}

                    onChange={(v) => update("schema", v)}
                  />
                </div>

                <div className="form-row">
                  <Field
                    label={tr("Username")}

                    value={profile.username}

                    onChange={(v) => update("username", v)}
                  />

                  <Field
                    label={tr("Password")}

                    value={profile.password}

                    onChange={(v) => update("password", v)}

                    type="password"
                  />
                </div>

                <div className="file-field">
                  <Field
                    label={tr("CA certificate (optional for public CAs)")}

                    value={profile.caPath}

                    onChange={(v) => update("caPath", v)}

                    placeholder={tr("Path to a PEM CA certificate")}
                  />

                  <button
                    className="secondary"

                    onClick={async () => {
                      const p = await chooseFile(["crt", "pem", "cer"]);

                      if (p) update("caPath", p);
                    }}
                  >
                    <FolderOpen size={16} />
                  </button>
                </div>

                <div className="panel-tip">
                  <ShieldCheck size={16} />
                  {tr(
                    "TLS certificate and server hostname verification are always enabled.",
                  )}
                </div>

                <div className="button-row">
                  <button
                    disabled={busy}

                    className="secondary"

                    onClick={() =>
                      void execute(async () => {
                        const r = await call<any>("sync_test", { profile });

                        setResult(
                          tr(
                            "Connected to {database} as {username} · TLS verified",
                            { database: r.database, username: r.username },
                          ) +
                            " · " +
                            (r.schemaReady
                              ? tr("Schema v{version} ready", {
                                  version: r.schemaVersion,
                                })
                              : tr(
                                  "Schema requires preparation (version {version}, missing tables: {count})",
                                  {
                                    version: r.schemaVersion ?? tr("None"),
                                    count: r.missingTables?.length ?? 0,
                                  },
                                )),
                        );
                      })
                    }
                  >
                    <PlugZap size={16} />
                    {tr("Test connection")}
                  </button>

                  <button
                    disabled={busy}

                    className="primary"

                    onClick={() => void execute(save)}
                  >
                    {tr("Save profile")}
                  </button>
                </div>
              </div>

              <div className="settings-card">
                <h3>{tr("Prepare database schema")}</h3>

                <p className="muted small">
                  {tr(
                    "Run versioned migrations with a separate schema owner. These credentials are used only for this operation. Application role grants remain in the provided migration scripts.",
                  )}
                </p>

                <Field
                  label={tr("Migration username")}
                  value={migrationUser}
                  onChange={setMigrationUser}
                />

                <Field
                  label={tr("Migration password")}
                  type="password"
                  value={migrationPassword}
                  onChange={setMigrationPassword}
                />

                <button
                  className="secondary"
                  disabled={
                    busy || !migrationUser || migrationUser === profile.username
                  }
                  onClick={() =>
                    void execute(async () => {
                      await call("sync_prepare", {
                        profile: {
                          ...profile,
                          username: migrationUser,
                          password: migrationPassword,
                        },
                      });

                      setMigrationPassword("");
                      setResult(
                        tr(
                          "Database schema is ready. Test the application connection next.",
                        ),
                      );
                    })
                  }
                >
                  {tr("Create / upgrade schema")}
                </button>
              </div>

              <div className="settings-card">
                <h3>{tr("Connect this vault")}</h3>

                <button
                  className="secondary"
                  disabled={busy}
                  onClick={() =>
                    void execute(async () => {
                      setSyncPreview(await call("sync_preview", { profile }));
                    })
                  }
                >
                  {tr("Preview database changes")}
                </button>

                {syncPreview && (
                  <div className="notice">
                    <p>
                      {syncPreview.localRecords} {tr("local records ·")}{" "}
                      {syncPreview.pending} {tr("pending changes ·")}{" "}
                      {syncPreview.registered
                        ? tr("Remote vault found")
                        : tr("Vault has not been uploaded")}
                      {syncPreview.targetChanged
                        ? tr(" · Database target changed")
                        : ""}
                    </p>

                    <ul>
                      {syncPreview.changes.slice(0, 100).map((c) => (
                        <li key={c.id}>
                          {c.label}: {c.action}
                        </li>
                      ))}
                    </ul>

                    {syncPreview.changes.length > 100 && (
                      <p>
                        {syncPreview.changes.length}{" "}
                        {tr("changes total; showing the first 100.")}
                      </p>
                    )}

                    <button
                      className="primary"
                      disabled={busy}
                      onClick={() =>
                        void execute(async () => {
                          await call("sync_bind", {
                            profile,
                            token: syncPreview.token,
                          });
                          setSyncPreview(null);

                          setResult(
                            tr(
                              "Database target connected. Choose Upload local vault for a new target, or Sync now for an existing vault. Conflicting records require your choice.",
                            ),
                          );
                        })
                      }
                    >
                      {tr("Connect reviewed target")}
                    </button>
                  </div>
                )}

                <p className="muted small">
                  {tr(
                    "Upload this local vault to a prepared database, or open a remote vault into a new local file.",
                  )}
                </p>

                <div className="button-row">
                  <button
                    disabled={busy}

                    className="secondary"

                    onClick={() =>
                      void execute(async () => {
                        await save();

                        await call("sync_upload", { profile });

                        await sync();
                      })
                    }
                  >
                    <Upload size={16} />
                    {tr("Upload local vault")}
                  </button>

                  <button
                    disabled={busy}

                    className="secondary"

                    onClick={() =>
                      void execute(async () => {
                        setRemote(await call<any[]>("sync_list", { profile }));
                      })
                    }
                  >
                    <Download size={16} />
                    {tr("Open remote vault")}
                  </button>

                  <button
                    disabled={busy}

                    className="primary"

                    onClick={() => void execute(sync)}
                  >
                    <RefreshCw size={16} />
                    {tr("Sync now")}
                  </button>
                </div>

                <Check
                  checked={auto}

                  onChange={(enabled) =>
                    void execute(async () => {
                      await call("sync_watch", { profile, enabled });

                      setAuto(enabled);
                    })
                  }
                >
                  {tr(
                    "Keep syncing in the background (notifications + 15-second fallback)",
                  )}
                </Check>

                <p className="muted small">
                  {tr(
                    "Database schema setup and role grants are included in the migration scripts. Use a separate migration account.",
                  )}
                </p>
              </div>
            </>
          )}

          {tab === "team" && (
            <>
              <h2>{tr("Team vault")}</h2>

              <p className="muted">
                {tr(
                  "Each member uses a distinct PostgreSQL login. Permissions are enforced by the database.",
                )}
              </p>

              <div className="settings-card">
                <div className="section-header">
                  <h3>{tr("Members")}</h3>

                  <button
                    disabled={busy}

                    className="secondary"

                    onClick={() =>
                      void execute(async () =>
                        setMembers(await call<any[]>("team_list", { profile })),
                      )
                    }
                  >
                    <RefreshCw size={15} />
                    {tr("Refresh")}
                  </button>
                </div>

                {members.length ? (
                  members.map((m) => (
                    <div className="member-row" key={m.username}>
                      <div className="avatar">
                        {m.username[0].toUpperCase()}
                      </div>

                      <strong>{m.username}</strong>

                      <span className="pill">{m.role}</span>

                      {m.role !== "owner" && (
                        <button
                          className="text-btn danger"

                          onClick={() =>
                            void execute(async () => {
                              await call("team_set", {
                                profile,

                                username: m.username,

                                role: "remove",

                                password: "",
                              });

                              setMembers(
                                await call<any[]>("team_list", { profile }),
                              );
                            })
                          }
                        >
                          {tr("Remove")}
                        </button>
                      )}
                    </div>
                  ))
                ) : (
                  <p className="muted small">
                    {tr(
                      "Connect a profile and refresh to view vault membership.",
                    )}
                  </p>
                )}
              </div>

              <div className="settings-card">
                <h3>{tr("Add or update a member")}</h3>

                <Field
                  label={tr("Existing PostgreSQL login")}

                  value={username}

                  onChange={setUsername}
                />

                <Select
                  label={tr("Vault role")}

                  value={role}

                  onChange={setRole}

                  options={[
                    { value: "viewer", label: tr("Viewer · connect and read") },

                    {
                      value: "editor",

                      label: tr("Editor · connect, read and write"),
                    },

                    {
                      value: "owner",
                      label: tr("Owner · manage vault members"),
                    },
                  ]}
                />

                <Field
                  label={tr("Member vault password")}

                  value={memberPassword}

                  onChange={setMemberPassword}

                  type="password"

                  hint={tr(
                    "Wraps this vault key specifically for the member. Share it securely outside the app.",
                  )}
                />

                <button
                  disabled={busy}

                  className="primary"

                  onClick={() =>
                    void execute(async () => {
                      await call("team_set", {
                        profile,

                        username,

                        role,

                        password: memberPassword,
                      });

                      setMemberPassword("");

                      setMembers(await call<any[]>("team_list", { profile }));

                      setResult(tr("Membership updated."));
                    })
                  }
                >
                  <Users size={16} />
                  {tr("Save membership")}
                </button>
              </div>
            </>
          )}

          {tab === "integrations" && (
            <Integrations vault={vault} onVault={onVault} />
          )}

          {tab === "shortcuts" && (
            <>
              <h2>{tr("Keyboard shortcuts")}</h2>

              <div className="settings-card shortcuts">
                {shortcuts.map((s) => (
                  <div key={s.id}>
                    <span>
                      {tr(s.label)}

                      {!s.terminal ? tr(" (outside terminal)") : ""}
                    </span>

                    <kbd>{shortcutLabel(s.id)}</kbd>
                  </div>
                ))}
              </div>

              <p className="muted">
                {tr(
                  "Control keys belong to the shell while a terminal is focused. Function keys, AltGr and terminal modes are handled by xterm.",
                )}
              </p>
            </>
          )}

          {busy && <Busy />}

          {result && (
            <div className="notice success">
              <CheckCircle2 size={17} />

              {result}
            </div>
          )}

          {error && <div className="notice error">{error}</div>}

          {conflicts.length > 0 && (
            <div className="settings-card">
              <h3>
                {conflicts.length} {tr("conflicts need review")}
              </h3>

              {conflicts.map((c) => (
                <div className="conflict" key={c.id}>
                  <strong>
                    {c.local?.data?.label ?? c.remote?.data?.label ?? c.id}
                  </strong>

                  <div className="conflict-versions">
                    <pre>{JSON.stringify(c.local, null, 2)}</pre>

                    <pre>{JSON.stringify(c.remote, null, 2)}</pre>
                  </div>

                  <div className="button-row">
                    {["local", "remote", "both"].map((choice) => (
                      <button
                        className="secondary"

                        key={choice}

                        onClick={() =>
                          void execute(async () => {
                            await call("sync_resolve", {
                              profile,

                              id: c.id,

                              choice,

                              revision: c.revision,
                            });

                            await sync();
                          })
                        }
                      >
                        {tr("Keep")} {choice}
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {remote && (
        <Modal title={tr("Open remote vault")} onClose={() => setRemote(null)}>
          <div className="modal-body">
            <p>
              {tr(
                "Choose a vault and enter its vault password. PostgreSQL credentials alone cannot decrypt it.",
              )}
            </p>

            <Select
              label={tr("Remote vault")}

              value={remoteId}

              onChange={setRemoteId}

              options={[
                { value: "", label: tr("Select a vault") },

                ...remote.map((r) => ({
                  value: r.id,

                  label: `${r.id.slice(0, 8)}… · ${tr(r.role[0].toUpperCase() + r.role.slice(1))} · ${tr("revision {revision}", { revision: r.revision })}`,
                })),
              ]}
            />

            <Field
              label={tr("Vault password")}

              value={remotePassword}

              onChange={setRemotePassword}

              type="password"
            />

            {error && <div className="notice error">{error}</div>}
          </div>

          <footer>
            <button className="secondary" onClick={() => setRemote(null)}>
              {tr("Cancel")}
            </button>

            <button
              className="primary"

              disabled={busy || !remoteId}

              onClick={() => void execute(openRemote)}
            >
              {busy ? <Busy /> : tr("Open in new file")}
            </button>
          </footer>
        </Modal>
      )}
    </div>
  );
}
