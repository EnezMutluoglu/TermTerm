import { useEffect, useRef, useState } from "react";
import { call, errorText } from "./api";
import TeamPins from "./TeamPins";
type Entry = {
  id: string;
  actor: string | null;
  action: string;
  outcome: string;
  serverAt: string;
};
const labels: Record<string, string> = {
  "account.activate": "Hesap etkinleştirildi",
  "account.create": "Üye oluşturuldu",
  "account.disable": "Hesap devre dışı bırakıldı",
  "account.login": "Oturum açıldı",
  "account.reset": "Geçici parola yenilendi",
  "acl.apply": "Kaynak izinleri değiştirildi",
  "delegation.set": "Yetki devri değiştirildi",
  "credential.private_save": "Kişisel kimlik kaydedildi",
  "credential.connect": "Bağlantı için kimlik kullanıldı",
  "credential.reveal": "Kimlik sırrı görüntülendi",
  "credential.export": "Kimlik dışa aktarıldı",
  "resource.save": "Kaynak kaydedildi",
  "resource.delete": "Kaynak silindi",
  "resource.restore": "Geçmiş sürüm geri yüklendi",
  "vault.create": "Ortak kasa oluşturuldu",
  "conflict.server": "Çakışmada sunucu sürümü korundu",
  "conflict.candidate": "Çakışmada aday sürüm seçildi",
  "conflict.merge": "Çakışma birleştirilerek çözüldü",
  "recovery.authorize": "Kurtarma yetkilendirildi",
  "recovery.confirm": "Kurtarma alıcı tarafından doğrulandı",
  "recovery.enroll": "Kurtarma anahtarı kuruldu",
  "recovery.read_envelopes": "Kurtarma anahtar zarflarına erişildi",
  "recovery.rewrap": "Kurtarma anahtarları yeniden sarıldı",
  "recovery.cancel": "Kurtarma iptal edildi",
  "credential.share": "Ortak kimlik oluşturuldu",
  "credential.shared_update": "Ortak kimlik ve alıcıları güncellendi",
  "credential.unshare": "Kimlik paylaşımı kaldırıldı",
  "maintenance.prune": "Saklama süresi dolan kayıtlar temizlendi",
  "history.pin_release": "Geçmiş sürüm koruması lider tarafından kaldırıldı",
};
export default function TeamAudit() {
  const [pinsOpen, setPinsOpen] = useState(false);
  const [storage, setStorage] = useState<{
    encryptedPayloadBytes: string;
    records: number;
    versions: number;
    auditDays: number;
    lastMaintenance: string | null;
    nextMaintenance: string | null;
    databasePhysicalBytes?: string;
    teamSchemaTableBytes?: string;
    teamSchemaIndexBytes?: string;
  } | null>(null);
  const [entries, setEntries] = useState<Entry[]>([]),
    [cursor, setCursor] = useState<string | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const live = useRef(true),
    gate = useRef(false);
  async function load(before: string | null) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      const r = await call<{ events: Entry[]; nextBefore: string | null }>(
        "team_v2_action",
        { action: { kind: "audit", before } },
      );
      if (live.current) {
        setEntries((old) => (before ? [...old, ...r.events] : r.events));
        setCursor(r.nextBefore);
      }
      if (!before) {
        const value = await call<typeof storage>("team_v2_action", {
          action: { kind: "storage" },
        });
        if (live.current) setStorage(value);
      }
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      gate.current = false;
      if (live.current) setBusy(false);
    }
  }
  useEffect(() => {
    live.current = true;
    void load(null);
    return () => {
      live.current = false;
    };
  }, []);
  async function prune() {
    let succeeded = false;
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      const r = await call<{
        auditDeleted: number;
        accessPlansDeleted: number;
        sharePlansDeleted: number;
        updatePlansDeleted?: number;
        pinPlansDeleted?: number;
        auditDays: number;
      }>("team_v2_action", { action: { kind: "maintenance" } });
      succeeded = true;
      if (live.current)
        setNotice(
          `${r.auditDays} günlük saklama kuralı uygulandı: ${r.auditDeleted} işlem kaydı, ${r.accessPlansDeleted + r.sharePlansDeleted + (r.updatePlansDeleted ?? 0) + (r.pinPlansDeleted ?? 0)} süresi dolmuş önizleme temizlendi. Her çalıştırma en fazla 1.000 işlem kaydını temizler.`,
        );
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      gate.current = false;
      if (live.current) {
        setBusy(false);
        if (succeeded) void load(null);
      }
    }
  }
  return (
    <section className="ts-audit" aria-label="İşlem geçmişi">
      {pinsOpen && <TeamPins onClose={() => setPinsOpen(false)} />}
      {storage && (
        <p>
          {storage.records} kayıt · {storage.versions} kayıt sürümü · Şifreli
          sürüm ve anahtar zarfları:{" "}
          {(Number(storage.encryptedPayloadBytes) / 1048576).toLocaleString(
            "tr-TR",
            { maximumFractionDigits: 2 },
          )}{" "}
          MiB. Bu değer indeksler ve diğer tablolar dahil fiziksel veritabanı
          boyutu değildir.
        </p>
      )}
      {storage?.databasePhysicalBytes && (
        <p>
          Veritabanının tamamı:{" "}
          {(Number(storage.databasePhysicalBytes) / 1048576).toLocaleString(
            "tr-TR",
            { maximumFractionDigits: 2 },
          )}{" "}
          MiB. Tüm takımların Team tabloları:{" "}
          {(Number(storage.teamSchemaTableBytes) / 1048576).toLocaleString(
            "tr-TR",
            { maximumFractionDigits: 2 },
          )}{" "}
          MiB; indeksleri:{" "}
          {(Number(storage.teamSchemaIndexBytes) / 1048576).toLocaleString(
            "tr-TR",
            { maximumFractionDigits: 2 },
          )}{" "}
          MiB. Bu fiziksel değerler yalnız bu takıma ait değildir.
        </p>
      )}
      {storage && (
        <p>
          Otomatik bakım:{" "}
          {storage.lastMaintenance
            ? new Date(storage.lastMaintenance).toLocaleString("tr-TR")
            : "İlk çalışma bekleniyor"}
          . Saklama: {storage.auditDays} gün. Servis çalışırken süresi gelen
          takımlar sınırlı gruplar halinde temizlenir.
        </p>
      )}
      <p>
        Sunucunun saatine göre kayıt ve yetki işlemleri. Parolalar, özel
        anahtarlar ve terminal çıktısı burada tutulmaz.
      </p>
      <div className="ts-actions">
        <button disabled={busy} onClick={() => setPinsOpen(true)}>
          Korunan sürümleri yönet
        </button>
        <button disabled={busy} onClick={() => void load(null)}>
          Geçmişi yenile
        </button>
        <button disabled={busy} onClick={() => void prune()}>
          Saklama süresi dolanları temizle
        </button>
      </div>
      <p>
        Varsayılan saklama 90 gündür. Bekleyen kararlar, sürüm sabitlemeleri ve
        kurtarma kayıtları bu temizlikten etkilenmez.
      </p>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <table>
        <thead>
          <tr>
            <th>Zaman</th>
            <th>İşlemi yapan</th>
            <th>İşlem</th>
            <th>Sonuç</th>
          </tr>
        </thead>
        <tbody>
          {entries.map((e) => (
            <tr key={e.id}>
              <td>{new Date(e.serverAt).toLocaleString("tr-TR")}</td>
              <td>{e.actor ?? "Sistem"}</td>
              <td>{labels[e.action] ?? e.action}</td>
              <td>
                {e.outcome === "succeeded"
                  ? "Başarılı"
                  : e.outcome === "denied"
                    ? "Reddedildi"
                    : e.outcome === "failed"
                      ? "Başarısız"
                      : "Yetkilendirildi"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {!busy && !entries.length && <p>Henüz işlem kaydı yok.</p>}
      {cursor && (
        <div className="ts-actions">
          <button disabled={busy} onClick={() => void load(cursor)}>
            Önceki kayıtlar
          </button>
        </div>
      )}
    </section>
  );
}
