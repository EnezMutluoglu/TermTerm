import { useEffect, useRef, useState } from "react";
import { call, errorText } from "./api";
import TeamProxy,{type ProxyEndpoint} from "./TeamProxy";
type Profile = {
  hostId: string | null;
  revision: number;
  method: string;
  credentialId: string | null;
  username: string | null;
  unavailable: boolean;
};
type Identity = {
  id: string;
  label: string;
  kind: string;
  unavailable: boolean;
  visibility?: string;
  permissions?: string[];
};
export default function TeamProfile({
  host,
  root,
  onClose,
}: {
  host: {
    id: string;
    proxy?:ProxyEndpoint|null;
    name: string;
    usernamePolicy: string | null;
    username: string | null;
  } | null;
  root?:string;
  onClose: () => void;
}) {
  const live = useRef(true),
    gate = useRef(false);
  const [busy, setBusy] = useState(true),
    [ready, setReady] = useState(false),
    [error, setError] = useState("");
  const [identities, setIdentities] = useState<Identity[]>([]),
    [revision, setRevision] = useState(0),
    [method, setMethod] = useState(host ? "inherit" : "prompt"),
    [credential, setCredential] = useState(""),
    [username, setUsername] = useState("");
  const locked = host?.usernamePolicy === "shared_required";
  const [unreadable, setUnreadable] = useState(false),
    [replaceUnreadable, setReplaceUnreadable] = useState(false);
  const [history,setHistory]=useState<Profile[]|null>(null),[selectedVersion,setSelectedVersion]=useState<Profile|null>(null);
  async function rpc<T>(action: Record<string, unknown>) {
    const r = await call<T>("team_v2_action", { action });
    if (!live.current) throw Error("Ekran kapatıldı");
    return r;
  }
  useEffect(() => {
    live.current = true;
    void (async () => {
      try {
        const p = await rpc<{ default: Profile | null; hosts: Profile[] }>({
          kind: "profiles",
        });
        const selected = host
          ? p.hosts.find((p) => p.hostId === host.id)
          : p.default;
        if (selected) {
          if (selected.unavailable) setUnreadable(true);
          setRevision(selected.revision);
          setMethod(selected.method);
          setCredential(selected.credentialId ?? "");
          setUsername(selected.username ?? "");
        }
        setIdentities(
          (await rpc<Identity[]>({ kind: "credentials" })).filter(
            (i) =>
              i.kind!=="proxy" && (i.visibility !== "shared" || i.permissions?.includes("connect")),
          ),
        );
        setReady(true);
      } catch (e) {
        if (live.current) setError(errorText(e));
      } finally {
        if (live.current) setBusy(false);
      }
    })();
    return () => {
      live.current = false;
    };
  }, []);
  async function save() {
    if (gate.current || !ready || (unreadable && !replaceUnreadable)) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      await rpc({
        kind: "saveProfile",
        selection: {
          hostId: host?.id ?? null,
          expectedRevision: revision,
          method,
          credentialId: method === "credential" ? credential : null,
          username: locked || method === "inherit" ? null : username || null,
        },
      });
      onClose();
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      gate.current = false;
      if (live.current) setBusy(false);
    }
  }
  async function versions(restore=false) {
    if(gate.current||!ready)return;
    gate.current=true;setBusy(true);setError("");
    try {
      if(restore && selectedVersion) {
        await rpc({kind:"restoreProfile",hostId:host?.id??null,revision:selectedVersion.revision,expectedRevision:revision});
        onClose();
      } else setHistory(await rpc<Profile[]>({kind:"profileHistory",hostId:host?.id??null}));
    } catch(e){if(live.current)setError(errorText(e));}
    finally {gate.current=false;if(live.current)setBusy(false);}
  }
  return (
    <div className="ts-overlay">
      <section
        className="ts-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Kişisel bağlantı profili"
      >
        <h2>{host ? host.name : "Varsayılan bağlantı"} · Kişisel profilim</h2>
        <p>
          Bu seçim yalnız senin hesabına kaydedilir. Başarısız girişte başka bir
          kimliğe otomatik geçilmez. Sunucu seçili kimliği kabul edip ikinci doğrulama adımı isterse MFA penceresi açılır; tek kullanımlık kod saklanmaz.
        </p>
        {error && <p role="alert">{error}</p>}
        {unreadable && (
          <p role="status">
            Eski kişisel kullanıcı adı bu şifreleme kimliğiyle açılamıyor.
            Şirket kurtarma zarfı bulunan profiller liderin kurtarma akışıyla
            açılabilir. Eski, kurtarma zarfı bulunmayan alanlarda kullanıcı adını
            yeniden girebilirsin; önceki şifreli profil bilgisi korunur.
          </p>
        )}
        {unreadable && (
          <label>
            <input
              type="checkbox"
              checked={replaceUnreadable}
              disabled={busy}
              onChange={(e) => setReplaceUnreadable(e.target.checked)}
            />
            Kullanıcı adını yeniden girerek profilimi güncellemek istiyorum
          </label>
        )}
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <label>
            Bağlantı yöntemi
            <select
              aria-label="Bağlantı yöntemi"
              disabled={busy || !ready}
              value={method}
              onChange={(e) => setMethod(e.target.value)}
            >
              {host && (
                <option value="inherit">Kişisel varsayılanımı kullan</option>
              )}
              <option value="prompt">Her bağlantıda parola sor</option>
              <option value="keyboard_interactive">Etkileşimli giriş / MFA</option>
              <option value="credential">Kayıtlı kimlik seç</option>
            </select>
          </label>
          {method === "credential" && (
            <label>
              Kimlik
              <select
                aria-label="Kimlik"
                required
                disabled={busy}
                value={credential}
                onChange={(e) => setCredential(e.target.value)}
              >
                <option value="">Kimlik seç</option>
                {identities.map((i) => (
                  <option key={i.id} value={i.id} disabled={i.unavailable}>
                    {i.label} ·{" "}
                    {i.kind === "ssh_key" ? "SSH anahtarı" : i.kind === "agent" ? "SSH agent" : i.kind === "password" ? "Parola" : "Bu cihazda kullanılamayan kimlik"}
                    {i.unavailable ? " · Kurtarma gerekli" : ""}
                  </option>
                ))}
              </select>
            </label>
          )}
          <label>
            Kullanıcı adı
            <input
              aria-label="Profil kullanıcı adı"
              autoComplete="off"
              disabled={busy || !ready || locked || method === "inherit"}
              value={locked ? (host?.username ?? "") : username}
              placeholder={
                method === "inherit"
                  ? "Kişisel varsayılandan alınır"
                  : "Boşsa bağlantıda sorulur"
              }
              onChange={(e) => setUsername(e.target.value)}
            />
          </label>
          {locked && (
            <small>
              Takım ortak kullanıcı adını zorunlu tutuyor; kişisel kimlik
              seçimini değiştirebilirsin.
            </small>
          )}
          <div className="ts-actions">
            <button
              className="ts-primary"
              disabled={
                busy ||
                !ready ||
                (unreadable && !replaceUnreadable) ||
                (method === "credential" && !credential)
              }
            >
              Profili kaydet
            </button>
            <button type="button" onClick={onClose}>
              Kapat
            </button>
          </div>
        </form>
        {host?.proxy?.authenticate && <TeamProxy key={host.id} root={root??host.id} hostId={host.id} endpoint={host.proxy}/> }
        <button disabled={busy||!ready} onClick={()=>void versions()}>Önceki kullanıcı adı sürümleri</button>
        {history && <section aria-label="Kişisel profil geçmişi">
          <p>Önizleme ortak hostu değiştirmez. Uygulama, seçilen kişisel profili yeni sürüm olarak kaydeder; güncel yetkiler yeniden kontrol edilir.</p>
          {!history.length && <p>Arşivlenmiş kullanıcı adı yok.</p>}
          {history.map(p=><button key={p.revision} disabled={busy||p.unavailable} onClick={()=>setSelectedVersion(p)}>
            Sürüm {p.revision}{p.unavailable?" · Bu kimlikle açılamıyor":" · Önizle"}
          </button>)}
          {selectedVersion && <div role="region" aria-label="Profil geri yükleme önizlemesi">
            <p>Kullanıcı adı: <strong>{selectedVersion.username}</strong> · Yöntem: {selectedVersion.method === "credential" ? "Kayıtlı kimlik" : selectedVersion.method === "prompt" ? "Parola sor" : "Etkileşimli giriş"}</p>
            {selectedVersion.credentialId && <p>Kimlik: {identities.find(i=>i.id===selectedVersion.credentialId)?.label??"Eksik veya izinsiz kimlik; geri yükleme reddedilebilir"}</p>}
            <button disabled={busy||selectedVersion.unavailable||!!locked} onClick={()=>void versions(true)}>Yeni sürüm olarak geri yükle</button>
            {locked && <p>Ortak kullanıcı adı zorunlu; eski kişisel kullanıcı adı uygulanamaz.</p>}
          </div>}
        </section>}
      </section>
    </div>
  );
}
