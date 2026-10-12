import { useState } from "react";
import { KeyRound, Search, LayoutGrid, List, X } from "lucide-react";
import ContextMenu, {
  menuPosition,
  type MenuAction,
  type MenuPosition,
} from "./ContextMenu";

export type TeamIdentity = {
  id: string;
  label: string;
  kind: string;
  revision: number;
  unavailable: boolean;
  visibility?: string;
  permissions?: string[];
  canRemove?: boolean;
  canUpdate?: boolean;
};
const kindLabel = (kind: string) =>
  ({ ssh_key: "SSH anahtarı", agent: "SSH agent", password: "Parola" })[kind] ??
  kind;

// This browser receives metadata only. Selecting a card never retrieves a secret.
export default function TeamIdentities({
  items,
  busy,
  actions,
  onError,
}: {
  items: TeamIdentity[];
  busy: boolean;
  actions: (item: TeamIdentity) => MenuAction[];
  onError: (error: unknown) => void;
}) {
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState("all");
  const [view, setView] = useState<"grid" | "list">("grid");
  const [selected, setSelected] = useState<string | null>(null);
  const [menu, setMenu] = useState<{
    id: string;
    position: MenuPosition;
  } | null>(null);
  const visible = items.filter(
    (item) =>
      item.label
        .toLocaleLowerCase("tr")
        .includes(query.toLocaleLowerCase("tr")) &&
      (scope === "all" ||
        (item.visibility === "shared" ? "shared" : "private") === scope),
  );
  const detail = items.find((item) => item.id === selected);
  // Resolve the current item on every render so refreshed permissions disable old menus.
  const target = items.find((item) => item.id === menu?.id);
  function show(
    event: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>,
    item: TeamIdentity,
  ) {
    event.preventDefault();
    if (!busy) setMenu({ id: item.id, position: menuPosition(event) });
  }
  return (
    <section className="team-identities" aria-label="Kimlik listesi">
      <div className="records-toolbar">
        <div className="search-field">
          <Search size={17} />
          <input
            aria-label="Kimliklerde ara"
            placeholder="Kimliklerde ara…"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setMenu(null);
            }}
          />
        </div>
        <div className="toolbar-right">
          <select
            aria-label="Kimlik görünürlüğü"
            value={scope}
            onChange={(e) => {
              setScope(e.target.value);
              setMenu(null);
            }}
          >
            <option value="all">Tüm kimlikler</option>
            <option value="private">Özel kimlikler</option>
            <option value="shared">Ortak kimlikler</option>
          </select>
          <span>{visible.length} kimlik</span>
          <div className="view-toggle">
            <button
              aria-label="Kimlik kart görünümü"
              aria-pressed={view === "grid"}
              className={view === "grid" ? "active" : ""}
              onClick={() => setView("grid")}
            >
              <LayoutGrid size={16} />
            </button>
            <button
              aria-label="Kimlik liste görünümü"
              aria-pressed={view === "list"}
              className={view === "list" ? "active" : ""}
              onClick={() => setView("list")}
            >
              <List size={17} />
            </button>
          </div>
        </div>
      </div>
      <div className="team-identity-layout">
        <div className={"records-grid " + (view === "list" ? "list" : "")}>
          {visible.map((item) => (
            <article
              key={item.id}
              role="button"
              tabIndex={0}
              aria-label={item.label}
              aria-disabled={busy}
              data-identity-id={item.id}
              className={
                "record-card " + (selected === item.id ? "selected" : "")
              }
              onClick={() => {
                if (!busy) setSelected(item.id);
              }}
              onContextMenu={(event) => show(event, item)}
              onKeyDown={(event) => {
                if (
                  event.key === "ContextMenu" ||
                  (event.key === "F10" && event.shiftKey)
                )
                  show(event, item);
                else if (
                  (event.key === "Enter" || event.key === " ") &&
                  !busy
                ) {
                  event.preventDefault();
                  setSelected(item.id);
                }
              }}
            >
              <div className="card-top">
                <div className="host-icon">
                  <KeyRound size={21} />
                </div>
                <div className="card-title">
                  <h3>{item.label}</h3>
                  <span>{kindLabel(item.kind)}</span>
                </div>
              </div>
              <footer>
                <span className="tag">
                  {item.visibility === "shared" ? "Ortak" : "Özel"}
                </span>
                <span>
                  {item.unavailable ? "Açılamıyor" : `Sürüm ${item.revision}`}
                </span>
              </footer>
            </article>
          ))}
          {!visible.length && (
            <p className="ts-empty">
              {items.length
                ? "Eşleşen kimlik yok."
                : "Henüz kişisel kimliğin yok."}
            </p>
          )}
        </div>
        {detail && (
          <aside
            className="team-identity-details"
            aria-label="Kimlik bilgileri"
          >
            <header>
              <h2>{detail.label}</h2>
              <button
                aria-label="Kimlik bilgilerini kapat"
                onClick={() => setSelected(null)}
              >
                <X size={17} />
              </button>
            </header>
            <dl className="detail-list">
              <div className="detail-row">
                <dt>Tür</dt>
                <dd>{kindLabel(detail.kind)}</dd>
              </div>
              <div className="detail-row">
                <dt>Görünürlük</dt>
                <dd>
                  {detail.visibility === "shared"
                    ? "Ortak kimlik"
                    : "Yalnız bana özel"}
                </dd>
              </div>
              <div className="detail-row">
                <dt>Sürüm</dt>
                <dd>{detail.revision}</dd>
              </div>
              <div className="detail-row">
                <dt>Durum</dt>
                <dd>
                  {detail.unavailable
                    ? "Şifreli içerik açılamıyor. Profil veya kurtarma işlemlerini kontrol edin."
                    : "Kullanılabilir"}
                </dd>
              </div>
              {detail.visibility === "shared" && (
                <div className="detail-row">
                  <dt>İzinlerin</dt>
                  <dd>
                    {[
                      ["connect", "Bağlanma"],
                      ["reveal", "Sırrı görüntüleme"],
                      ["export", "Dışa aktarma"],
                    ].map(([id, label]) => (
                      <div key={id}>
                        {label}:{" "}
                        {detail.permissions?.includes(id) ? "Var" : "Yok"}
                      </div>
                    ))}
                  </dd>
                </div>
              )}
            </dl>
            <p>
              İşlemler için kimliğe sağ tıklayın veya Shift+F10 kullanın. Bu
              alan parola veya özel anahtar içeriğini göstermez.
            </p>
          </aside>
        )}
      </div>
      {menu && target && (
        <ContextMenu
          position={menu.position}
          title={target.label}
          actions={[
            {
              id: "details",
              label: "Bilgiler",
              run: () => setSelected(target.id),
            },
            ...actions(target),
          ].map((a) => ({ ...a, disabled: busy || a.disabled }))}
          onClose={() => setMenu(null)}
          onError={onError}
        />
      )}
    </section>
  );
}
