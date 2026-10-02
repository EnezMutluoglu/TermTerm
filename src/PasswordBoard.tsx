import { useEffect, useRef, useState } from "react";
import { Copy, Eye, EyeOff, RefreshCw, Trash2 } from "lucide-react";
import { call, errorText } from "./api";
import { copyText } from "./clipboard";
import { Modal, Check } from "./components";
import "./passwordBoard.css";

type Options = { length: number; lowercase: boolean; uppercase: boolean; digits: boolean; punctuation: boolean; symbols: boolean; brackets: boolean; excludeSimilar: boolean };
type Entry = { id: string; createdAt: string; password: string };
type Snapshot = { context: string; options: Options; entries: Entry[] };
const defaults: Options = { length: 21, lowercase: true, uppercase: true, digits: true, punctuation: true, symbols: false, brackets: false, excludeSimilar: false };
const classes = [
  ["lowercase", "Küçük harf", "a–z"], ["uppercase", "Büyük harf", "A–Z"],
  ["digits", "Rakam", "0–9"], ["punctuation", "Noktalama", ". , ; : ! ?"],
  ["symbols", "Sembol", "@ # $ % ^ & * + - _ = ~ | / \\"], ["brackets", "Parantez", "( ) [ ] { } < >"],
] as const;

export default function PasswordBoard({ onClose, team }: { onClose: () => void; team: boolean }) {
  const [board, setBoard] = useState<Snapshot>();
  const [options, setOptions] = useState(defaults);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [visible, setVisible] = useState<string>();
  const [confirmClear, setConfirmClear] = useState(false);
  const alive = useRef(true), running = useRef(false);
  useEffect(() => {
    alive.current = true;
    call<Snapshot>("password_board", { action: { kind: "list" } }).then((b) => {
      if (alive.current) { setBoard(b); setOptions(b.options); }
    }).catch((e) => { if (alive.current) setError(errorText(e)); })
      .finally(() => { if (alive.current) setBusy(false); });
    return () => { alive.current = false; };
  }, []);
  async function run(action: Record<string, unknown>) {
    if (running.current || !board) return;
    running.current = true; setBusy(true); setError(""); setNotice("");
    try {
      const result = await call<Snapshot>("password_board", { context: board.context, action });
      if (alive.current) { setBoard(result); setVisible(undefined); setConfirmClear(false); }
    } catch (e) { if (alive.current) setError(errorText(e)); }
    finally { running.current = false; if (alive.current) setBusy(false); }
  }
  async function copy(entry: Entry) {
    try { await copyText(entry.password); if (alive.current) setNotice("Parola panoya kopyalandı."); }
    catch (e) { if (alive.current) setError(errorText(e)); }
  }
  const selected = classes.filter(([key]) => options[key]).length;
  const valid = Number.isInteger(options.length) && options.length >= Math.max(4, selected) && options.length <= 256 && selected > 0;
  return <Modal title="Parola üretici ve panosu" onClose={onClose} wide>
    <div className="password-board">
      <p className="password-board-note">{team ? "Bu hesaba özel, cihazdaki şifreli geçmiş. Takıma veya PostgreSQL’e gönderilmez." : "Geçmiş bu kasada şifreli saklanır. Kasa kilitlenince pano kapanır."} Son 50 parola saklanır; 51. üretimde en eski kayıt silinir.</p>
      <fieldset disabled={busy || !board} className="password-options">
        <label className="field password-length"><span>Karakter sayısı</span><input aria-label="Karakter sayısı" type="number" min={4} max={256} value={Number.isNaN(options.length) ? "" : options.length} onChange={(e) => setOptions({ ...options, length: e.target.valueAsNumber })} /></label>
        <div className="password-classes">{classes.map(([key, label, examples]) => <Check key={key} checked={options[key]} onChange={(v) => setOptions({ ...options, [key]: v })}><strong>{label}</strong><small>{examples}</small></Check>)}</div>
        <Check checked={options.excludeSimilar} onChange={(v) => setOptions({ ...options, excludeSimilar: v })}>Benzer karakterleri kullanma (I l 1 O 0 o |)</Check>
        <small>Seçilen her türden en az bir karakter kullanılır. Boşluk ve tırnak eklenmez.</small>
        <button className="primary" disabled={!valid} onClick={() => void run({ kind: "generate", options })}><RefreshCw size={16} />{busy ? "Üretiliyor…" : "Parola üret ve kaydet"}</button>
      </fieldset>
      {error && <p role="alert" className="error-box">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <div className="password-history-heading"><h3>Parola geçmişi <small>{board?.entries.length ?? 0}/50</small></h3><button disabled={busy || !board?.entries.length} onClick={() => setConfirmClear(true)}><Trash2 size={14} />Geçmişi temizle</button></div>
      {confirmClear && <div className="password-clear" role="alert"><span>Kaydedilen tüm parolalar silinsin mi?</span><button disabled={busy} onClick={() => void run({ kind: "clear" })}>Tümünü sil</button><button onClick={() => setConfirmClear(false)}>Vazgeç</button></div>}
      <div className="password-history" aria-label="Parola geçmişi">{board?.entries.map((entry) => <div className="password-history-entry" key={entry.id}>
        <div className="password-entry-value"><time dateTime={entry.createdAt}>{new Date(entry.createdAt).toLocaleString("tr-TR")}</time><code>{visible === entry.id ? entry.password : "••••••••••••••••"}</code><small>{entry.password.length} karakter</small></div>
        <button className="icon-btn" aria-label={visible === entry.id ? "Parolayı gizle" : "Parolayı göster"} onClick={() => setVisible(visible === entry.id ? undefined : entry.id)}>{visible === entry.id ? <EyeOff size={17} /> : <Eye size={17} />}</button>
        <button className="icon-btn" aria-label="Parolayı kopyala" onClick={() => void copy(entry)}><Copy size={17} /></button>
        <button className="icon-btn" aria-label="Parolayı sil" disabled={busy} onClick={() => void run({ kind: "delete", id: entry.id })}><Trash2 size={17} /></button>
      </div>)}</div>
      {!busy && !board?.entries.length && <p className="password-board-note">Henüz üretilmiş parola yok.</p>}
      <small className="password-board-note">Geçmiş yereldir; bağlantı exportlarına ve .ttbackup dosyalarına eklenmez. Kişisel kasanın taşınabilir .ttvault kopyası geçmişi de içerir.</small>
    </div>
  </Modal>;
}
