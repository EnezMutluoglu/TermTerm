import { useEffect, useRef, useState } from "react";
import { call, errorText } from "./api";
type Pin = {
  id: string;
  displayName: string;
  accountStatus: string;
  name: string;
  resourceId: string;
  revision: number;
  purpose: string;
  createdAt: string;
};
type Plan = { id: string; pins: number; items: Pin[] };
export default function TeamPins({ onClose }: { onClose: () => void }) {
  const [items, setItems] = useState<Pin[]>([]),
    [after, setAfter] = useState<string | null>(null),
    [selected, setSelected] = useState<string[]>([]),
    [plan, setPlan] = useState<Plan | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const live = useRef(true),
    gate = useRef(false);
  const rpc = <T,>(action: Record<string, unknown>) =>
    call<T>("team_v2_action", { action });
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
  async function load(cursor: string | null) {
    const result = await rpc<{ items: Pin[]; nextAfter: string | null }>({
      kind: "reviewPins",
      after: cursor,
    });
    if (live.current) {
      setItems((old) => (cursor ? [...old, ...result.items] : result.items));
      setAfter(result.nextAfter);
    }
  }
  useEffect(() => {
    live.current = true;
    void run(() => load(null));
    return () => {
      live.current = false;
    };
  }, []);
  return (
    <div className="ts-overlay">
      <section
        className="ts-dialog ts-wide"
        role="dialog"
        aria-modal="true"
        aria-label="Korunan geçmiş sürümleri"
      >
        <h2>Korunan geçmiş sürümleri</h2>
        <p>
          Yaşa göre otomatik silinmez. Koruma kaldırıldığında son 10 sürüm
          dışındaki ve başka kullanıcı tarafından korunmayan içerik silinebilir.
          Kullanıcının geçici bağlantı seçimi güncele döner.
        </p>
        {error && <p role="alert">{error}</p>}
        {notice && <p role="status">{notice}</p>}
        {!plan ? (
          <>
            <table>
              <thead>
                <tr>
                  <th>Seç</th>
                  <th>Hesap</th>
                  <th>Kayıt / sürüm</th>
                  <th>Koruma</th>
                  <th>Tarih</th>
                </tr>
              </thead>
              <tbody>
                {items.map((p) => (
                  <tr key={p.id}>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`${p.displayName} ${p.name} sürüm ${p.revision}`}
                        checked={selected.includes(p.id)}
                        disabled={
                          busy ||
                          (!selected.includes(p.id) && selected.length >= 100)
                        }
                        onChange={(e) =>
                          setSelected((old) =>
                            e.target.checked
                              ? [...old, p.id]
                              : old.filter((x) => x !== p.id),
                          )
                        }
                      />
                    </td>
                    <td>
                      {p.displayName}
                      {p.accountStatus === "disabled" ? " · Devre dışı" : ""}
                    </td>
                    <td title={p.resourceId}>
                      {p.name} · {p.revision}
                    </td>
                    <td>
                      {p.purpose === "temporary"
                        ? "Geçici kullanım"
                        : "Geri dönüş noktası"}
                    </td>
                    <td>{new Date(p.createdAt).toLocaleString("tr-TR")}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!busy && !items.length && <p>Korunan eski sürüm yok.</p>}
            {after && (
              <button
                disabled={busy}
                onClick={() => void run(() => load(after))}
              >
                Diğer kayıtlar
              </button>
            )}
            <button
              disabled={busy || !selected.length}
              onClick={() =>
                void run(async () => {
                  const p = await rpc<Plan>({
                    kind: "preparePinRelease",
                    ids: selected,
                  });
                  if (live.current) setPlan(p);
                })
              }
            >
              Kaldırmayı önizle
            </button>
          </>
        ) : (
          <>
            <h3>{plan.pins} koruma kaldırılacak</h3>
            <ul>
              {plan.items.map((p) => (
                <li key={p.id}>
                  {p.displayName} ·{" "}
                  {items.find((x) => x.id === p.id)?.name ?? p.resourceId} ·
                  Sürüm {p.revision} ·{" "}
                  {p.purpose === "temporary"
                    ? "Geçici kullanım"
                    : "Geri dönüş noktası"}
                </li>
              ))}
            </ul>
            <p>
              Önizlemeden sonra seçim değişirse işlem uygulanmaz. Diğer
              kullanıcıların korumaları devam eder.
            </p>
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const r = await rpc<{ released: number; pruned: number }>({
                    kind: "finishPinRelease",
                    planId: plan.id,
                    cancel: false,
                  });
                  if (live.current) {
                    setPlan(null);
                    setSelected([]);
                    setNotice(
                      `${r.released} koruma kaldırıldı, ${r.pruned} eski sürüm temizlendi.`,
                    );
                    await load(null);
                  }
                })
              }
            >
              Korumaları kaldır
            </button>
          </>
        )}
        <button
          disabled={busy}
          onClick={() =>
            void run(async () => {
              if (plan)
                await rpc({
                  kind: "finishPinRelease",
                  planId: plan.id,
                  cancel: true,
                });
              if (live.current) onClose();
            })
          }
        >
          {plan ? "Vazgeç" : "Kapat"}
        </button>
      </section>
    </div>
  );
}
