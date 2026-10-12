import { useEffect, useRef, useState } from "react";
import { call, errorText } from "./api";

type AgentKey = { agentKey: string; label: string; certificate?: boolean };
export default function TeamAgent({ onChange }: { onChange: (value: string) => void }) {
  const windows = navigator.userAgent.includes("Windows");
  const [provider, setProvider] = useState(windows ? "openssh" : "unix");
  const [keys, setKeys] = useState<AgentKey[]>([]);
  const [selected, setSelected] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  useEffect(() => () => { generation.current++; }, []);
  async function load() {
    const current = ++generation.current;
    setBusy(true); setError(""); setKeys([]); setSelected(""); onChange("");
    try {
      const result = await call<AgentKey[]>("agent_identities", { kind: provider });
      if (current !== generation.current) return;
      setKeys(result);
      if (!result.length) setError("Agent açık fakat yüklü anahtar bulunamadı.");
    } catch (e) { if (current === generation.current) setError(errorText(e)); }
    finally { if (current === generation.current) setBusy(false); }
  }
  return <>
    <label>İmzalama agent’ı<select aria-label="İmzalama agent’ı" disabled={busy} value={provider} onChange={e => {
      generation.current++; setProvider(e.target.value); setKeys([]); setSelected(""); onChange(""); setError("");
    }}>
      {windows ? <><option value="openssh">Windows OpenSSH</option><option value="pageant">Pageant</option></> : <option value="unix">SSH_AUTH_SOCK</option>}
    </select></label>
    <button type="button" disabled={busy} onClick={() => void load()}>{busy ? "Anahtarlar okunuyor…" : "Agent anahtarlarını listele"}</button>
    {error && <p role="alert">{error}</p>}
    <label>İmzalama anahtarı<select aria-label="İmzalama anahtarı" required value={selected} disabled={busy} onChange={e => {
      setSelected(e.target.value);
      onChange(e.target.value ? JSON.stringify({ format: 1, provider, fingerprint: e.target.value }) : "");
    }}>
      <option value="">Anahtar seç</option>
      {keys.map(k => <option key={k.agentKey} value={k.agentKey}>{k.label || "SSH anahtarı"}{k.certificate ? " · Sertifika" : ""} · {k.agentKey}</option>)}
    </select></label>
    <small>Özel anahtar cihazdaki agent’ta kalır. Yalnız seçilen parmak izi kullanılır; başka anahtara geçilmez. Yeni cihazda aynı agent ve anahtar gerekir. Donanım desteği agent’ın yeteneklerine bağlıdır.</small>
  </>;
}
