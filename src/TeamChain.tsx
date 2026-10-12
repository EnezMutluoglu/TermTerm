import { useEffect, useRef, useState } from "react";
import type { TeamHostRow } from "./TeamHosts";

type Preview = { planId: string; recipientCount: number; hops: { id: string; name: string; revision: number }[] };
export default function TeamChain({ host, rows, rpc, onClose, onSaved }: {
  host: TeamHostRow; rows: TeamHostRow[];
  rpc: (action: Record<string, unknown>) => Promise<any>;
  onClose: () => void; onSaved: () => void;
}) {
  const [hops, setHops] = useState<string[]>([]), [choice, setChoice] = useState("");
  const [busy, setBusy] = useState(true), [error, setError] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const mounted = useRef(true), gate = useRef(false);
  useEffect(() => {
    mounted.current = true;
    void rpc({ kind: "chainConfiguration", id: host.id }).then(v => {
      if (mounted.current) setHops(v.hops);
    }).catch(e => { if (mounted.current) setError(String(e)); })
      .finally(() => { if (mounted.current) setBusy(false); });
    return () => { mounted.current = false; };
  }, [host.id]);
  async function run(fn: () => Promise<void>) {
    if (gate.current) return;
    gate.current = true; setBusy(true); setError("");
    try { await fn(); } catch (e) { if (mounted.current) setError(String(e)); }
    finally { gate.current = false; if (mounted.current) setBusy(false); }
  }
  const candidates = rows.filter(r => r.id !== host.id && r.content?.kind === "host" && r.content.protocol === "ssh" && !hops.includes(r.id));
  return <div className="ts-overlay"><section className="ts-dialog" role="dialog" aria-modal="true" aria-label="Host chain">
    <h2>Host chain · {host.content?.name}</h2>
    <p>İlk jump hosttan hedefe doğru tam bağlantı sırasını seç. Her adım kendi kişisel bağlantı profilini kullanır.</p>
    {error && <div role="alert" className="ts-message error">{error}</div>}
    <ol>{hops.map((id, index) => <li key={id}>
      <span>{preview?.hops.find(r => r.id === id)?.name ?? rows.find(r => r.id === id)?.content?.name ?? "Listede bulunmayan jump host · önizlemede doğrula"}</span>{" "}
      <button disabled={busy || !!preview || index === 0} aria-label={`Hop ${index + 1} yukarı`} onClick={() => setHops(old => {
        const next = [...old]; [next[index - 1], next[index]] = [next[index], next[index - 1]]; return next;
      })}>↑</button>{" "}
      <button disabled={busy || !!preview} onClick={() => setHops(old => old.filter(v => v !== id))}>Kaldır</button>
    </li>)}<li><strong>{host.content?.name} (hedef)</strong></li></ol>
    <label>Jump host<select aria-label="Jump host" disabled={busy || !!preview || hops.length >= 15} value={choice} onChange={e => setChoice(e.target.value)}>
      <option value="">Host seç…</option>{candidates.map(r => <option key={r.id} value={r.id}>{r.content?.name}</option>)}
    </select></label>
    <button disabled={busy || !!preview || !choice || hops.length >= 15} onClick={() => { setHops(old => [...old, choice]); setChoice(""); }}>Sıraya ekle</button>
    {preview && <div className="ts-message">
      <strong>{preview.hops.length} jump host · {preview.recipientCount} yetkili kullanıcı</strong>
      <p>Jump host bilgileri yalnız bu hedefe bağlantı için paylaşılır; bağımsız erişim izni verilmez. Önizleme 15 dakika geçerlidir.</p>
      <p>Jump host kaydı veya alıcı kimliği değişirse chain anahtarlarının yeni önizlemeyle yenilenmesi gerekebilir.</p>
    </div>}
    <div className="ts-actions">
      <button disabled={busy} onClick={() => void run(async () => {
        if (preview) await rpc({ kind: "finishChain", planId: preview.planId, cancel: true }); onClose();
      })}>İptal</button>
      {!preview ? <button className="ts-primary" disabled={busy} onClick={() => void run(async () => {
        const value = await rpc({ kind: "prepareChain", id: host.id, hops }); if (mounted.current) setPreview(value);
      })}>Erişim önizlemesi</button> : <button className="ts-primary" disabled={busy} onClick={() => void run(async () => {
        await rpc({ kind: "finishChain", planId: preview.planId, cancel: false }); onSaved(); onClose();
      })}>Onayla ve kaydet</button>}
    </div>
  </section></div>;
}
