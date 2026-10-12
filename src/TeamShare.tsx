import { useEffect, useRef, useState } from "react";
import { errorText } from "./api";
type Recipient = {
  accountId: string;
  displayName: string;
  connect: boolean;
  reveal: boolean;
  export: boolean;
};
type Preview = { id: string; recipients: Recipient[] };
export default function TeamShare({
  source,
  scopes,
  rpc,
  onClose,
  onSaved,
  privateSources = [],
}: {
  source: { id: string; label: string; visibility?: string };
  privateSources?: { id: string; label: string }[];
  scopes: { id: string; name: string }[];
  rpc: (action: Record<string, unknown>) => Promise<any>;
  onClose: () => void;
  onSaved: () => void;
}) {
  const updating = source.visibility === "shared";
  const [replacement, setReplacement] = useState("");
  const [expectedRevision, setExpectedRevision] = useState<number | null>(null);
  const [scope, setScope] = useState(""),
    [recipients, setRecipients] = useState<Recipient[]>([]),
    [preview, setPreview] = useState<Preview | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const live = useRef(true),
    gate = useRef(false),
    epoch = useRef(0);
  useEffect(() => {
    live.current = true;
    if (updating)
      void run(async () => {
        const result = await rpc({
          kind: "sharedUpdateOptions",
          id: source.id,
        });
        if (live.current) {
          setScope(result.scopeId);
          setExpectedRevision(result.revision);
          setRecipients(result.recipients);
        }
      });
    return () => {
      live.current = false;
      epoch.current++;
    };
  }, []);
  async function choose(id: string) {
    const stamp = ++epoch.current;
    setScope(id);
    setRecipients([]);
    setError("");
    if (!id) return;
    setBusy(true);
    try {
      const users = await rpc({ kind: "shareCandidates", scopeId: id });
      if (live.current && epoch.current === stamp)
        setRecipients(
          users.map((u: any) => ({
            ...u,
            connect: false,
            reveal: false,
            export: false,
          })),
        );
    } catch (e) {
      if (live.current && epoch.current === stamp) setError(errorText(e));
    } finally {
      if (live.current && epoch.current === stamp) setBusy(false);
    }
  }
  async function run(action: () => Promise<void>) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      gate.current = false;
      if (live.current) setBusy(false);
    }
  }
  return (
    <div className="ts-overlay">
      <section
        className="ts-dialog ts-wide"
        role="dialog"
        aria-modal="true"
        aria-label="Ortak kimlik paylaşımı"
      >
        <h2>
          {source.label} ·{" "}
          {updating ? "Ortak kimliği güncelle" : "Ortak kimlik olarak paylaş"}
        </h2>
        <p>
          {updating
            ? "Aynı ortak kaydın yeni sürümü oluşturulur. Yeni alıcılar yalnız bu sürüme erişir; çıkarılan alıcıların sunucudaki eski anahtar zarfları da kaldırılır. Özel kaynak değiştirilmez."
            : "Yeni ve bağımsız bir ortak kopya oluşturulur. Özel kimliğin korunur; sonraki özel değişiklikler ortak kopyaya aktarılmaz."}
        </p>
        {error && <p role="alert">{error}</p>}
        {!preview ? (
          <>
            {updating && (
              <label>
                Yeni sürümün içeriği
                <select
                  aria-label="Yeni sürümün içeriği"
                  disabled={busy}
                  value={replacement}
                  onChange={(e) => setReplacement(e.target.value)}
                >
                  <option value="">
                    Mevcut ortak kimliği koru, alıcıları güncelle
                  </option>
                  {privateSources.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.label} — özel kimlikten yeni kopya
                    </option>
                  ))}
                </select>
              </label>
            )}
            {!updating && (
              <>
                <label>
                  İzinli kapsam
                  <select
                    aria-label="Paylaşım kapsamı"
                    value={scope}
                    disabled={busy}
                    onChange={(e) => void choose(e.target.value)}
                  >
                    <option value="">Klasör veya host seç</option>
                    {scopes.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name}
                      </option>
                    ))}
                  </select>
                </label>
              </>
            )}
            {scope && (
              <table>
                <thead>
                  <tr>
                    <th>Üye</th>
                    <th>Bağlantıda kullan</th>
                    <th>Sırrı gör</th>
                    <th>Dışa aktar</th>
                  </tr>
                </thead>
                <tbody>
                  {recipients.map((r) => (
                    <tr key={r.accountId}>
                      <td>{r.displayName}</td>
                      {(["connect", "reveal", "export"] as const).map(
                        (flag) => (
                          <td key={flag}>
                            <input
                              type="checkbox"
                              aria-label={`${r.displayName} ${flag}`}
                              checked={r[flag]}
                              disabled={busy}
                              onChange={(e) =>
                                setRecipients((rows) =>
                                  rows.map((x) =>
                                    x.accountId === r.accountId
                                      ? { ...x, [flag]: e.target.checked }
                                      : x,
                                  ),
                                )
                              }
                            />
                          </td>
                        ),
                      )}
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            <p>
              Kaynak sahibi ve takım sahibi ortak kopyayı yönetebilir. Sonraki
              ekranda zorunlu alıcılar dahil tam listeyi göreceksin.
            </p>
            <button
              disabled={busy || !scope}
              onClick={() =>
                void run(async () => {
                  const value = await rpc({
                    ...(updating
                      ? {
                          kind: "prepareSharedUpdate",
                          id: source.id,
                          expectedRevision,
                          sourceId: replacement || null,
                        }
                      : {
                          kind: "prepareShare",
                          sourceId: source.id,
                          scopeId: scope,
                        }),
                    recipients: recipients
                      .filter((r) => r.connect || r.reveal || r.export)
                      .map(({ accountId, connect, reveal, export: exp }) => ({
                        accountId,
                        connect,
                        reveal,
                        export: exp,
                      })),
                  });
                  if (live.current) setPreview(value);
                })
              }
            >
              Paylaşımı önizle
            </button>
          </>
        ) : (
          <>
            <h3>{scopes.find((s) => s.id === scope)?.name}</h3>
            <table>
              <thead>
                <tr>
                  <th>Alıcı</th>
                  <th>Bağlantı</th>
                  <th>Sırrı gör</th>
                  <th>Dışa aktar</th>
                </tr>
              </thead>
              <tbody>
                {preview.recipients.map((r) => (
                  <tr key={r.accountId}>
                    <td>{r.displayName}</td>
                    {[r.connect, r.reveal, r.export].map((v, i) => (
                      <td key={i}>{v ? "İzinli" : "Kapalı"}</td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
            <p>
              İptal edilen paylaşım kayıt oluşturmaz. Paylaşımı sonradan
              kaldırmak, daha önce dışarı alınmış kopyaları geri çekemez.
            </p>
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await rpc({
                    kind: updating ? "finishSharedUpdate" : "finishShare",
                    planId: preview.id,
                  });
                  if (live.current) onSaved();
                })
              }
            >
              {updating ? "Yeni sürümü kaydet" : "Ortak kopyayı oluştur"}
            </button>
          </>
        )}
        <button
          disabled={busy}
          onClick={() =>
            void run(async () => {
              if (preview)
                await rpc({
                  kind: updating ? "cancelSharedUpdate" : "cancelShare",
                  planId: preview.id,
                });
              if (live.current) onClose();
            })
          }
        >
          Vazgeç
        </button>
      </section>
    </div>
  );
}
