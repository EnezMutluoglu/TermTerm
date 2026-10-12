import { useEffect, useRef, useState } from "react";
import { call, errorText } from "./api";

type Version = {
  revision: number;
  createdAt: string;
  actorId: string;
  content: Record<string, unknown> | null;
  error: string | null;
};
const names: Record<string, string> = {
  name: "Ad",
  address: "Adres",
  port: "Port",
  protocol: "Protokol",
  username: "Ortak kullanıcı adı",
  usernamePolicy: "Kullanıcı adı politikası",
  parentId: "Üst klasör",
  deleted: "Silindi",
  kind: "Kayıt türü",
  document: "Komutlar ve çalışma alanı",
  proxy: "Proxy",
};
export default function TeamHistory({
  id,
  name,
  onClose,
  onChanged,
}: {
  id: string;
  name: string;
  onClose: () => void;
  onChanged: () => void;
}) {
  const live = useRef(true),
    gate = useRef(false);
  const [busy, setBusy] = useState(true),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [versions, setVersions] = useState<Version[]>([]),
    [selected, setSelected] = useState<number | null>(null),
    [confirm, setConfirm] = useState(false);
  const [current, setCurrent] = useState<{
    revision: number;
    content: Record<string, unknown>;
    permissions: string[];
  } | null>(null);
  const [restorePlan, setRestorePlan] = useState<{
    planId: string;
    hops: { id: string; name: string }[];
    recipientCount?: number;
    routeUnchanged?: boolean;
  } | null>(null);
  const [pins, setPins] = useState<{ revision: number; purpose: string }[]>([]);
  async function rpc<T>(action: Record<string, unknown>) {
    const result = await call<T>("team_v2_action", { action });
    if (!live.current) throw Error("Ekran kapatıldı");
    return result;
  }
  async function load() {
    setConfirm(false);
    setRestorePlan(null);
    const present = await rpc<NonNullable<typeof current>>({
      kind: "resource",
      id,
    });
    const history = await rpc<{ versions: Version[]; pins: typeof pins }>({
      kind: "history",
      id,
    });
    setCurrent(present);
    setVersions(history.versions);
    setPins(history.pins);
  }
  async function run(task: () => Promise<void>) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      await task();
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      gate.current = false;
      if (live.current) setBusy(false);
    }
  }
  useEffect(() => {
    live.current = true;
    void run(load);
    return () => {
      live.current = false;
    };
  }, [id]);
  const chosen = versions.find((v) => v.revision === selected);
  const canRestore = !!(
    chosen?.content &&
    current?.permissions.includes("edit") &&
    chosen.revision !== current.revision &&
    !chosen.content.deleted &&
    chosen.content.parentId === current.content.parentId
  );
  return (
    <div className="ts-overlay">
      <section
        className="ts-dialog ts-history"
        role="dialog"
        aria-modal="true"
        aria-label="Kayıt geçmişi"
        aria-busy={busy}
      >
        <h2>Kayıt geçmişi — {name}</h2>
        <p>
          Güncel kayıt ve önceki 10 sürüm. Korunan dönüş noktaları bu sayıya
          dahil değildir. SSH sunucusundaki dosyalar ve kişisel kimlikler bu
          işlemle değişmez.
        </p>
        {error && <p role="alert">{error}</p>}
        {notice && <p role="status">{notice}</p>}
        {pins
          .filter((p) => p.purpose === "return_point")
          .map((p) => (
            <div className="ts-history-return" key={p.revision}>
              <span>Korunan dönüş noktası: sürüm {p.revision}</span>
              <button
                disabled={busy}
                onClick={() => {
                  setSelected(p.revision);
                  setConfirm(false);
                  setRestorePlan(null);
                }}
              >
                Dönüş noktasını karşılaştır
              </button>
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await rpc({ kind: "releaseReturnPoint", id });
                    await load();
                    setNotice("Dönüş noktası koruması kaldırıldı.");
                  })
                }
              >
                Korumayı kaldır
              </button>
            </div>
          ))}
        <label>
          Sürüm
          <select
            aria-label="Geçmiş sürüm"
            disabled={busy}
            value={selected ?? ""}
            onChange={(e) => {
              setSelected(Number(e.target.value) || null);
              setConfirm(false);
              setRestorePlan(null);
              setNotice("");
            }}
          >
            <option value="">Sürüm seç</option>
            {versions.map((v) => (
              <option key={v.revision} value={v.revision}>
                Sürüm {v.revision} —{" "}
                {new Date(v.createdAt).toLocaleString("tr-TR")}
                {v.revision === current?.revision ? " (güncel)" : ""}
                {v.error ? " (açılamıyor)" : ""}
              </option>
            ))}
          </select>
        </label>
        {chosen?.error && (
          <p role="alert">Bu sürüm açılamadı: {chosen.error}</p>
        )}
        {chosen?.content && current && (
          <div className="ts-history-table">
            <table>
              <thead>
                <tr>
                  <th>Alan</th>
                  <th>Güncel ({current.revision})</th>
                  <th>Seçilen ({chosen.revision})</th>
                </tr>
              </thead>
              <tbody>
                {Object.entries(names).map(([field, label]) => (
                  <tr
                    key={field}
                    data-changed={
                      JSON.stringify(current.content[field]) !==
                      JSON.stringify(chosen.content![field])
                    }
                  >
                    <th>{label}</th>
                    <td>
                      {typeof current.content[field] === "object" &&
                      current.content[field]
                        ? JSON.stringify(current.content[field])
                        : String(current.content[field] ?? "—")}
                    </td>
                    <td>
                      {typeof chosen.content![field] === "object" &&
                      chosen.content![field]
                        ? JSON.stringify(chosen.content![field])
                        : String(chosen.content![field] ?? "—")}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {chosen && <small>İşlemi yapan hesap: {chosen.actorId}</small>}
        {confirm && (
          <p>
            Seçilen içerik yeni bir revizyon olarak kaydedilecek. Mevcut sürüm
            dönüş noktası olarak korunacak.
          </p>
        )}
        {restorePlan && (
          <section aria-label="Geri yükleme bağlantı önizlemesi">
            <p>
              Chain:{" "}
              {restorePlan.hops.length
                ? restorePlan.hops.map((h) => h.name).join(" → ")
                : "Doğrudan bağlantı"}
            </p>
            <p>
              {restorePlan.routeUnchanged
                ? "Chain değişmiyor."
                : `Güncel bağlantı alıcısı: ${restorePlan.recipientCount}.`}{" "}
              Kişisel kimlikler değişmez; güncel yetkiler tekrar kontrol edilir.
            </p>
          </section>
        )}
        <div className="ts-actions">
          <button
            disabled={
              busy ||
              !chosen?.content ||
              chosen.content.kind !== "host" ||
              !current?.permissions.includes("connect")
            }
            onClick={() =>
              void run(async () => {
                await rpc({ kind: "selectHistory", id, revision: selected });
                await load();
                setNotice(
                  `Geçmiş sürüm ${selected} kişisel bağlantı görünümünde kullanılıyor. Ortak kayıt değişmedi.`,
                );
                onChanged();
              })
            }
          >
            Geçici kullan (SSH)
          </button>
          {pins.some((p) => p.purpose === "temporary") && (
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await rpc({ kind: "selectHistory", id, revision: null });
                  await load();
                  setNotice("Bağlantılarda güncel sürüme dönüldü.");
                  onChanged();
                })
              }
            >
              Güncele dön
            </button>
          )}
          <button
            disabled={busy || !canRestore}
            onClick={() => {
              if (!confirm) {
                if (chosen?.content?.kind === "host")
                  void run(async () => {
                    setRestorePlan(
                      await rpc({
                        kind: "prepareRestore",
                        id,
                        sourceRevision: selected,
                        currentRevision: current!.revision,
                      }),
                    );
                    setConfirm(true);
                  });
                else setConfirm(true);
                return;
              }
              void run(async () => {
                const result = restorePlan
                  ? await rpc<{ revision: number }>({
                      kind: "finishChain",
                      planId: restorePlan.planId,
                      cancel: false,
                    })
                  : await rpc<{ revision: number }>({
                      kind: "restoreVersion",
                      id,
                      sourceRevision: selected,
                      currentRevision: current!.revision,
                      clientAt: new Date().toISOString(),
                    });
                setConfirm(false);
                setRestorePlan(null);
                await load();
                setNotice(`Sürüm ${result.revision} kaydedildi.`);
                onChanged();
              });
            }}
          >
            {confirm
              ? "Geri yüklemeyi uygula"
              : "Yeni revizyon olarak geri yükle"}
          </button>
          <button disabled={busy} onClick={() => void run(load)}>
            Yenile
          </button>
          <button onClick={onClose}>Kapat</button>
        </div>
      </section>
    </div>
  );
}
