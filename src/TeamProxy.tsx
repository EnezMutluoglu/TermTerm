import { useEffect, useRef, useState } from "react";
import { call, errorText } from "./api";
export type ProxyEndpoint = {
  kind: string;
  host: string;
  port: number;
  authenticate: boolean;
};
type Status = {
  credentialId: string | null;
  revision: number;
  username?: string;
  endpoint?: ProxyEndpoint;
};
export default function TeamProxy({
  root,
  hostId,
  endpoint,
}: {
  root: string;
  hostId: string;
  endpoint: ProxyEndpoint;
}) {
  const [status, setStatus] = useState<Status | null>(null),
    [username, setUsername] = useState(""),
    [password, setPassword] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  const generation = useRef(0),
    gate = useRef(false);
  async function rpc<T>(action: Record<string, unknown>, epoch: number) {
    const r = await call<T>("team_v2_action", { action });
    if (epoch !== generation.current) throw Error("Ekran değişti");
    return r;
  }
  useEffect(() => {
    const epoch = ++generation.current;
    gate.current = false;
    setBusy(false);
    setStatus(null);
    setUsername("");
    setPassword("");
    setError("");
    setNotice("");
    void rpc<Status>({ kind: "proxyStatus", root, hostId }, epoch)
      .then((s) => {
        setStatus(s);
        setUsername(s.username ?? "");
      })
      .catch((e) => {
        if (epoch === generation.current) setError(errorText(e));
      });
    return () => {
      generation.current++;
    };
  }, [root, hostId]);
  async function save(forget = false) {
    if (gate.current || !status) return;
    gate.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    const epoch = generation.current;
    try {
      await rpc(
        {
          kind: "saveProxy",
          root,
          hostId,
          credentialId: status.credentialId,
          revision: status.revision,
          secret: forget ? null : { endpoint, username, password },
        },
        epoch,
      );
      setPassword("");
      const next = await rpc<Status>(
        { kind: "proxyStatus", root, hostId },
        epoch,
      );
      setStatus(next);
      setUsername(next.username ?? "");
      setNotice(
        forget
          ? "Kayıt kaldırıldı; bağlantıda sorulacak."
          : "Proxy kimliğin şifreli kaydedildi.",
      );
    } catch (e) {
      if (epoch === generation.current) setError(errorText(e));
    } finally {
      if (epoch === generation.current) {
        gate.current = false;
        setBusy(false);
      }
    }
  }
  const old = status?.endpoint,
    changed =
      old &&
      (old.kind !== endpoint.kind ||
        old.host !== endpoint.host ||
        old.port !== endpoint.port ||
        old.authenticate !== endpoint.authenticate);
  return (
    <section aria-label="Kişisel proxy kimliğim">
      <h3>Proxy kimliğim</h3>
      <p>
        {endpoint.kind.toUpperCase()} · {endpoint.host}:{endpoint.port}
      </p>
      <small>
        Yalnız hesabına şifreli kaydedilir. Proxy adresi değişirse eski parola
        gönderilmez. Parolayı boş bırakarak değiştiremezsin.
      </small>
      {status?.credentialId && <p role="status">Kayıtlı proxy kimliği var.</p>}
      {changed && (
        <p role="alert">
          Proxy adresi veya türü değişmiş. Bağlanmadan önce kimliğini yeniden
          kaydet veya eski kaydı kaldır.
        </p>
      )}
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <label>
        Proxy kullanıcı adı
        <input
          aria-label="Kayıtlı proxy kullanıcı adı"
          autoComplete="off"
          value={username}
          disabled={busy}
          onChange={(e) => setUsername(e.target.value)}
        />
      </label>
      <label>
        Proxy parolası
        <input
          aria-label="Kayıtlı proxy parolası"
          type="password"
          autoComplete="new-password"
          value={password}
          disabled={busy}
          onChange={(e) => setPassword(e.target.value)}
        />
      </label>
      <div className="ts-actions">
        <button
          type="button"
          disabled={busy || !status || !username || !password}
          onClick={() => void save()}
        >
          Proxy kimliğini kaydet
        </button>
        <button
          type="button"
          disabled={busy || !status?.credentialId}
          onClick={() => void save(true)}
        >
          Proxy kimliğini unut
        </button>
      </div>
    </section>
  );
}
