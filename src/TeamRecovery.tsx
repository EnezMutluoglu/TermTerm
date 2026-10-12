import { useEffect, useRef, useState } from "react";
import { call, chooseFile, errorText } from "./api";
type Preview = {
  operationId: string;
  displayName: string;
  publicKey: string;
  total: number;
  verification: boolean;
};
export default function TeamRecovery({
  member,
  onClose,
  onSaved,
}: {
  member: { accountId: string; displayName: string } | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [path, setPath] = useState(""),
    [password, setPassword] = useState(""),
    [reauth, setReauth] = useState(""),
    [reason, setReason] = useState("");
  const [pending, setPending] = useState<
      { id: string; actor: string; createdAt: string }[]
    >([]),
    [preview, setPreview] = useState<Preview | null>(null);
  const [busy, setBusy] = useState(false),
    [running, setRunning] = useState(false),
    [error, setError] = useState(""),
    [processed, setProcessed] = useState(0),
    [done, setDone] = useState(false),
    [stopping, setStopping] = useState(false);
  const live = useRef(true),
    gate = useRef(false),
    stop = useRef(false);
  async function rpc<T = any>(action: Record<string, unknown>) {
    return call<T>("team_v2_action", { action });
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
    if (!member)
      void run(async () => {
        const rows = await rpc({ kind: "recoveryPending" });
        if (live.current) setPending(rows);
      });
    return () => {
      live.current = false;
      stop.current = true;
    };
  }, []);
  async function execute() {
    if (!preview) return;
    stop.current = false;
    setRunning(true);
    setStopping(false);
    try {
      await run(async () => {
        for (;;) {
          if (stop.current) {
            await rpc({ kind: "recoveryCancel" });
            if (live.current) {
              setPreview(null);
              setStopping(false);
            }
            return;
          }
          const r = await rpc({ kind: "recoveryStep" });
          if (!live.current) return;
          setProcessed(r.processed);
          if (r.completed) {
            setDone(true);
            onSaved();
            return;
          }
        }
      });
    } finally {
      if (live.current) setRunning(false);
    }
  }
  return (
    <div className="ts-overlay">
      <section
        className="ts-dialog ts-wide"
        role="dialog"
        aria-modal="true"
        aria-label="Kimlik kurtarma"
      >
        <h2>
          {member
            ? `${member.displayName} · Kimlik kurtarma`
            : "Kurtarmayı kendi cihazımda doğrula"}
        </h2>
        <p>
          Parola sıfırlama ve eski kimliklerin kurtarılması ayrı işlemlerdir.
          Sırlar ekrana veya işlem geçmişine yazılmaz.
        </p>
        {error && <p role="alert">{error}</p>}
        {!preview && member && (
          <>
            <label>
              Kurtarma dosyası
              <input value={path} readOnly />
            </label>
            <button
              disabled={busy}
              onClick={() =>
                void chooseFile(
                  ["ttrecovery"],
                  "Şirket kurtarma dosyasını seç",
                ).then((p) => {
                  if (p && live.current) setPath(p);
                })
              }
            >
              Dosya seç
            </button>
            <label>
              Kurtarma dosyası parolası
              <input
                type="password"
                value={password}
                disabled={busy}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="off"
              />
            </label>
            <label>
              Kendi Team parolan
              <input
                type="password"
                value={reauth}
                disabled={busy}
                onChange={(e) => setReauth(e.target.value)}
                autoComplete="off"
              />
            </label>
            <label>
              Kurtarma gerekçesi
              <textarea
                value={reason}
                disabled={busy}
                onChange={(e) => setReason(e.target.value)}
                minLength={10}
                maxLength={1000}
              />
            </label>
            <button
              disabled={
                busy ||
                !path ||
                !password ||
                !reauth ||
                reason.trim().length < 10
              }
              onClick={() =>
                void run(async () => {
                  const p = password,
                    r = reauth;
                  setPassword("");
                  setReauth("");
                  const result = await rpc({
                    kind: "recoveryBegin",
                    accountId: member.accountId,
                    path,
                    password: p,
                    reauthentication: r,
                    reason,
                  });
                  if (live.current) setPreview(result);
                })
              }
            >
              Kurtarmayı önizle
            </button>
          </>
        )}
        {!preview && !member && (
          <>
            {pending.length === 0 && !busy && (
              <p>Doğrulama bekleyen kurtarma işlemi yok.</p>
            )}
            {pending.map((p) => (
              <div className="ts-actions" key={p.id}>
                <span>
                  {p.actor} · {new Date(p.createdAt).toLocaleString("tr-TR")}
                </span>
                <button
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      const r = await rpc({
                        kind: "recoveryVerify",
                        operationId: p.id,
                      });
                      if (live.current) setPreview(r);
                    })
                  }
                >
                  Bu kurtarmayı doğrula
                </button>
              </div>
            ))}
          </>
        )}
        {preview && (
          <>
            <p>
              Alıcı: <strong>{preview.displayName}</strong> · {preview.total}{" "}
              kimlik / kişisel kullanıcı adı ve saklanan sürümleri
            </p>
            <label>
              Alıcının public key bilgisi
              <textarea readOnly value={preview.publicKey} />
            </label>
            <p>
              {preview.verification
                ? "Her sürüm bu cihazın anahtarıyla açılıp bütünlüğü doğrulanacak. Eksik veya bozuk içerikte onay verilmez."
                : "Bu alıcının yeni anahtarına erişim yeniden sarılacak. Tamamlandığında alıcının Kimliklerim ekranından doğrulaması gerekir."}
            </p>
            <p role="status">
              {done
                ? preview.verification
                  ? "Kurtarma doğrulandı ve tamamlandı."
                  : "Anahtarlar yeniden sarıldı. Alıcının doğrulaması bekleniyor."
                : stopping
                  ? "Durduruluyor; tamamlanan güvenli adımlar korunuyor."
                  : `${processed} / ${preview.total} kayıt işlendi`}
            </p>
            {!done && (
              <button disabled={busy} onClick={() => void execute()}>
                {preview.verification
                  ? "Açılabildiğini doğrula"
                  : "Anahtarları yeniden sar"}
              </button>
            )}
          </>
        )}
        {running ? (
          <button
            disabled={stopping}
            onClick={() => {
              stop.current = true;
              setStopping(true);
            }}
          >
            Durdur
          </button>
        ) : (
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                if (preview && !done) await rpc({ kind: "recoveryCancel" });
                if (live.current) onClose();
              })
            }
          >
            {done ? "Kapat" : "Vazgeç"}
          </button>
        )}
      </section>
    </div>
  );
}
