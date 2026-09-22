import { tr } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, desktop, errorText } from "./api";
import { Modal } from "./components";

type Info = {
  version: string;
  channel: string;
  enabled: boolean;
  repository: string;
  packageSupported: boolean;
};
type Release = { available: boolean; version?: string; notes?: string };
export default function Updates() {
  const [info, setInfo] = useState<Info>();
  const [release, setRelease] = useState<Release>();
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const working = useRef(false);
  useEffect(() => {
    if (!desktop) return;
    let mounted = true;
    void call<Info>("update_info")
      .then((i) => {
        if (mounted) setInfo(i);
      })
      .catch((e) => {
        if (mounted) setError(errorText(e));
      });
    const unlisten = listen<{
      phase: string;
      received?: number;
      total?: number;
    }>("update-progress", ({ payload: p }) => {
      if (p.phase === "vault-locked") {
        window.location.reload();
        return;
      }
      if (mounted)
        setMessage(
          p.phase === "installing"
            ? tr("Installing verified update. Restarting…")
            : tr("Downloading: {progress}", {
                progress: `${((p.received ?? 0) / 1048576).toFixed(1)} MB${p.total ? ` / ${(p.total / 1048576).toFixed(1)} MB` : ""}`,
              }),
        );
    });
    return () => {
      mounted = false;
      void unlisten.then((fn) => fn());
    };
  }, []);
  async function execute(install: boolean) {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setError("");
    setMessage(
      install
        ? tr("Downloading and verifying signature…")
        : tr("Checking approved releases…"),
    );
    try {
      if (install) await call("update_install", { version: release?.version });
      else {
        const result = await call<Release>("update_check");
        setRelease(result);
        setMessage(
          result.available
            ? tr("Version {version} is available.", {
                version: result.version ?? "",
              })
            : tr("You are up to date."),
        );
      }
    } catch (e) {
      setError(errorText(e));
      setMessage("");
    } finally {
      working.current = false;
      setBusy(false);
      setConfirm(false);
    }
  }
  return (
    <div className="settings-card">
      <h2>{tr("Application updates")}</h2>
      <p>
        {info
          ? `TermTerm ${info.version} · ${tr(info.channel[0].toUpperCase() + info.channel.slice(1))}`
          : tr("Open the desktop application to check updates.")}
      </p>
      <p className="muted">
        {tr(
          "Only owner-approved stable releases are offered. Updates are signature-verified before installation. Your encrypted vault files are kept.",
        )}
      </p>
      {info && !info.enabled && (
        <p>{tr("Development build: stable updates are disabled.")}</p>
      )}
      {info && !info.packageSupported && (
        <p>
          {tr(
            "For a .deb or .rpm installation, download the matching package from GitHub Releases and install it using your package manager. AppImage installations support in-app updates.",
          )}
        </p>
      )}
      <p>
        <a
          href="https://github.com/EnezMutluoglu/TermTerm/releases"
          target="_blank"
          rel="noreferrer"
        >
          {tr("GitHub Releases")}
        </a>
      </p>
      <button
        className="primary"
        disabled={!info?.enabled || busy}
        onClick={() => void execute(false)}
      >
        {tr("Check for updates")}
      </button>
      {release?.available && (
        <>
          <h3>TermTerm {release.version}</h3>
          <pre style={{ whiteSpace: "pre-wrap" }}>{release.notes}</pre>
          <button
            className="primary"
            disabled={busy || !info?.packageSupported}
            onClick={() => setConfirm(true)}
          >
            {tr("Download and install")}
          </button>
        </>
      )}
      {message && <p role="status">{message}</p>}
      {error && (
        <p role="alert" className="error-banner">
          {error}
        </p>
      )}
      {confirm && (
        <Modal
          title={tr("Install TermTerm {version}", {
            version: release?.version ?? "",
          })}
          onClose={() => {
            if (!busy) setConfirm(false);
          }}
        >
          <p>
            {tr(
              "After the download is verified, terminal and SFTP connections will close, the vault will lock, and TermTerm will restart. Finish or cancel file transfers and imports first.",
            )}
          </p>
          {message && <p role="status">{message}</p>}
          <button disabled={busy} onClick={() => setConfirm(false)}>
            {tr("Cancel")}
          </button>
          <button
            className="primary"
            disabled={busy}
            onClick={() => void execute(true)}
          >
            {busy ? tr("Updating…") : tr("Install and restart")}
          </button>
        </Modal>
      )}
    </div>
  );
}
