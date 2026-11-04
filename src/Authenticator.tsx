import { useEffect, useRef, useState } from "react";
import { Copy, Download, KeyRound, Plus, QrCode, RefreshCw, Search, Share2, ShieldCheck, Trash2 } from "lucide-react";
import jsQR from "jsqr";
import { call, errorText } from "./api";
import { copyText } from "./clipboard";
import { Field, Modal, Select } from "./components";
import "./authenticator.css";

export type AuthenticatorEntry = {
  id: string; revision: number; label: string; issuer: string;
  algorithm: string; digits: number; period: number; code: string | null;
  validUntil: number; visibility: string; permissions: string[];
  canRemove: boolean; canUpdate: boolean; unavailable?: boolean;
};
type Snapshot = { context: string; entries: AuthenticatorEntry[]; serverTime: number };
type Input = { provisioning: string; label: string; issuer: string; algorithm: string; digits: number; period: number };
const emptyInput = (): Input => ({ provisioning: "", label: "", issuer: "", algorithm: "SHA1", digits: 6, period: 30 });

// Image pixels and provisioning stay in this window; no remote QR service is used.
async function readQr(file: File): Promise<string> {
  if (!["image/png", "image/jpeg", "image/webp"].includes(file.type) || file.size > 4 * 1024 * 1024)
    throw new Error("En fazla 4 MB boyutunda PNG, JPEG veya WebP görseli seçin.");
  let image: ImageBitmap;
  try { image = await createImageBitmap(file); }
  catch { throw new Error("QR görseli açılamadı."); }
  try {
    if (image.width > 4096 || image.height > 4096 || image.width * image.height > 12_000_000)
      throw new Error("QR görseli en fazla 4096 × 4096 ve 12 milyon piksel olabilir.");
    const canvas = document.createElement("canvas");
    canvas.width = image.width; canvas.height = image.height;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) throw new Error("Bu cihazda QR görseli okunamıyor.");
    context.drawImage(image, 0, 0);
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
    const result = jsQR(pixels.data, canvas.width, canvas.height);
    pixels.data.fill(0); canvas.width = 0; canvas.height = 0;
    if (!result) throw new Error("Görselde okunabilir bir QR kodu bulunamadı.");
    if (result.data.length > 4096) throw new Error("QR kurulum verisi çok uzun.");
    if (result.data.startsWith("otpauth-migration:"))
      throw new Error("Google Authenticator taşıma QR kodu desteklenmiyor. Her hesap için ayrı otpauth TOTP kurulum kodu kullanın.");
    if (!result.data.startsWith("otpauth://totp/"))
      throw new Error("QR kodu standart bir otpauth TOTP kurulumu içermiyor. HOTP desteklenmiyor.");
    return result.data;
  } finally { image.close(); }
}

