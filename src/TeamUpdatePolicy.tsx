import { useEffect, useState } from "react";
import { call, errorText } from "./api";
type Policy = {
  revision: number;
  minimumClientVersion: string;
  selectedVersion: string | null;
  versions: string[];
};
export default function TeamUpdatePolicy() {
  const [policy, setPolicy] = useState<Policy>(),
    [version, setVersion] = useState(""),
    [preview, setPreview] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [done, setDone] = useState(false);
  useEffect(() => {
    let live = true;
    void call<Policy>("team_v2_action", { action: { kind: "clientPolicy" } })
      .then((p) => {
        if (live) {
          setPolicy(p);
          setVersion(p.selectedVersion ?? p.versions.at(-1) ?? "");
        }
      })
      .catch((e) => {
        if (live) setError(errorText(e));
      });
    return () => {
      live = false;
    };
  }, []);
  async function apply() {
    if (busy || !policy) return;
    setBusy(true);
    setError("");
    try {
      const result = await call<{ revision: number; selectedVersion: string }>(
        "team_v2_action",
        {
          action: {
            kind: "setClientPolicy",
            version,
            expectedRevision: policy.revision,
          },
        },
      );
      setPolicy({ ...policy, ...result });
      setDone(true);
      setPreview(false);
    } catch (e) {
      setError(errorText(e));
      setPreview(false);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="settings-card" aria-label="Team sürüm politikası">
      <h2>Team sürüm politikası</h2>
      <p>
        Kişisel kullanımda güncelleme isteğe bağlıdır. Buradaki seçim yalnız bu
        takımı etkiler; tüm üyelere ve takım sahibine uygulanır.
      </p>
      <p>
        Bağlı uygulamalar en geç 30 saniyelik denetimde veya sonraki işlemde
        uyarılır. Kapalı uygulamalar sonraki girişte denetlenir. Tamamen
        çevrimdışı cihaz kararı ancak servise yeniden ulaşınca öğrenebilir.
      </p>
      {policy && (
        <>
          <p>
            Teknik alt sınır ve etkin politika: {policy.minimumClientVersion}.
            Yetkilinin seçtiği: {policy.selectedVersion ?? "Henüz seçilmedi"}.
          </p>
          <label>
            Zorunlu istemci sürümü
            <select
              disabled={busy}
              value={version}
              onChange={(e) => {
                setVersion(e.target.value);
                setPreview(false);
                setDone(false);
              }}
            >
              {policy.versions.map((v) => (
                <option key={v} value={v}>
                  {v}
                  {v.includes("-") ? " · geliştirme paketi" : ""}
                </option>
              ))}
            </select>
          </label>
          <p>
            Liste, bu servis paketiyle uyumlu olarak tanımlanmış istemci
            sürümleridir. Seçmeden önce Windows/Linux paketlerinin üyelerin
            erişimine hazır olduğundan emin olun. Bu işlem paket yayımlamaz veya
            veritabanını yükseltmez.
          </p>
          <button
            disabled={busy || !version}
            onClick={() => {
              setPreview(true);
              setDone(false);
            }}
          >
            Etkiyi önizle
          </button>
          {preview && (
            <div role="region" aria-label="Sürüm zorunluluğu önizlemesi">
              <p>
                <strong>{version}</strong> altındaki istemciler Team kullanımına
                devam edemeyecek. Açık Team bağlantıları kapatılacak;
                gönderilmemiş şifreli değişiklikler saklanacak. Güncelleme
                sonrasında giriş ve eşitleme yeniden yapılır.
              </p>
              <button disabled={busy} onClick={() => setPreview(false)}>
                Vazgeç
              </button>
              <button
                className="primary"
                disabled={busy}
                onClick={() => void apply()}
              >
                {busy ? "Kaydediliyor…" : "Onayla ve tüm takıma bildir"}
              </button>
            </div>
          )}
        </>
      )}
      {done && (
        <p role="status">
          Sürüm politikası kaydedildi. Üyeler bir sonraki denetimde güncelleme
          uyarısını alacak.
        </p>
      )}
      {error && (
        <p role="alert">
          {error}{" "}
          {error.includes("revision_conflict")
            ? "Başka bir yönetici değiştirdi. Ekranı yeniden açıp güncel politikayı inceleyin."
            : ""}
        </p>
      )}
    </section>
  );
}
