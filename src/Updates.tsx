import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, desktop, errorText } from "./api";
import { Modal } from "./components";

type Info = { version: string; channel: string; enabled: boolean; repository: string; packageSupported: boolean };
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
    void call<Info>("update_info").then((i) => { if (mounted) setInfo(i); }).catch((e) => { if (mounted) setError(errorText(e)); });
    const unlisten = listen<{phase:string; received?:number; total?:number}>("update-progress", ({ payload: p }) => {
      if (p.phase === "vault-locked") { window.location.reload(); return; }
      if (mounted) setMessage(p.phase === "installing" ? "Installing verified update. Restarting…" : `Downloading: ${((p.received ?? 0) / 1048576).toFixed(1)} MB${p.total ? ` / ${(p.total / 1048576).toFixed(1)} MB` : ""}`);
    });
    return () => { mounted = false; void unlisten.then((fn) => fn()); };
  }, []);
  async function execute(install: boolean) {
    if (working.current) return;
    working.current = true; setBusy(true); setError(""); setMessage(install ? "Downloading and verifying signature…" : "Checking approved releases…");
    try {
      if (install) await call("update_install", { version: release?.version });
      else { const result = await call<Release>("update_check"); setRelease(result); setMessage(result.available ? `Version ${result.version} is available.` : "You are up to date."); }
    } catch (e) { setError(errorText(e)); setMessage(""); }
    finally { working.current = false; setBusy(false); setConfirm(false); }
  }
  return <div className="settings-card">
    <h2>Application updates</h2>
    <p>{info ? `TermTerm ${info.version} · ${info.channel}` : "Open the desktop application to check updates."}</p>
    <p className="muted">Only owner-approved stable releases are offered. Updates are signature-verified before installation. Your encrypted vault files are kept.</p>
    {info && !info.enabled && <p>Development build: stable updates are disabled.</p>}
    {info && !info.packageSupported && <p>For a .deb or .rpm installation, download the matching package from GitHub Releases and install it using your package manager. AppImage installations support in-app updates.</p>}
    <p><a href="https://github.com/EnezMutluoglu/TermTerm/releases" target="_blank" rel="noreferrer">GitHub Releases</a></p>
    <button className="primary" disabled={!info?.enabled || busy} onClick={() => void execute(false)}>Check for updates</button>
    {release?.available && <>
      <h3>TermTerm {release.version}</h3>
      <pre style={{whiteSpace:"pre-wrap"}}>{release.notes}</pre>
      <button className="primary" disabled={busy || !info?.packageSupported} onClick={() => setConfirm(true)}>Download and install</button>
    </>}
    {message && <p role="status">{message}</p>}
    {error && <p role="alert" className="error-banner">{error}</p>}
    {confirm && <Modal title={`Install TermTerm ${release?.version}`} onClose={() => { if (!busy) setConfirm(false); }}>
      <p>After the download is verified, terminal and SFTP connections will close, the vault will lock, and TermTerm will restart. Finish or cancel file transfers and imports first.</p>
      {message && <p role="status">{message}</p>}
      <button disabled={busy} onClick={() => setConfirm(false)}>Cancel</button>
      <button className="primary" disabled={busy} onClick={() => void execute(true)}>{busy ? "Updating…" : "Install and restart"}</button>
    </Modal>}
  </div>;
}
