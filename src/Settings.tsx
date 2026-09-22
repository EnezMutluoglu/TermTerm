import Integrations from "./Integrations";
import Updates from "./Updates";

import { terminalThemes, terminalTheme } from "./terminalThemes";

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
}: {
  vault: Vault;

  onVault: (v: Vault) => void;

  notify: (s: string) => void;

  fontSize: number;

  onFontSize: (n: number) => void;

  themeId: string;

  onTheme: (id: string) => void;
}) {
  const preferences = vault.records.find((r) => r.kind === "settings");

  const [logging, setLogging] = useState(preferences?.data.logging !== false);

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

  const [tab, setTab] = useState("sync");

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

    setResult("Profile saved in your encrypted local vault.");
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
      `Synced · ${report.uploaded} uploaded · ${report.downloaded} downloaded · revision ${report.revision}`,
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
          `Synced · ${r.uploaded} uploaded · ${r.downloaded} downloaded`,
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

      setResult("Remote vault opened as an encrypted local file.");
    }
  }

  return (
    <div className="settings-page">
      <div className="page-heading">
        <div>
          <span className="eyebrow">MAKE IT YOURS</span>

          <h1>Settings</h1>

          <p>Your workspace, on your terms.</p>
        </div>

        <span className="local-badge">
          <ShieldCheck size={15} />
          Local vault active
        </span>
      </div>

      <div className="settings-layout">
        <nav className="settings-nav">
          {[
            { id: "general", icon: Settings2, label: "General" },

            { id: "sync", icon: Database, label: "PostgreSQL sync" },

            { id: "team", icon: Users, label: "Team vault" },

            { id: "integrations", icon: PlugZap, label: "Integrations" },

            { id: "shortcuts", icon: Keyboard, label: "Keyboard shortcuts" },
            { id: "updates", icon: Download, label: "Updates" },
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
              <h2>General</h2>

              <p className="muted">
                Your files stay available without a database connection.
              </p>

              <div className="settings-card">
                <h3>
                  <HardDrive size={18} />
                  Local storage
                </h3>

                <Field
                  label="Vault file"

                  value={vault.path}

                  readOnly

                  onChange={() => {}}
                />

                <div className="settings-facts">
                  <span>Encryption</span>

                  <strong>XChaCha20-Poly1305</strong>

                  <span>Password derivation</span>

                  <strong>Argon2id · 64 MiB · 3 passes</strong>

                  <span>Device identity</span>

                  <code>{vault.deviceId}</code>
                </div>
              </div>

              <div className="settings-card">
                <h3>Terminal appearance</h3>

                <Select
                  label="Terminal color theme"

                  value={themeId}

                  onChange={onTheme}

                  options={Object.entries(terminalThemes).map(([value, t]) => ({
                    value,

                    label: t.label,
                  }))}
                />

                {/win/i.test(navigator.platform) ? (
                  <Select
                    label="Local terminal shell"

                    value={localShell}

                    onChange={setLocalShell}

                    options={[
                      { value: "", label: "System default (PowerShell)" },

                      { value: "powershell.exe", label: "Windows PowerShell" },

                      {
                        value: "pwsh.exe",

                        label: "PowerShell 7 (if installed)",
                      },

                      { value: "cmd.exe", label: "Command Prompt" },

                      { value: "wsl.exe", label: "WSL default distribution" },
                    ]}
                  />
                ) : (
                  <Field
                    label="Local shell executable (blank uses login shell)"

                    value={localShell}

                    onChange={setLocalShell}
                  />
                )}

                <Field
                  label="Font size"

                  type="number"

                  value={fontSize}

                  onChange={(v) => onFontSize(Math.max(10, Math.min(24, v)))}
                />

                <div
                  className="terminal-sample"

                  style={{
                    fontSize,

                    background: terminalTheme(themeId).background,

                    color: terminalTheme(themeId).foreground,
                  }}
                >
                  you@server <span>~</span> $ echo "Hello, TermTerm"
                  <br />
                  Hello, TermTerm
                  <div className="terminal-palette">
                    {(
                      [
                        "red",

                        "green",

                        "yellow",

                        "blue",

                        "magenta",

                        "cyan",
                      ] as const
                    ).map((color) => (
                      <span
                        key={color}

                        style={{ background: terminalTheme(themeId)[color] }}
                      />
                    ))}
                  </div>
                </div>

                <Check checked={logging} onChange={setLogging}>
                  Save terminal output in the encrypted vault
                </Check>

                <Field
                  label="External editor executable (blank uses system default)"

                  value={externalEditor}

                  onChange={setExternalEditor}
                />

                <Field
                  label="Log retention (days; 0 keeps all logs)"

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

                                label: "Preferences",

                                fontSize,

                                terminalTheme: themeId,

                                logging,

                                externalEditor,

                                localShell,

                                logRetentionDays: retention,
                              },
                            },
                          ],
                        }),
                      );

                      notify("Preferences saved.");
                    })
                  }
                >
                  Save preferences
                </button>
              </div>
            </>
          )}

          {tab === "sync" && (
            <>
              <h2>PostgreSQL sync</h2>

              <p className="muted">
                Connect directly to your PostgreSQL server. Only encrypted vault
                records leave this computer.
              </p>

              <div className="settings-card">
                <div className="section-header">
                  <h3>
                    <Database size={18} />
                    Connection profile
                  </h3>

                  {import.meta.env.DEV && (
                    <button
                      className="text-btn"

                      onClick={() =>
                        void execute(async () => {
                          setProfile(await call<SyncProfile>("lab_profile"));

                          setResult(
                            "WSL lab profile loaded. Save it to keep it in this vault.",
                          );
                        })
                      }
                    >
                      Load WSL test profile
                    </button>
                  )}
                </div>

                <Select
                  label="Saved profile"

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
                    { value: "", label: "New profile" },

                    ...profiles.map((p) => ({
                      value: p.id,

                      label: p.data.label,
                    })),
                  ]}
                />

                <Field
                  label="Profile name"

                  value={profile.label}

                  onChange={(v) => update("label", v)}
                />

                <div className="form-row wide-left">
                  <Field
                    label="Server address"

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
                    label="Database"

                    value={profile.database}

                    onChange={(v) => update("database", v)}
                  />

                  <Field
                    label="Schema"

                    value={profile.schema}

                    onChange={(v) => update("schema", v)}
                  />
                </div>

                <div className="form-row">
                  <Field
                    label="Username"

                    value={profile.username}

                    onChange={(v) => update("username", v)}
                  />

                  <Field
                    label="Password"

                    value={profile.password}

                    onChange={(v) => update("password", v)}

                    type="password"
                  />
                </div>

                <div className="file-field">
                  <Field
                    label="CA certificate (optional for public CAs)"

                    value={profile.caPath}

                    onChange={(v) => update("caPath", v)}

                    placeholder="Path to a PEM CA certificate"
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
                  TLS certificate and server hostname verification are always
                  enabled.
                </div>

                <div className="button-row">
                  <button
                    disabled={busy}

                    className="secondary"

                    onClick={() =>
                      void execute(async () => {
                        const r = await call<any>("sync_test", { profile });

                        setResult(
                          `Connected to ${r.database} as ${r.username} · TLS verified · ${r.schemaReady ? `Schema v${r.schemaVersion} ready` : `Schema requires preparation (version ${r.schemaVersion ?? "none"}, missing tables: ${r.missingTables?.length ?? 0})`}`,
                        );
                      })
                    }
                  >
                    <PlugZap size={16} />
                    Test connection
                  </button>

                  <button
                    disabled={busy}

                    className="primary"

                    onClick={() => void execute(save)}
                  >
                    Save profile
                  </button>
                </div>
              </div>

              <div className="settings-card">
                <h3>Prepare database schema</h3>

                <p className="muted small">
                  Run versioned migrations with a separate schema owner. These
                  credentials are used only for this operation. Application role
                  grants remain in the provided migration scripts.
                </p>

                <Field
                  label="Migration username"
                  value={migrationUser}
                  onChange={setMigrationUser}
                />

                <Field
                  label="Migration password"
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
                        "Database schema is ready. Test the application connection next.",
                      );
                    })
                  }
                >
                  Create / upgrade schema
                </button>
              </div>

              <div className="settings-card">
                <h3>Connect this vault</h3>

                <button
                  className="secondary"
                  disabled={busy}
                  onClick={() =>
                    void execute(async () => {
                      setSyncPreview(await call("sync_preview", { profile }));
                    })
                  }
                >
                  Preview database changes
                </button>

                {syncPreview && (
                  <div className="notice">
                    <p>
                      {syncPreview.localRecords} local records ·{" "}
                      {syncPreview.pending} pending changes ·{" "}
                      {syncPreview.registered
                        ? "Remote vault found"
                        : "Vault has not been uploaded"}
                      {syncPreview.targetChanged
                        ? " · Database target changed"
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
                        {syncPreview.changes.length} changes total; showing the
                        first 100.
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
                            "Database target connected. Choose Upload local vault for a new target, or Sync now for an existing vault. Conflicting records require your choice.",
                          );
                        })
                      }
                    >
                      Connect reviewed target
                    </button>
                  </div>
                )}

                <p className="muted small">
                  Upload this local vault to a prepared database, or open a
                  remote vault into a new local file.
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
                    Upload local vault
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
                    Open remote vault
                  </button>

                  <button
                    disabled={busy}

                    className="primary"

                    onClick={() => void execute(sync)}
                  >
                    <RefreshCw size={16} />
                    Sync now
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
                  Keep syncing in the background (notifications + 15-second
                  fallback)
                </Check>

                <p className="muted small">
                  Database schema setup and role grants are included in the
                  migration scripts. Use a separate migration account.
                </p>
              </div>
            </>
          )}

          {tab === "team" && (
            <>
              <h2>Team vault</h2>

              <p className="muted">
                Each member uses a distinct PostgreSQL login. Permissions are
                enforced by the database.
              </p>

              <div className="settings-card">
                <div className="section-header">
                  <h3>Members</h3>

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
                    Refresh
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
                          Remove
                        </button>
                      )}
                    </div>
                  ))
                ) : (
                  <p className="muted small">
                    Connect a profile and refresh to view vault membership.
                  </p>
                )}
              </div>

              <div className="settings-card">
                <h3>Add or update a member</h3>

                <Field
                  label="Existing PostgreSQL login"

                  value={username}

                  onChange={setUsername}
                />

                <Select
                  label="Vault role"

                  value={role}

                  onChange={setRole}

                  options={[
                    { value: "viewer", label: "Viewer · connect and read" },

                    {
                      value: "editor",

                      label: "Editor · connect, read and write",
                    },

                    { value: "owner", label: "Owner · manage vault members" },
                  ]}
                />

                <Field
                  label="Member vault password"

                  value={memberPassword}

                  onChange={setMemberPassword}

                  type="password"

                  hint="Wraps this vault key specifically for the member. Share it securely outside the app."
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

                      setResult("Membership updated.");
                    })
                  }
                >
                  <Users size={16} />
                  Save membership
                </button>
              </div>
            </>
          )}

          {tab === "integrations" && (
            <Integrations vault={vault} onVault={onVault} />
          )}

          {tab === "shortcuts" && (
            <>
              <h2>Keyboard shortcuts</h2>

              <div className="settings-card shortcuts">
                {shortcuts.map((s) => (
                  <div key={s.id}>
                    <span>
                      {s.label}

                      {!s.terminal ? " (outside terminal)" : ""}
                    </span>

                    <kbd>{shortcutLabel(s.id)}</kbd>
                  </div>
                ))}
              </div>

              <p className="muted">
                Control keys belong to the shell while a terminal is focused.
                Function keys, AltGr and terminal modes are handled by xterm.
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
              <h3>{conflicts.length} conflicts need review</h3>

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
                        Keep {choice}
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
        <Modal title="Open remote vault" onClose={() => setRemote(null)}>
          <div className="modal-body">
            <p>
              Choose a vault and enter its vault password. PostgreSQL
              credentials alone cannot decrypt it.
            </p>

            <Select
              label="Remote vault"

              value={remoteId}

              onChange={setRemoteId}

              options={[
                { value: "", label: "Select a vault" },

                ...remote.map((r) => ({
                  value: r.id,

                  label: `${r.id.slice(0, 8)}… · ${r.role} · revision ${r.revision}`,
                })),
              ]}
            />

            <Field
              label="Vault password"

              value={remotePassword}

              onChange={setRemotePassword}

              type="password"
            />

            {error && <div className="notice error">{error}</div>}
          </div>

          <footer>
            <button className="secondary" onClick={() => setRemote(null)}>
              Cancel
            </button>

            <button
              className="primary"

              disabled={busy || !remoteId}

              onClick={() => void execute(openRemote)}
            >
              {busy ? <Busy /> : "Open in new file"}
            </button>
          </footer>
        </Modal>
      )}
    </div>
  );
}
