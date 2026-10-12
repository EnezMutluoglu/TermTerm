import { useRef, useState } from "react";
import {
  ArrowRight,
  Check,
  ChevronRight,
  FileUp,
  Folder,
  Globe2,
  Info,
  LockKeyhole,
  Plus,
  Search,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { Field, Modal, Select } from "./components";
import "./vpn.css";

const families = [
  {
    value: "openVpn",
    label: "OpenVPN",
    detail: "OpenVPN GUI / Connect · .ovpn dosyası",
  },
  {
    value: "fortinet",
    label: "Fortinet",
    detail: "FortiGate SSL VPN · adres ve realm",
  },
  {
    value: "globalProtect",
    label: "GlobalProtect",
    detail: "Portal / gateway · bağlantı bilgileri",
  },
] as const;
type Family = (typeof families)[number]["value"];
type Draft = {
  id: string;
  label: string;
  family: Family;
  server: string;
  username: string;
  realm: string;
  folder: string;
  file?: string;
};
const labelFor = (family: Family) =>
  families.find((f) => f.value === family)!.label;
const samples: Draft[] = [
  {
    id: "office",
    label: "İstanbul · Ofis",
    family: "fortinet",
    server: "https://vpn.firma.example:443",
    username: "",
    realm: "employees",
    folder: "Firma",
  },
  {
    id: "customer",
    label: "Müşteri · Ankara",
    family: "globalProtect",
    server: "https://portal.musteri.example",
    username: "",
    realm: "",
    folder: "Müşteriler",
  },
  {
    id: "lab",
    label: "Geliştirme · Lab",
    family: "openVpn",
    server: "",
    username: "",
    realm: "",
    folder: "Lab",
    file: "ornek-lab.ovpn",
  },
];
const blank = (): Draft => ({
  id: crypto.randomUUID(),
  label: "",
  family: "openVpn",
  server: "",
  username: "",
  realm: "",
  folder: "Firma",
});

// Explicitly transient UX preview: no credential store or connection is simulated.
export default function VpnPreview() {
  const [profiles, setProfiles] = useState<Draft[]>(samples);
  const [selected, setSelected] = useState("office");
  const [folder, setFolder] = useState("");
  const [search, setSearch] = useState("");
  const [draft, setDraft] = useState<Draft>();
  const [planned, setPlanned] = useState<string[]>([
    "office",
    "customer",
    "lab",
  ]);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [checking, setChecking] = useState<Draft[]>();
  const fileInput = useRef<HTMLInputElement>(null);
  const current = profiles.find((p) => p.id === selected);
  const visible = profiles.filter(
    (p) =>
      (!folder || p.folder === folder) &&
      `${p.label} ${labelFor(p.family)} ${p.server}`
        .toLocaleLowerCase("tr")
        .includes(search.toLocaleLowerCase("tr")),
  );
  function edit(profile: Draft) {
    setDraft({ ...profile });
    setError("");
    setNotice("");
  }
  function save() {
    if (!draft) return;
    if (!draft.label.trim()) return setError("Profil adı gerekli.");
    if (draft.family === "openVpn" && !draft.file)
      return setError("Bir .ovpn dosyası seçin.");
    if (draft.family !== "openVpn") {
      try {
        const url = new URL(draft.server);
        if (
          url.protocol !== "https:" ||
          !url.hostname ||
          url.username ||
          url.password ||
          url.hash
        )
          throw new Error();
      } catch {
        return setError(
          "Kullanıcı adı/parola içermeyen bir HTTPS adresi girin.",
        );
      }
    }
    setProfiles((all) => [
      ...all.filter((p) => p.id !== draft.id),
      { ...draft, label: draft.label.trim() },
    ]);
    setSelected(draft.id);
    setFolder("");
    setSearch("");
    setDraft(undefined);
    setNotice(
      "Profil taslağı eklendi. Yalnız bu önizlemenin belleğinde tutulur; kasaya kaydedilmedi.",
    );
  }
  function chooseFile(file?: File) {
    if (!file || !draft) return;
    if (file.size > 1024 * 1024)
      return setError("VPN dosyası 1 MiB sınırını aşıyor.");
    if (!/\.(ovpn|conf|xml)$/i.test(file.name))
      return setError(".ovpn, .conf veya .xml dosyası seçin.");
    setDraft({
      ...draft,
      file: file.name,
      family: /\.ovpn$/i.test(file.name) ? "openVpn" : draft.family,
    });
    setError("");
  }
  return (
    <section className="vpn-workspace" aria-label="VPN çalışma alanı">
      <header className="vpn-heading">
        <div className="vpn-heading-title">
          <span className="vpn-emblem">
            <ShieldCheck size={25} />
          </span>
          <div>
            <h1>VPN</h1>
            <p>Bağlantıların, aynı çalışma alanında.</p>
          </div>
          <span className="vpn-pill">0.4.3-dev.1</span>
        </div>
        <div className="vpn-actions">
          <button className="secondary" onClick={() => edit(blank())}>
            <FileUp size={16} /> Dosyadan ekle
          </button>
          <button
            className="primary"
            onClick={() => edit({ ...blank(), family: "fortinet" })}
          >
            <Plus size={16} /> Yeni VPN
          </button>
        </div>
      </header>
      <div className="vpn-preview-note">
        <Info size={17} />
        <span>
          <strong>Etkileşimli arayüz önizlemesi.</strong> Gerçek VPN bağlantısı
          açılmaz. Taslaklar geçicidir; dosya içerikleri ve parolalar okunmaz
          veya kaydedilmez.
        </span>
      </div>
      {notice && (
        <div className="vpn-notice" role="status">
          {notice}
        </div>
      )}
      <div className="vpn-overview">
        <div>
          <span>Aktif bağlantı</span>
          <strong>
            0 <small>/ bağımsız tüneller</small>
          </strong>
        </div>
        <div>
          <span>Profil taslağı</span>
          <strong>
            {profiles.length}
            <small> OpenVPN · Fortinet · GlobalProtect</small>
          </strong>
        </div>
        <div>
          <span>Birlikte bağlantı</span>
          <strong>
            {planned.length}
            <small> profil planlandı · henüz bağlanmadı</small>
          </strong>
        </div>
      </div>
      <div className="vpn-body">
        <aside className="vpn-folders">
          <span className="vpn-eyebrow">VPN KLASÖRLERİ</span>
          {["", "Firma", "Müşteriler", "Lab"].map((name) => (
            <button
              key={name}
              className={folder === name ? "active" : ""}
              onClick={() => setFolder(name)}
            >
              <Folder size={16} />
              <span>{name || "Tüm profiller"}</span>
              <small>
                {profiles.filter((p) => !name || p.folder === name).length}
              </small>
            </button>
          ))}
          <div className="vpn-folder-tip">
            <LockKeyhole size={18} />
            <strong>Kişisel bağlantı</strong>
            <p>
              Team paylaşımı ve kişi bazlı kimlikler sonraki entegrasyon adımı.
            </p>
          </div>
        </aside>
        <div className="vpn-profiles">
          <label className="vpn-search">
            <Search size={17} />
            <input
              aria-label="VPN ara"
              placeholder="İsim, VPN türü veya sunucu ara…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </label>
          <div className="vpn-list-caption">
            <span>{visible.length} profil</span>
            <span>Bağlantılar birbirinden bağımsız olacak</span>
          </div>
          <div className="vpn-list">
            {visible.map((profile) => (
              <article
                className={
                  "vpn-card " + (selected === profile.id ? "selected" : "")
                }
                key={profile.id}
              >
                <button
                  className="vpn-card-summary"
                  onClick={() => setSelected(profile.id)}
                >
                  <span className="vpn-provider-icon">
                    <Globe2 size={21} />
                  </span>
                  <span className="vpn-card-text">
                    <strong>{profile.label}</strong>
                    <span>
                      {labelFor(profile.family)} <i>·</i> {profile.folder}
                    </span>
                    <small>{profile.server || profile.file}</small>
                  </span>
                  <ChevronRight size={17} />
                </button>
                <div className="vpn-card-bottom">
                  <span className="vpn-disconnected">
                    <i /> Bağlı değil
                  </span>
                  <button
                    aria-pressed={planned.includes(profile.id)}
                    onClick={() =>
                      setPlanned((ids) =>
                        ids.includes(profile.id)
                          ? ids.filter((id) => id !== profile.id)
                          : [...ids, profile.id],
                      )
                    }
                  >
                    {planned.includes(profile.id) ? (
                      <Check size={14} />
                    ) : (
                      <Plus size={14} />
                    )}
                    {planned.includes(profile.id) ? "Planda" : "Plana ekle"}
                  </button>
                  <button onClick={() => setChecking([profile])}>
                    Bağlantıyı kontrol et <ArrowRight size={14} />
                  </button>
                </div>
              </article>
            ))}
            {!visible.length && (
              <p className="vpn-empty">Bu aramayla eşleşen profil yok.</p>
            )}
          </div>
          <section className="vpn-parallel">
            <div>
              <strong>Aynı anda 2–3 VPN</strong>
              <p>
                Ayrı bağlantılar için plan oluştur. Rota ve DNS kontrolü
                tamamlanmadan hiçbir tünel açılmayacak.
              </p>
            </div>
            <button
              className="secondary"
              disabled={!planned.length}
              onClick={() =>
                setChecking(profiles.filter((p) => planned.includes(p.id)))
              }
            >
              {planned.length} profili kontrol et <ArrowRight size={15} />
            </button>
          </section>
        </div>
        <aside className="vpn-details">
          {current && (
            <>
              <span className="vpn-eyebrow">PROFİL BİLGİLERİ</span>
              <h2>{current.label}</h2>
              <span className="vpn-pill">{labelFor(current.family)}</span>
              <dl>
                <dt>Sunucu / dosya</dt>
                <dd>{current.server || current.file}</dd>
                <dt>Kullanıcı</dt>
                <dd>{current.username || "Bağlantıda sorulacak"}</dd>
                <dt>Realm / grup</dt>
                <dd>{current.realm || "Sunucunun varsayılanı"}</dd>
                <dt>OTP</dt>
                <dd>Sunucu isterse sorulacak</dd>
                <dt>Masaüstü motoru</dt>
                <dd className="vpn-warning">Entegrasyon bekleniyor</dd>
              </dl>
              <div className="vpn-details-actions">
                <button className="secondary" onClick={() => edit(current)}>
                  Profili düzenle
                </button>
                <button
                  className="icon-btn"
                  title="Taslağı sil"
                  aria-label="Taslağı sil"
                  onClick={() => {
                    setProfiles((all) =>
                      all.filter((p) => p.id !== current.id),
                    );
                    setPlanned((ids) => ids.filter((id) => id !== current.id));
                    setSelected("");
                  }}
                >
                  <Trash2 size={16} />
                </button>
              </div>
              <div className="vpn-folder-tip">
                <ShieldCheck size={18} />
                <p>
                  OpenVPN ve ocserv testleri Linux laboratuvarında geçti. Bu
                  profilin gerçek sunucusu doğrulanmadı.
                </p>
              </div>
            </>
          )}
        </aside>
      </div>
      {draft && (
        <Modal
          title="VPN profili ekle / düzenle"
          onClose={() => setDraft(undefined)}
          wide
        >
          <form
            className="vpn-form"
            onSubmit={(e) => {
              e.preventDefault();
              save();
            }}
          >
            <p className="vpn-form-intro">
              Önce VPN türünü seç. Dosya veya bağlantı bilgileriyle profil
              taslağını oluştur.
            </p>
            <div
              className="vpn-family-picker"
              role="group"
              aria-label="VPN türü"
            >
              {families.map((family) => (
                <button
                  type="button"
                  key={family.value}
                  aria-pressed={draft.family === family.value}
                  className={draft.family === family.value ? "selected" : ""}
                  onClick={() => {
                    setDraft({
                      ...draft,
                      family: family.value,
                      file: undefined,
                    });
                    setError("");
                  }}
                >
                  <ShieldCheck size={19} />
                  <strong>{family.label}</strong>
                  <small>{family.detail}</small>
                </button>
              ))}
            </div>
            <div className="vpn-form-grid">
              <Field
                label="Profil adı"
                value={draft.label}
                onChange={(label) => setDraft({ ...draft, label })}
                placeholder="Örn. İstanbul · Ofis"
                required
              />
              <Select
                label="Klasör"
                value={draft.folder}
                onChange={(folder) => setDraft({ ...draft, folder })}
                options={["Firma", "Müşteriler", "Lab"].map((label) => ({
                  label,
                  value: label,
                }))}
              />
            </div>
            <div
              className="vpn-drop"
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault();
                chooseFile(e.dataTransfer.files[0]);
              }}
            >
              <FileUp size={24} />
              <strong>
                {draft.file ||
                  (draft.family === "openVpn"
                    ? "OpenVPN dosyanı buraya bırak"
                    : "Varsa yapılandırma dosyasını ekle")}
              </strong>
              <span>.ovpn, .conf, .xml · en fazla 1 MiB</span>
              <button
                className="secondary"
                type="button"
                onClick={() => fileInput.current?.click()}
              >
                Dosya seç
              </button>
              <input
                ref={fileInput}
                aria-label="VPN dosyası"
                type="file"
                accept=".ovpn,.conf,.xml"
                hidden
                onChange={(e) => {
                  chooseFile(e.target.files?.[0]);
                  e.target.value = "";
                }}
              />
            </div>
            {draft.file && (
              <p className="vpn-file-note">
                Dosya adı seçildi; içerik doğrulaması ve şifreli import henüz
                bağlı değil.
              </p>
            )}
            {draft.family !== "openVpn" && (
              <Field
                label={
                  draft.family === "globalProtect"
                    ? "Portal / gateway adresi"
                    : "VPN sunucusu"
                }
                value={draft.server}
                onChange={(server) => {
                  setDraft({ ...draft, server });
                  setError("");
                }}
                placeholder="https://vpn.firma.example:443"
                required
              />
            )}
            <div className="vpn-form-grid">
              <Field
                label="Kullanıcı adı"
                value={draft.username}
                onChange={(username) => setDraft({ ...draft, username })}
                placeholder="Bağlantıda sorulabilir"
              />
              <Field
                label="Realm / grup (isteğe bağlı)"
                value={draft.realm}
                onChange={(realm) => setDraft({ ...draft, realm })}
              />
            </div>
            <div className="vpn-auth-note">
              <LockKeyhole size={19} />
              <div>
                <strong>Parola ve OTP</strong>
                <p>
                  Parola hatırlama, şifreli kasa entegrasyonu tamamlanınca
                  açılacak. OTP yalnız sunucu istediğinde sorulacak ve
                  kaydedilmeyecek. Bu önizlemeye gerçek parola girmene gerek
                  yok.
                </p>
              </div>
            </div>
            {error && (
              <p role="alert" className="vpn-error">
                {error}
              </p>
            )}
            <footer>
              <span>Taslak · yalnız bu oturumda</span>
              <button
                className="secondary"
                type="button"
                onClick={() => setDraft(undefined)}
              >
                Vazgeç
              </button>
              <button className="primary" type="submit">
                Taslağı ekle
              </button>
            </footer>
          </form>
        </Modal>
      )}
      {checking && (
        <Modal
          title="Bağlantı hazırlığı"
          onClose={() => setChecking(undefined)}
          wide
        >
          <div className="vpn-preflight">
            <p>
              {checking.length} profil bağımsız bağlantı olarak kontrol
              ediliyor. Gerçek tünel başlatılmadı.
            </p>
            {checking.map((p) => (
              <div className="vpn-preflight-row" key={p.id}>
                <Globe2 size={21} />
                <div>
                  <strong>{p.label}</strong>
                  <small>{labelFor(p.family)}</small>
                </div>
                <span>Motor entegrasyonu bekleniyor</span>
              </div>
            ))}
            <div className="vpn-auth-note">
              <Info size={21} />
              <div>
                <strong>Bağlantı başlamadı</strong>
                <p>
                  Masaüstü VPN servisi hazır değil. Bu nedenle kimlik doğrulama,
                  rota/DNS kontrolü ve tünel açma çalıştırılmadı. Mevcut ağ
                  ayarların değişmedi.
                </p>
              </div>
            </div>
            <h3>Birlikte bağlantı kuralları</h3>
            <ul>
              <li>
                Farklı hedef ağlara giden rotalar birlikte kullanılabilir.
              </li>
              <li>
                Aynı IP aralığı veya çakışan varsayılan rota yeni bağlantıyı
                engelleyecek.
              </li>
              <li>
                DNS politikaları birlikte uygulanamıyorsa mevcut bağlantı
                korunacak.
              </li>
              <li>Bir VPN kapanınca diğer VPN'ler otomatik kapatılmayacak.</li>
            </ul>
            <footer>
              <button
                className="primary"
                onClick={() => setChecking(undefined)}
              >
                Anladım
              </button>
            </footer>
          </div>
        </Modal>
      )}
    </section>
  );
}
