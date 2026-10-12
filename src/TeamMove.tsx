import { useEffect, useRef, useState } from "react";
import type { TeamHostRow } from "./TeamHosts";
type Preview = { name: string; requiredKeys: number; affectedAccess: number };
type Impact = {
  resourceId: string;
  accountId: string;
  displayName: string;
  before: string[];
  after: string[];
};
export default function TeamMove({
  row,
  rows,
  rpc,
  onClose,
  onSaved,
}: {
  row: TeamHostRow;
  rows: TeamHostRow[];
  rpc: (a: Record<string, unknown>) => Promise<any>;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [destination, setDestination] = useState(""),
    [preview, setPreview] = useState<Preview | null>(null),
    [impact, setImpact] = useState<Impact[]>([]),
    [next, setNext] = useState<number | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [processed, setProcessed] = useState(0),
    [chains, setChains] = useState(0),
    [shared, setShared] = useState(0);
  const [applyAttempted, setApplyAttempted] = useState(false);
  const live = useRef(true),
    gate = useRef(false);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  async function run(fn: () => Promise<void>) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      if (live.current) setError(String(e));
    } finally {
      gate.current = false;
      if (live.current) setBusy(false);
    }
  }
  async function page(after: number) {
    const value = await rpc({ kind: "moveImpact", after });
    if (live.current) {
      setImpact((old) => (after ? [...old, ...value.items] : value.items));
      setNext(value.nextAfter);
      setChains(value.chainReferences);
      setShared(value.sharedIdentities);
    }
  }
  const folders = rows.filter(
    (r) => r.content?.kind === "folder" && r.id !== row.id && !r.navigationOnly,
  );
  const current = row.content?.parentId ?? "";
  return (
    <div className="ts-overlay">
      <section
        className="ts-dialog ts-wide"
        role="dialog"
        aria-modal="true"
        aria-label="Kaynağı taşı"
      >
        <h2>Taşı · {row.content?.name}</h2>
        <p>
          Taşıma çevrimiçi yapılır. Kayıt, alıcı veya chain değişirse yeni
          önizleme gerekir.
        </p>
        {error && <p role="alert">{error}</p>}
        <label>
          Hedef klasör
          <select
            aria-label="Hedef klasör"
            value={destination}
            disabled={busy || !!preview}
            onChange={(e) => setDestination(e.target.value)}
          >
            <option value="">Kasa kökü</option>
            {folders.map((f) => (
              <option key={f.id} value={f.id}>
                {f.content?.name}
              </option>
            ))}
          </select>
        </label>
        {preview && (
          <>
            <p>
              {preview.affectedAccess} kullanıcı/kayıt erişimi değişiyor ·{" "}
              {chains} chain bağlantısı · {shared} ortak kimlik kapsamı.
            </p>
            <p>
              Ortak kimliklerin mevcut alıcı seçimi korunur; yeni host erişimi
              bir ortak sırrı kendiliğinden paylaşmaz. Önceden dışarı çıkarılmış
              anahtarlar geri alınamaz.
            </p>
            <table>
              <thead>
                <tr>
                  <th>Kayıt</th>
                  <th>Kullanıcı</th>
                  <th>Önce</th>
                  <th>Sonra</th>
                </tr>
              </thead>
              <tbody>
                {impact.map((i) => (
                  <tr key={i.resourceId + i.accountId}>
                    <td>
                      {rows.find((r) => r.id === i.resourceId)?.content?.name ??
                        i.resourceId}
                    </td>
                    <td>{i.displayName}</td>
                    <td>{i.before.join(", ") || "Erişim yok"}</td>
                    <td>{i.after.join(", ") || "Erişim yok"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {next !== null && (
              <button
                disabled={busy}
                onClick={() => void run(() => page(next))}
              >
                Sonraki erişimler
              </button>
            )}
            <p role="status">
            {processed} / {preview.requiredKeys} anahtar hazırlandı.
            {applyAttempted ? " Uygulama yanıtı alınamadıysa sonuç belirsiz olabilir; kapattıktan sonra güncel kaydı kontrol et." : " Henüz taşınmadı."}
            </p>
            {processed < preview.requiredKeys && (
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const p = await rpc({ kind: "stageMove" });
                    if (live.current) setProcessed(p.processed);
                  })
                }
              >
                Sonraki anahtarları hazırla
              </button>
            )}
          </>
        )}
        <div className="ts-actions">
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                try {
                  if (preview) await rpc({ kind: "finishMove", cancel: true });
                } finally {
                  onClose();
                }
              })
            }
          >
            Vazgeç
          </button>
          {!preview ? (
            <button
              disabled={busy || destination === current}
              onClick={() =>
                void run(async () => {
                  const p = await rpc({
                    kind: "prepareMove",
                    id: row.id,
                    destinationId: destination || null,
                    clientAt: new Date().toISOString(),
                  });
                  if (live.current) setPreview(p);
                  await page(0);
                })
              }
            >
              Taşıma ve erişim önizlemesi
            </button>
          ) : (
            <button
              disabled={
                busy || processed < preview.requiredKeys || next !== null
              }
              onClick={() =>
                void run(async () => {
                  setApplyAttempted(true);
                  await rpc({ kind: "finishMove", cancel: false });
                  onSaved();
                  onClose();
                })
              }
            >
              Onayla ve taşı
            </button>
          )}
        </div>
      </section>
    </div>
  );
}
