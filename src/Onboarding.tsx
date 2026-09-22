import { useState } from "react";
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
  onOpen,
  onRestore,
}: {
  info?: AppInfo;
  onOpen: (v: Vault) => void;
  onRestore: () => void;
}) {
  const [mode, setMode] = useState<"welcome" | "create" | "open">("welcome");
  const [path, setPath] = useState("");
  const [name, setName] = useState("Personal vault");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [opened, setOpened] = useState<Vault | null>(null);
  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (opened) {
      setPassword("");
      setConfirm("");
      onOpen(opened);
      return;
    }
    setError("");
    if (mode === "create" && password !== confirm) {
      setError("Passwords do not match.");
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
        mode === "create" ? "vault_create" : "vault_open",
        { path: target, password, name },
      );
      try {
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
          TermTerm <span className="badge">DESKTOP</span>
        </div>
        <div className="onboard-copy">
          <span className="eyebrow">YOUR SERVERS. YOUR WORKSPACE.</span>
          <h1>
            A place for every
            <br />
            connection<span>.</span>
          </h1>
          <p>
            From your first server to your whole infrastructure.
            <br />
            Keep your connections close, and your keys closer.
          </p>
          <div className="terminal-art">
            <div className="art-bar">
              <i />
              <i />
              <i />
              <span>your next connection</span>
            </div>
            <div className="art-body">
              <span className="muted">
                # A little less setup. A little more doing.
              </span>
              <br />
              <b>❯</b> ssh my-server
              <br />
              <span className="muted">
                Authenticating with your encrypted keychain…
              </span>
              <br />
              <span className="success-text">Connection established.</span>
              <br />
              <br />
              <b>you@server</b> <span className="muted">~</span> ${" "}
              <i className="cursor" />
            </div>
          </div>
          <div className="story-points">
            <span>
              <ShieldCheck size={18} />
              Encrypted by default
            </span>
            <span>
              <HardDrive size={18} />
              Works offline
            </span>
            <span>
              <Database size={18} />
              Sync on your terms
            </span>
          </div>
        </div>
        <small>
          Your encrypted terminal workspace · v{info?.version ?? "0.2.0"}
        </small>
      </section>
      <section className="onboard-form">
        {mode === "welcome" ? (
          <>
            <div className="welcome-symbol">
              <LockKeyhole size={30} />
            </div>
            <h2>Your workspace starts here</h2>
            <p>
              Create a local vault or bring one from another computer. You can
              connect PostgreSQL later.
            </p>
            {!desktop && (
              <div className="notice">
                This is the interface preview. Vaults and connections run in the
                desktop application.
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
                <strong>Create local vault</strong>
                <small>A new encrypted home for your connections</small>
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
                <strong>Open vault file</strong>
                <small>Unlock an existing .ttvault file</small>
              </div>
              <ArrowRight size={18} />
            </button>
            <button className="welcome-option" onClick={onRestore}>
              <div className="option-icon">
                <FileArchive size={22} />
              </div>
              <div>
                <strong>Restore a backup</strong>
                <small>Bring your workspace back from .ttbackup</small>
              </div>
              <ArrowRight size={18} />
            </button>
            <div className="privacy-note">
              <ShieldCheck size={16} />
              <span>
                Your vault password stays with you.
                <br />
                No account or cloud service required.
              </span>
            </div>
          </>
        ) : (
          <form onSubmit={submit}>
            <button
              type="button"
              className="text-btn back"
              onClick={() => {
                setMode("welcome");
                setError("");
              }}
            >
              <ChevronLeft size={16} /> Back
            </button>
            <div className="welcome-symbol">
              <LockKeyhole size={30} />
            </div>
            <h2>
              {mode === "create" ? "Create your local vault" : "Welcome back"}
            </h2>
            <p>
              {mode === "create"
                ? "Choose a password you can keep safe. You will need it to open your vault on another computer."
                : "Your connections are encrypted. Unlock your vault to continue."}
            </p>
            {mode === "create" && (
              <Field
                label="Vault name"
                value={name}
                onChange={setName}
                required
              />
            )}
            <div className="file-field">
              <Field
                label="Vault file"
                value={path}
                onChange={setPath}
                placeholder={
                  mode === "create"
                    ? "Choose where to save your vault"
                    : "Select a .ttvault file"
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
              label="Vault password"
              value={password}
              onChange={setPassword}
              type="password"
              hint={
                mode === "create"
                  ? "At least 8 characters. There is no password reset."
                  : undefined
              }
              required
            />
            {mode === "create" && (
              <Field
                label="Confirm password"
                value={confirm}
                onChange={setConfirm}
                type="password"
                required
              />
            )}
            <Check checked={remember} onChange={setRemember}>
              Remember password using{" "}
              {info?.platform?.rememberStore ?? "secure system storage"}
            </Check>
            {mode === "open" && (
              <button
                type="button"
                className="secondary full"
                disabled={busy || !path}
                onClick={async () => {
                  setBusy(true);
                  try {
                    onOpen(
                      await call<Vault>("vault_open_remembered", { path }),
                    );
                  } catch (e) {
                    setError(errorText(e));
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                Unlock with remembered password
              </button>
            )}
            {error && <div className="notice error">{error}</div>}
            <button disabled={busy} className="primary full">
              {busy ? (
                <Busy label="Unlocking encrypted storage…" />
              ) : (
                <>
                  {opened
                    ? "Continue without remembering password"
                    : mode === "create"
                      ? "Create vault"
                      : "Unlock vault"}
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
