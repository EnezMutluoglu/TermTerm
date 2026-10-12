import { useEffect, useRef, useState } from "react";
import { errorText } from "./api";

export type DelegationState = {
  aclRevision: number;
  delegations: {
    accountId: string;
    resourceId: string;
    permissions: string[];
  }[];
};
const labels: Record<string, string> = {
  read: "Görüntüleme",
  connect: "Bağlantı",
  edit: "Düzenleme",
};
export default function TeamDelegation({
  resource,
  members,
  rpc,
  onClose,
  onSaved,
}: {
  resource: { id: string; name: string };
  members: {
    accountId: string;
    displayName: string;
    role: string;
    status: string;
  }[];
  rpc: (action: Record<string, unknown>) => Promise<any>;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [snapshot, setSnapshot] = useState<DelegationState | null>(null);
  const [account, setAccount] = useState("");
  const [permissions, setPermissions] = useState<string[]>([]);
  const [preview, setPreview] = useState(false),
    [busy, setBusy] = useState(true),
    [error, setError] = useState("");
  const live = useRef(true),
    gate = useRef(false);
  useEffect(() => {
    live.current = true;
    void rpc({ kind: "delegations" })
      .then((v) => {
        if (live.current) setSnapshot(v);
      })
      .catch((e) => {
        if (live.current) setError(errorText(e));
      })
      .finally(() => {
        if (live.current) setBusy(false);
      });
    return () => {
      live.current = false;
    };
  }, []);
  const before =
    snapshot?.delegations.find(
      (d) => d.accountId === account && d.resourceId === resource.id,
    )?.permissions ?? [];
  async function save() {
    if (gate.current || !snapshot) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      await rpc({
        kind: "setDelegation",
        accountId: account,
        resourceId: resource.id,
        permissions,
        expectedRevision: snapshot.aclRevision,
      });
      if (live.current) onSaved();
    } catch (e) {
      if (live.current) {
        setError(errorText(e));
        setPreview(false);
      }
    } finally {
      gate.current = false;
      if (live.current) setBusy(false);
    }
  }
  const describe = (values: string[]) =>
    values.map((p) => labels[p]).join(", ") || "Yetki devri yok";
  return (
    <div className="ts-overlay">
      <section
        className="ts-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Yetki devri"
      >
        <h2>{resource.name} · Yetki devri</h2>
        <p>
          Yönetici bu kapsamın altındaki kaynaklara yalnız seçtiğin izinleri
          verebilir. Önce kendisine kaynak erişimi vermelisin. Kendi yetkisini
          veya başka birinin yöneticilik sınırını değiştiremez.
        </p>
        {error && <p role="alert">{error}</p>}
        {!preview ? (
          <>
            <label>
              Kapsam yöneticisi
              <select
                aria-label="Kapsam yöneticisi"
                value={account}
                disabled={busy}
                onChange={(e) => {
                  setAccount(e.target.value);
                  setPermissions(
                    snapshot?.delegations.find(
                      (d) =>
                        d.accountId === e.target.value &&
                        d.resourceId === resource.id,
                    )?.permissions ?? [],
                  );
                }}
              >
                <option value="">Yönetici seç</option>
                {members
                  .filter((m) => m.role === "manager")
                  .map((m) => (
                    <option key={m.accountId} value={m.accountId}>
                      {m.displayName}
                      {m.status === "disabled" ? " · Devre dışı" : ""}
                    </option>
                  ))}
              </select>
            </label>
            {Object.entries(labels).map(([p, label]) => (
              <label className="ts-check" key={p}>
                <input
                  type="checkbox"
                  checked={permissions.includes(p)}
                  disabled={busy || !account}
                  onChange={(e) =>
                    setPermissions((values) =>
                      e.target.checked
                        ? [...values, p]
                        : values.filter((v) => v !== p),
                    )
                  }
                />
                {label} izni verebilir
              </label>
            ))}
            <p>
              Tüm kutuları kaldırmak bu kapsamdaki yetki devrini iptal eder.
              Kişinin kendi host erişimini değiştirmez.
            </p>
            <button
              disabled={busy || !snapshot || !account}
              onClick={() => setPreview(true)}
            >
              Değişikliği önizle
            </button>
          </>
        ) : (
          <>
            <p>{members.find((m) => m.accountId === account)?.displayName}</p>
            <dl>
              <dt>Önce</dt>
              <dd>{describe(before)}</dd>
              <dt>Sonra</dt>
              <dd>{describe(permissions)}</dd>
            </dl>
            <button disabled={busy} onClick={() => void save()}>
              Yetki devrini uygula
            </button>
            <button disabled={busy} onClick={() => setPreview(false)}>
              Geri
            </button>
          </>
        )}
        <button disabled={busy} onClick={onClose}>
          Vazgeç
        </button>
      </section>
    </div>
  );
}