export default function Authenticator({ team = false, visible, onShare, onExport, onRecover }: {
  team?: boolean; visible: boolean;
  onShare?: (row: AuthenticatorEntry) => void;
  onExport?: (row: AuthenticatorEntry) => void;
  onRecover?: (row: AuthenticatorEntry) => void;
}) {
  const [snapshot, setSnapshot] = useState<Snapshot>();
  const [now, setNow] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [input, setInput] = useState<Input>();
  const [qrBusy, setQrBusy] = useState(false);
  const [formError, setFormError] = useState("");
  const [remove, setRemove] = useState<AuthenticatorEntry>();
  const [revealed, setRevealed] = useState<{ id: string; revision: number; uri: string }>();
  const scope = useRef(0), running = useRef(false), qrVersion = useRef(0);
  const current = useRef<{ snapshot?: Snapshot; received: number }>({ received: 0 });
  const visibleRef = useRef(visible); visibleRef.current = visible;
  const context = useRef<string | undefined>(undefined);
  const clock = () => current.current.snapshot ? current.current.snapshot.serverTime + (performance.now() - current.current.received) / 1000 : 0;
  const isCurrent = (generation: number) => scope.current === generation && visibleRef.current;
  function clearSetup() { qrVersion.current++; setInput(undefined); setFormError(""); setQrBusy(false); }
  function hideCodes() {
    const value = current.current.snapshot;
    if (value) { const cleared = { ...value, entries: value.entries.map((entry) => ({ ...entry, code: null })) }; current.current.snapshot = cleared; setSnapshot(cleared); }
    setRevealed(undefined); clearSetup();
  }
  async function request(action: Record<string, unknown>, generation = scope.current): Promise<Snapshot | { uri: string } | undefined> {
    if (running.current || !isCurrent(generation)) return;
    running.current = true; setBusy(true);
    const args = { context: context.current, action };
    try {
      const result = await (team
        ? call<Snapshot | { uri: string }>("team_v2_action", { action: { kind: "authenticator", ...args } })
        : call<Snapshot | { uri: string }>("authenticator", args));
      if (!isCurrent(generation)) return;
      if ("entries" in result) {
        if (context.current && result.context !== context.current) throw new Error("Oturum değişti. Authenticator sekmesini yeniden açın.");
        context.current = result.context;
        current.current = { snapshot: result, received: performance.now() };
        setSnapshot(result); setNow(result.serverTime);
        setRevealed((value) => value && result.entries.some((entry) => entry.id === value.id && entry.revision === value.revision && entry.permissions.includes("reveal")) ? value : undefined);
      }
      setError("");
      return result;
    } catch (e) {
      if (isCurrent(generation)) { hideCodes(); setError(errorText(e)); }
      return;
    } finally {
      if (isCurrent(generation)) { running.current = false; setBusy(false); }
    }
  }
  useEffect(() => {
    const generation = ++scope.current;
    running.current = false; context.current = undefined; current.current = { received: 0 };
    setSnapshot(undefined); setError(""); setNotice(""); setBusy(false); setQuery(""); setFilter("all"); setRemove(undefined); setRevealed(undefined); clearSetup();
    if (!visible) return;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      if (!isCurrent(generation)) return;
      if (running.current) { timer = setTimeout(poll, 250); return; }
      await request({ kind: "list" }, generation);
      if (!isCurrent(generation)) return;
      const value = current.current.snapshot;
      const expiry = value?.entries.filter((entry) => entry.code).map((entry) => entry.validUntil - clock()) ?? [];
      timer = setTimeout(poll, expiry.length ? Math.max(250, Math.min(300_000, Math.min(...expiry) * 1000)) : 30_000);
    }
    void poll();
    const ticker = setInterval(() => { if (isCurrent(generation)) setNow(clock()); }, 250);
    return () => { ++scope.current; qrVersion.current++; clearTimeout(timer); clearInterval(ticker); current.current = { received: 0 }; context.current = undefined; };
  }, [visible, team]);

  async function copy(entry: AuthenticatorEntry) {
    const generation = scope.current, expectedContext = context.current;
    setNotice("");
    const result = await request({ kind: "list" }, generation);
    if (!result || !("entries" in result) || !isCurrent(generation)) return;
    const fresh = result.entries.find((row) => row.id === entry.id);
    if (!fresh?.code || !fresh.permissions.includes("connect") || fresh.validUntil <= clock()) { setError("Kod kullanılamıyor veya süresi doldu."); return; }
    const canCopy = () => isCurrent(generation) && context.current === expectedContext && fresh.validUntil > clock() && !!current.current.snapshot?.entries.some((row) => row.id === fresh.id && row.revision === fresh.revision && row.code === fresh.code && row.permissions.includes("connect"));
    try {
      await copyText(fresh.code, canCopy);
      if (canCopy()) setNotice("Kod panoya kopyalandı.");
    } catch { if (isCurrent(generation)) setError("Kod panoya kopyalanamadı."); }
  }
  async function add() {
    if (!input) return;
    const generation = scope.current;
    const result = await request({ kind: "add", input });
    if (result && isCurrent(generation)) { clearSetup(); setNotice("TOTP hesabı eklendi."); }
  }
  async function importQr(file?: File) {
    if (!file || !input) return;
    const generation = scope.current, version = ++qrVersion.current;
    setQrBusy(true); setFormError("");
    // A failed new selection must never leave an earlier setup secret ready to save.
    setInput({ ...input, provisioning: "" });
    try {
      const provisioning = await readQr(file);
      if (isCurrent(generation) && version === qrVersion.current) { setInput((value) => value ? { ...value, provisioning } : value); setNotice("QR kodu cihazda okundu; eklemek için kaydedin."); }
    } catch (e) { if (isCurrent(generation) && version === qrVersion.current) setFormError(errorText(e)); }
    finally { if (isCurrent(generation) && version === qrVersion.current) setQrBusy(false); }
  }
  async function reveal(entry: AuthenticatorEntry) {
    const generation = scope.current;
    const result = await request({ kind: "reveal", id: entry.id });
    if (result && "uri" in result && isCurrent(generation)) setRevealed({ id: entry.id, revision: entry.revision, uri: result.uri });
  }
  if (!visible) return null;
  const entries = (snapshot?.entries ?? []).filter((entry) => `${entry.issuer} ${entry.label}`.toLocaleLowerCase("tr").includes(query.toLocaleLowerCase("tr")) && (filter === "all" || entry.visibility === filter));
  const manual = input && !input.provisioning.trim().includes(":");
  return <section className="authenticator-workspace" aria-label="Authenticator">
    <header className="authenticator-heading"><div className="authenticator-title"><span className="authenticator-icon"><ShieldCheck size={25} /></span><div><h1>Authenticator</h1><p>{team ? "Size özel ve sizinle paylaşılan TOTP hesapları" : "Kasanızdaki TOTP hesapları"}</p></div></div><button className="primary" disabled={busy || !snapshot} onClick={() => { setInput(emptyInput()); setFormError(""); setNotice(""); }}><Plus size={17} />Hesap ekle</button></header>
    <p className="authenticator-note">Koda tıklayarak kopyalayın, giriş ekranındaki tek kullanımlık kod alanına yapıştırın. Cihaz saatiniz doğru olmalıdır.</p>
    <div className="authenticator-toolbar"><div className="search-field"><Search size={17} /><input aria-label="Authenticator hesaplarında ara" placeholder="Hesap veya sağlayıcı ara…" value={query} onChange={(event) => setQuery(event.target.value)} /></div>{team && <select aria-label="Hesap görünürlüğü" value={filter} onChange={(event) => setFilter(event.target.value)}><option value="all">Tüm hesaplar</option><option value="private">Bana özel</option><option value="shared">Paylaşılan</option></select>}<span>{entries.length} hesap</span><button aria-label="Kodları yenile" disabled={busy} onClick={() => void request({ kind: "list" })}><RefreshCw size={17} className={busy ? "spin" : ""} /></button></div>
    {error && <p className="error-box" role="alert">{error}</p>}
    {notice && <p className="authenticator-notice" role="status">{notice}</p>}
    <div className="authenticator-grid">{entries.map((entry) => {
      const seconds = Math.max(0, Math.ceil(entry.validUntil - now));
      const usable = !!entry.code && seconds > 0 && entry.permissions.includes("connect");
      return <article key={entry.id} className="authenticator-card" aria-label={entry.label}>
        <div className="authenticator-card-heading"><span className="authenticator-account-icon"><KeyRound size={19} /></span><div><h2>{entry.issuer || entry.label}</h2><p>{entry.issuer ? entry.label : "TOTP hesabı"}</p></div>{team && <span className="authenticator-scope">{entry.visibility === "shared" ? "Paylaşılan" : "Özel"}</span>}</div>
        <button className="authenticator-code" aria-label={`${entry.label} kodunu kopyala`} disabled={busy || !usable} onClick={() => void copy(entry)}><code>{usable ? entry.code!.replace(entry.digits === 8 ? /(.{4})(?=.)/g : /(.{3})(?=.)/g, "$1 ") : "— — — — — —"}</code><Copy size={18} /></button>
        <div className={"authenticator-countdown" + (seconds <= 5 ? " expiring" : "")}><progress aria-label={`${entry.label} kod süresi`} value={usable ? seconds : 0} max={entry.period} /><span>{usable ? `${seconds} sn` : entry.unavailable ? "Kurtarma gerekli" : entry.permissions.includes("connect") ? "Yenileniyor…" : "Kod kullanma izni yok"}</span></div>
        <footer><small>{entry.algorithm} · {entry.digits} hane · {entry.period} sn</small><div>
          {entry.permissions.includes("reveal") && <button aria-label={`${entry.label} kurulum sırrını göster`} title="Kurulum sırrını göster" disabled={busy} onClick={() => void reveal(entry)}><QrCode size={16} /></button>}
          {team && onShare && (entry.visibility === "private" || entry.canUpdate) && <button aria-label={`${entry.label} paylaşımını yönet`} title={entry.visibility === "private" ? "Ortak kopya paylaş" : "Paylaşımı yönet"} disabled={busy} onClick={() => onShare(entry)}><Share2 size={16} /></button>}
          {team && onExport && entry.visibility === "shared" && entry.permissions.includes("export") && <button aria-label={`${entry.label} dışa aktar`} title="Dışa aktar" disabled={busy} onClick={() => onExport(entry)}><Download size={16} /></button>}
          {team && onRecover && (entry.visibility === "private" ? entry.unavailable : entry.canUpdate) && <button aria-label={`${entry.label} kurtarma erişimini yönet`} title="Kurtarma erişimi" disabled={busy} onClick={() => onRecover(entry)}><ShieldCheck size={16} /></button>}
          {entry.canRemove && <button aria-label={`${entry.label} hesabını sil`} title="Hesabı sil" disabled={busy} onClick={() => setRemove(entry)}><Trash2 size={16} /></button>}
        </div></footer>
      </article>;
    })}</div>
    {!busy && snapshot && !entries.length && <div className="authenticator-empty"><ShieldCheck size={38} /><h2>{snapshot.entries.length ? "Eşleşen hesap yok" : "Henüz TOTP hesabı yok"}</h2><p>{snapshot.entries.length ? "Başka bir hesap veya sağlayıcı arayın." : "Bir kurulum anahtarı, otpauth adresi veya QR görseliyle hesap ekleyin."}</p></div>}
    {busy && !snapshot && <p role="status">Hesaplar yükleniyor…</p>}
    <p className="authenticator-note authenticator-storage">{team ? "Özel hesaplar hesabınıza, paylaşılanlar takımın erişim izinlerine bağlıdır. Team hesapları için çevrimiçi bağlantı gerekir; kurulum sırları bu cihazda çevrimdışı önbelleğe alınmaz." : "Kurulum sırları bu kasada şifreli saklanır."} QR görselleri yalnız bu cihazda okunur.</p>
    {input && <Modal title="TOTP hesabı ekle" onClose={clearSetup}>
      <form className="authenticator-form" onSubmit={(event) => { event.preventDefault(); void add(); }}>
        <p>Çok adımlı doğrulama (MFA) kurulurken verilen gizli Base32 anahtarını veya otpauth://totp adresini girin. Bu alan VPN veya giriş parolanız için değildir. Google Authenticator taşıma QR kodları ve push bildirimleri desteklenmez.</p>
        <fieldset disabled={busy || qrBusy}>
          <label className="field"><span>Kurulum anahtarı veya otpauth adresi</span><input aria-label="Kurulum anahtarı veya otpauth adresi" type="password" autoComplete="off" spellCheck={false} maxLength={4096} value={input.provisioning} onChange={(event) => { qrVersion.current++; setInput({ ...input, provisioning: event.target.value }); setFormError(""); }} /></label>
          <label className="authenticator-qr"><QrCode size={18} /><span>{qrBusy ? "QR okunuyor…" : "QR görselinden oku"}<small>PNG, JPEG, WebP · en fazla 4 MB</small></span><input aria-label="TOTP QR görseli" type="file" accept="image/png,image/jpeg,image/webp" onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ""; void importQr(file); }} /></label>
          {manual && <><Field label="Hesap adı" value={input.label} onChange={(label) => setInput({ ...input, label })} required /><Field label="Sağlayıcı" value={input.issuer} onChange={(issuer) => setInput({ ...input, issuer })} /><div className="authenticator-options"><Select label="Algoritma" value={input.algorithm} onChange={(algorithm) => setInput({ ...input, algorithm })} options={["SHA1", "SHA256", "SHA512"].map((value) => ({ value, label: value }))} /><Select label="Hane" value={String(input.digits)} onChange={(digits) => setInput({ ...input, digits: Number(digits) })} options={[6, 8].map((value) => ({ value: String(value), label: String(value) }))} /><label className="field"><span>Süre (saniye)</span><input aria-label="Süre (saniye)" type="number" min={1} max={300} required value={Number.isNaN(input.period) ? "" : input.period} onChange={(event) => setInput({ ...input, period: event.target.valueAsNumber })} /></label></div></>}
        </fieldset>
        {formError && <p role="alert" className="error-box">{formError}</p>}
        <div className="authenticator-form-actions"><button type="button" onClick={clearSetup}>Vazgeç</button><button className="primary" disabled={busy || qrBusy || !input.provisioning.trim() || (manual && !input.label.trim())} type="submit">Hesabı kaydet</button></div>
      </form>
    </Modal>}
    {remove && <Modal title="TOTP hesabını sil" onClose={() => setRemove(undefined)}><div className="authenticator-form"><p><strong>{remove.label}</strong> silinsin mi? Sağlayıcının kurtarma kodları veya başka bir doğrulayıcı olmadan hesabınıza erişemeyebilirsiniz.</p><div className="authenticator-form-actions"><button onClick={() => setRemove(undefined)}>Vazgeç</button><button className="danger" disabled={busy} onClick={() => { const generation = scope.current; void request({ kind: "delete", id: remove.id, revision: remove.revision }).then((result) => { if (result && isCurrent(generation)) { setRemove(undefined); setNotice("TOTP hesabı silindi."); } }); }}>Hesabı sil</button></div></div></Modal>}
    {revealed && <Modal title="TOTP kurulum sırrı" onClose={() => setRevealed(undefined)}><div className="authenticator-form"><p>Bu adresi bilen biri doğrulama kodlarınızı üretebilir. Yalnız güvendiğiniz bir doğrulayıcıya taşıyın.</p><textarea aria-label="TOTP kurulum adresi" readOnly rows={5} value={revealed.uri} spellCheck={false} /><button className="primary" onClick={() => setRevealed(undefined)}>Kapat</button></div></Modal>}
  </section>;
}
