import { tr } from "./i18n";
import { useEffect, useState } from "react";
import {
  Terminal,
  ShieldCheck,
  FolderOpen,
  ArrowRight,
  HardDrive,
  Database,
  LockKeyhole,
  FileArchive,
  ChevronLeft,
} from "lucide-react";
import { call, chooseFile, saveFile, desktop, errorText } from "./api";
import { Field, Busy, Check } from "./components";
import type { Vault, AppInfo } from "./types";
export default function Onboarding({
  info,
  loading = false,
  onOpen,
  onRestore,
}: {
  info?: AppInfo;
  loading?: boolean;
  onOpen: (v: Vault) => void;
  onRestore: () => void;
}) {
  const [mode, setMode] = useState<"welcome" | "create" | "open">("welcome");
  const [path, setPath] = useState("");
  const [name, setName] = useState(tr("Personal vault"));
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [remember, setRemember] = useState(false);
  const [rememberChanged, setRememberChanged] = useState(false);
  const [saved, setSaved] = useState(false);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [opened, setOpened] = useState<Vault | null>(null);
  useEffect(() => {
    if (info?.recent?.path) {
      setPath(info.recent.path);
      setMode("open");
    }
  }, [info?.recent?.path]);
  useEffect(() => {
    let current = true;
    setRemember(false);
    setRememberChanged(false);
    setSaved(false);
    if (!desktop || loading || mode !== "open" || !path) {
      setChecking(false);
      return;
    }
    setChecking(true);
    void call<boolean>("vault_remember_status", { path })
      .then((available) => {
        if (current) {
          setRemember(available);
          setSaved(available);
        }
      })
      .catch((e) => {
        if (current) setError(errorText(e));
      })
      .finally(() => {
        if (current) setChecking(false);
      });
    return () => {
      current = false;
    };
  }, [path, mode, loading]);
  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (busy || checking || loading) return;
    if (opened) {
      setPassword("");
      setConfirm("");
      onOpen(opened);
      return;
    }
    setError("");
    if (mode === "create" && password !== confirm) {
      setError(tr("Passwords do not match."));
      return;
    }
    setBusy(true);
    try {
      let target = path;
      if (!target) {
        target =
          (mode === "create"
            ? await saveFile(
                "ttvault",
                info?.defaultVaultPath ?? "Personal.ttvault",
              )
            : await chooseFile(["ttvault"])) ?? "";
      }
      if (!target) return;
      const v = await call<Vault>(
        mode === "create"
          ? "vault_create"
          : !password && saved
            ? "vault_open_remembered"
            : "vault_open",
        { path: target, password, name },
      );
      try {
        // Preserve existing storage unless the user explicitly opts out. A
        // blank password means the backend already used the saved credential.
        if ((remember && password) || (!remember && rememberChanged))
          await call("vault_remember", { password, enabled: remember });
      } catch (e) {
        setOpened(v);
        setRemember(false);
        setError(errorText(e));
        return;
      }
      setPassword("");
      setConfirm("");
      onOpen(v);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="onboarding">
      <section className="onboard-story">
        <div className="brand">
          <div className="brand-mark">
            <Terminal size={24} />
          </div>
          TermTerm <span className="badge">{tr("DESKTOP")}</span>
        </div>
        <div className="onboard-copy">
          <span className="eyebrow">{tr("YOUR SERVERS. YOUR WORKSPACE.")}</span>
          <h1>
            {tr("A place for every")}
            <br />
            {tr("connection")}
            <span>.</span>
          </h1>
          <p>
            {tr("From your first server to your whole infrastructure.")}
            <br />
            {tr("Keep your connections close, and your keys closer.")}
          </p>
          <div className="terminal-art">
            <div className="art-bar">
              <i />
              <i />
              <i />
              <span>{tr("your next connection")}</span>
            </div>
            <div className="art-body">
              <span className="muted">
                {tr("# A little less setup. A little more doing.")}
              </span>
              <br />
              <b>❯</b> ssh my-server
              <br />
              <span className="muted">
                {tr("Authenticating with your encrypted keychain…")}
              </span>
              <br />
              <span className="success-text">
                {tr("Connection established.")}
              </span>
              <br />
              <br />
              <b>you@server</b> <span className="muted">~</span> ${" "}
              <i className="cursor" />
            </div>
          </div>
          <div className="story-points">
            <span>
              <ShieldCheck size={18} />
              {tr("Encrypted by default")}
            </span>
            <span>
              <HardDrive size={18} />
              {tr("Works offline")}
            </span>
            <span>
              <Database size={18} />
              {tr("Sync on your terms")}
            </span>
          </div>
        </div>
        <small>
          {tr("Your encrypted terminal workspace · v")}
          {info?.version ?? "0.2.0"}
        </small>
      </section>
      <section className="onboard-form">
        {loading ? (
          <Busy label={tr("Opening your saved workspace…")} />
        ) : mode === "welcome" ? (
          <>
            <div className="welcome-symbol">
              <LockKeyhole size={30} />
            </div>
            <h2>{tr("Your workspace starts here")}</h2>
            <p>
              {tr(
                "Create a local vault or bring one from another computer. You can connect PostgreSQL later.",
              )}
            </p>
            {!desktop && (
              <div className="notice">
                {tr(
                  "This is the interface preview. Vaults and connections run in the desktop application.",
                )}
              </div>
            )}
            <button
              className="welcome-option"
              onClick={() => {
                setMode("create");
                setPath("");
              }}
            >
              <div className="option-icon">
                <HardDrive size={22} />
              </div>
              <div>
                <strong>{tr("Create local vault")}</strong>
                <small>{tr("A new encrypted home for your connections")}</small>
              </div>
              <ArrowRight size={18} />
            </button>
            <button
              className="welcome-option"
              onClick={() => {
                setMode("open");
                setPath(info?.recent?.path ?? "");
              }}
            >
              <div className="option-icon">
                <FolderOpen size={22} />
              </div>
              <div>
                <strong>{tr("Open vault file")}</strong>
                <small>{tr("Unlock an existing .ttvault file")}</small>
              </div>
              <ArrowRight size={18} />
            </button>
            <button className="welcome-option" onClick={onRestore}>
              <div className="option-icon">
                <FileArchive size={22} />
              </div>
              <div>
                <strong>{tr("Restore a backup")}</strong>
                <small>{tr("Bring your workspace back from .ttbackup")}</small>
              </div>
              <ArrowRight size={18} />
            </button>
            <div className="privacy-note">
              <ShieldCheck size={16} />
              <span>
                {tr("Your vault password stays with you.")}
                <br />
                {tr("No account or cloud service required.")}
              </span>
            </div>
          </>
        ) : (
          <form onSubmit={submit}>
            <button
              type="button"
              className="text-btn back"
              disabled={busy || !!opened}
              onClick={() => {
                setMode("welcome");
                setError("");
              }}
            >
              <ChevronLeft size={16} /> {tr("Back")}
            </button>
            <div className="welcome-symbol">
              <LockKeyhole size={30} />
            </div>
            <h2>
              {mode === "create"
                ? tr("Create your local vault")
                : tr("Welcome back")}
            </h2>
            <p>
              {mode === "create"
                ? tr(
                    "Choose a password you can keep safe. You will need it to open your vault on another computer.",
                  )
                : tr(
                    "Your connections are encrypted. Unlock your vault to continue.",
                  )}
            </p>
            <fieldset
              disabled={busy || checking || !!opened}
              style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}
            >
              {mode === "create" && (
                <Field
                  label={tr("Vault name")}
                  value={name}
                  onChange={setName}
                  required
                />
              )}
              <div className="file-field">
                <Field
                  label={tr("Vault file")}
                  value={path}
                  onChange={setPath}
                  placeholder={
                    mode === "create"
                      ? tr("Choose where to save your vault")
                      : tr("Select a .ttvault file")
                  }
                />
                <button
                  type="button"
                  className="secondary"
                  onClick={async () => {
                    try {
                      const p =
                        mode === "create"
                          ? await saveFile(
                              "ttvault",
                              info?.defaultVaultPath ?? "Personal.ttvault",
                            )
                          : await chooseFile(["ttvault"]);
                      if (p) setPath(p);
                    } catch (e) {
                      setError(errorText(e));
                    }
                  }}
                >
                  <FolderOpen size={17} />
                </button>
              </div>
              <Field
                label={tr("Vault password")}
                value={password}
                onChange={setPassword}
                type="password"
                hint={
                  mode === "create"
                    ? tr("At least 8 characters. There is no password reset.")
                    : undefined
                }
                required={mode === "create" || !saved}
              />
              {mode === "create" && (
                <Field
                  label={tr("Confirm password")}
                  value={confirm}
                  onChange={setConfirm}
                  type="password"
                  required
                />
              )}
              <Check
                checked={remember}
                onChange={(value) => {
                  setRemember(value);
                  setRememberChanged(true);
                }}
              >
                {tr("Remember password using {store}", {
                  store:
                    info?.platform?.rememberStore ??
                    tr("secure system storage"),
                })}
              </Check>
              {saved && (
                <p className="muted">
                  {tr(
                    "Password saved on this device. Leave the password field empty to unlock with it.",
                  )}
                </p>
              )}
            </fieldset>
            {error && <div className="notice error">{error}</div>}
            <button disabled={busy || checking} className="primary full">
              {busy ? (
                <Busy label={tr("Unlocking encrypted storage…")} />
              ) : (
                <>
                  {opened
                    ? tr("Continue without remembering password")
                    : mode === "create"
                      ? tr("Create vault")
                      : tr("Unlock vault")}
                  <ArrowRight size={17} />
                </>
              )}
            </button>
          </form>
        )}
      </section>
    </div>
  );
}
