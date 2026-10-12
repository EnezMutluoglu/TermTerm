import { useEffect, useState } from "react";
import { errorText } from "./api";
import TeamProfile from "./TeamProfile";
import type { ProxyEndpoint } from "./TeamProxy";
type Hop = {
  proxy?: ProxyEndpoint | null;
  id: string;
  name: string;
  usernamePolicy: string | null;
  username: string | null;
};
export default function TeamRouteProfiles({
  id,
  rpc,
  onClose,
}: {
  id: string;
  rpc: (action: Record<string, unknown>) => Promise<any>;
  onClose: () => void;
}) {
  const [hops, setHops] = useState<Hop[]>([]),
    [selected, setSelected] = useState<Hop | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    void rpc({ kind: "connectionProfiles", id })
      .then((result) => {
        if (active) setHops(result);
      })
      .catch((e) => {
        if (active) setError(errorText(e));
      });
    return () => {
      active = false;
    };
  }, [id]);
  if (selected)
    return (
      <TeamProfile
        key={selected.id}
        host={selected}
        root={id}
        onClose={() => setSelected(null)}
      />
    );
  return (
    <div className="ts-overlay">
      <section
        className="ts-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Zincirdeki kişisel profillerim"
      >
        <h2>Zincirdeki kişisel profillerim</h2>
        <p>
          Her adım için kendi kimliğini seçebilirsin. Jump host profili
          oluşturmak o hosta bağımsız bağlantı izni vermez.
        </p>
        {error && (
          <div role="alert" className="ts-error">
            {error}
          </div>
        )}
        {!error && !hops.length && (
          <p role="status">Bağlantı adımları yükleniyor…</p>
        )}
        <ol>
          {hops.map((hop) => (
            <li key={hop.id}>
              <button onClick={() => setSelected(hop)}>
                {hop.name} · Profili düzenle
              </button>
            </li>
          ))}
        </ol>
        <button onClick={onClose}>Kapat</button>
      </section>
    </div>
  );
}
