import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, errorText } from "./api";
export type TunnelRequest = { key: string; hostId: string; label: string };
type Row = {
  id: string;
  label: string;
  mode: string;
  port?: number;
  status: string;
  error?: string;
};
export default function TeamTunnels({
  request,
}: {
  request: TunnelRequest | null;
}) {
  const live = useRef(true),
    ids = useRef(new Set<string>()),
    gate = useRef(false),
    seen = useRef("");
  const [rows, setRows] = useState<Row[]>([]),
    [draft, setDraft] = useState<TunnelRequest | null>(null),
    [mode, setMode] = useState("local"),
    [port, setPort] = useState(0),
    [target, setTarget] = useState("127.0.0.1"),
    [targetPort, setTargetPort] = useState(5432),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    live.current = true;
    const un = listen<{
      id: string;
      status: string;
      port?: number;
      error?: string;
    }>("tunnel-event", ({ payload }) => {
      if (ids.current.has(payload.id))
        setRows((rows) =>
          rows.map((r) => (r.id === payload.id ? { ...r, ...payload } : r)),
        );
    });
    return () => {
      live.current = false;
      void un.then((f) => f());
      for (const id of ids.current)
        void call("tunnel_stop", { id }).catch(() => {});
      ids.current.clear();
    };
  }, []);
  useEffect(() => {
    if (request && seen.current !== request.key) {
      seen.current = request.key;
      setDraft(request);
      setError("");
    }
  }, [request]);
  async function start() {
    if (!draft || gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    const id = crypto.randomUUID();
    ids.current.add(id);
    setRows((rows) => [
      ...rows,
      { id, label: draft.label, mode, status: "starting" },
    ]);
    try {
      await call("team_v2_tunnel", {
        id,
        tunnel: {
          hostId: draft.hostId,
          mode,
          bindAddress: "127.0.0.1",
          bindPort: port,
          targetAddress: target,
          targetPort,
        },
      });
      if (live.current) setDraft(null);
    } catch (e) {
      if (live.current) {
        setError(errorText(e));
        setRows((rows) =>
          rows.map((r) =>
            r.id === id ? { ...r, status: "failed", error: errorText(e) } : r,
          ),
        );
      }
    } finally {
      gate.current = false;
      if (live.current) setBusy(false);
    }
  }
  async function stop(id: string) {
    try {
      await call("tunnel_stop", { id });
      if (live.current)
        setRows((rows) =>
          rows.map((r) => (r.id === id ? { ...r, status: "stopped" } : r)),
        );
    } catch (e) {
      if (live.current) setError(errorText(e));
    }
  }
  const status: Record<string, string> = {
    starting: "Bağlanıyor",
    running: "Çalışıyor",
    stopped: "Durduruldu",
    failed: "Başarısız",
    connectionError: "İletim hatası",
  };
  return (
    <section>
      <p>
        Hostun sağ tık menüsünden tünel açabilirsin. Tüneller kişisel bağlantı
        profilini kullanır ve yalnız localhost üzerinde dinler. Bu oturumda
        açılan tüneller listelenir.
      </p>
      {error && <p role="alert">{error}</p>}
      <table>
        <thead>
          <tr>
            <th>Host</th>
            <th>Tür</th>
            <th>Port</th>
            <th>Durum</th>
            <th>İşlem</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.id}>
              <td>{r.label}</td>
              <td>{r.mode}</td>
              <td>{r.port ?? "—"}</td>
              <td title={r.error}>{status[r.status] ?? r.status}</td>
              <td>
                <button
                  disabled={["starting", "stopped", "failed"].includes(
                    r.status,
                  )}
                  onClick={() => void stop(r.id)}
                >
                  Durdur
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {!rows.length && <p>Açık tünel yok.</p>}
      {draft && (
        <div className="ts-overlay">
          <section
            className="ts-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Team tüneli"
          >
            <h2>{draft.label} · Tünel aç</h2>
            {error && <p role="alert">{error}</p>}
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void start();
              }}
            >
              <label>
                Tür
                <select
                  disabled={busy}
                  value={mode}
                  onChange={(e) => setMode(e.target.value)}
                >
                  <option value="local">Yerel port yönlendirme</option>
                  <option value="dynamic">SOCKS5</option>
                  <option value="remote">Uzak port yönlendirme</option>
                </select>
              </label>
              <label>
                {mode === "remote"
                  ? "SSH sunucusunda dinleme portu"
                  : "Bu bilgisayarda dinleme portu"}
                <input
                  type="number"
                  min={0}
                  max={65535}
                  required
                  disabled={busy}
                  value={port}
                  onChange={(e) => setPort(Number(e.target.value))}
                />
              </label>
              <small>0 seçilirse boş bir port atanır.</small>
              {mode !== "dynamic" && (
                <>
                  <label>
                    {mode === "remote"
                      ? "Bu bilgisayardan erişilecek hedef"
                      : "SSH sunucusundan erişilecek hedef"}
                    <input
                      required
                      disabled={busy}
                      value={target}
                      onChange={(e) => setTarget(e.target.value)}
                    />
                  </label>
                  <label>
                    Hedef port
                    <input
                      type="number"
                      min={1}
                      max={65535}
                      required
                      disabled={busy}
                      value={targetPort}
                      onChange={(e) => setTargetPort(Number(e.target.value))}
                    />
                  </label>
                </>
              )}
              <div className="ts-actions">
                <button className="ts-primary" disabled={busy}>
                  {busy ? "Bağlanıyor…" : "Tüneli başlat"}
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setDraft(null)}
                >
                  Kapat
                </button>
              </div>
            </form>
          </section>
        </div>
      )}
    </section>
  );
}
