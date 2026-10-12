import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import {
  Folder,
  Server,
  Search,
  LayoutGrid,
  List,
  MoreHorizontal,
} from "lucide-react";
export type TeamHostRow = {
  id: string;
  revision: number;
  navigationOnly?: boolean;
  permissions?: string[];
  content?: {
    kind: "host" | "folder" | "snippet" | "workspace";
    parentId: string | null;
    name: string;
    address: string | null;
    port: number | null;
    protocol: string | null;
    username: string | null;
    usernamePolicy: string | null;
    deleted: boolean;
  };
  unavailable?: boolean;
};
export default function TeamHosts({
  rows,
  parent,
  selected,
  busy,
  onFolder,
  onSelect,
  onConnect,
  onMenu,
}: {
  rows: TeamHostRow[];
  parent: string | null;
  selected: string | null;
  busy: boolean;
  onFolder: (id: string) => void;
  onSelect: (row: TeamHostRow) => void;
  onConnect: (row: TeamHostRow) => void;
  onMenu: (event: MouseEvent, row: TeamHostRow) => void;
}) {
  const [search, setSearch] = useState(""),
    [view, setView] = useState<"grid" | "list">("grid");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const clear = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => {
    setSearch("");
    clear();
    return clear;
  }, [parent]);
  const counts = useMemo(() => {
    const byId = new Map(rows.map((r) => [r.id, r]));
    const counts = new Map<string, number>();
    for (const row of rows)
      if (row.content?.kind === "host") {
        let parent = row.content.parentId;
        const seen = new Set<string>();
        while (parent && !seen.has(parent)) {
          seen.add(parent);
          counts.set(parent, (counts.get(parent) ?? 0) + 1);
          parent = byId.get(parent)?.content?.parentId ?? null;
        }
      }
    return counts;
  }, [rows]);
  const visible = rows.filter(
    (r) =>
      r.content?.parentId === parent &&
      [r.content.name, r.content.address, r.content.username].some((v) =>
        v?.toLocaleLowerCase("tr").includes(search.toLocaleLowerCase("tr")),
      ),
  );
  const folders = visible.filter((r) => r.content?.kind === "folder"),
    hosts = visible.filter((r) => r.content?.kind === "host");
  return (
    <div className="team-hosts">
      <div className="records-toolbar">
        <div className="search-field">
          <Search size={17} />
          <input
            aria-label="Bu klasörde ara"
            placeholder="Bu klasörde ara…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="toolbar-right">
          <span>
            {folders.length} klasör · {hosts.length} host
          </span>
          <div className="view-toggle">
            <button
              className={view === "grid" ? "active" : ""}
              aria-label="Kart görünümü"
              onClick={() => setView("grid")}
            >
              <LayoutGrid size={16} />
            </button>
            <button
              className={view === "list" ? "active" : ""}
              aria-label="Liste görünümü"
              onClick={() => setView("list")}
            >
              <List size={17} />
            </button>
          </div>
        </div>
      </div>
      {folders.length > 0 && (
        <div className="groups-grid">
          {folders.map((row) => (
            <div
              key={row.id}
              className="group-card"
              onContextMenu={(e) => onMenu(e, row)}
            >
              <button
                className="group-main"
                disabled={busy}
                onClick={() => {
                  clear();
                  onFolder(row.id);
                }}
              >
                <div className="folder-square">
                  <Folder size={23} />
                </div>
                <div>
                  <strong>{row.content!.name}</strong>
                  <small>
                    {row.navigationOnly
                      ? "Gezinme klasörü"
                      : `${counts.get(row.id) ?? 0} erişilebilir host · ${rows.filter((r) => r.content?.kind === "folder" && r.content.parentId === row.id).length} klasör`}
                  </small>
                </div>
              </button>
              <button
                className="icon-btn"
                aria-label={`${row.content!.name} işlemleri`}
                onClick={(e) => onMenu(e, row)}
              >
                <MoreHorizontal size={16} />
              </button>
            </div>
          ))}
        </div>
      )}
      <div className={"records-grid " + (view === "list" ? "list" : "")}>
        {hosts.map((row) => (
          <article
            role="button"
            tabIndex={0}
            aria-label={`${row.content!.name} ${row.content!.address}`}
            aria-disabled={busy}
            key={row.id}
            className={"record-card " + (selected === row.id ? "selected" : "")}
            onContextMenu={(e) => onMenu(e, row)}
            onClick={() => {
              if (busy) return;
              clear();
              timer.current = setTimeout(() => onSelect(row), 500);
            }}
            onDoubleClick={() => {
              clear();
              if (!busy) onConnect(row);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !busy) {
                e.preventDefault();
                clear();
                onSelect(row);
              }
              if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
                e.preventDefault();
                onMenu(e as unknown as MouseEvent, row);
              }
            }}
          >
            <div className="card-top">
              <div className="host-icon">
                <Server size={21} />
              </div>
              <div className="card-title">
                <h3>{row.content!.name}</h3>
                <span>
                  {row.content!.usernamePolicy === "shared_required"
                    ? row.content!.username + "@"
                    : ""}
                  {row.content!.address}
                </span>
              </div>
            </div>
            <div className="card-tags" />
            <footer>
              <span className="protocol-label">
                {row.content!.protocol?.toUpperCase()} · {row.content!.port}
              </span>
              <span className="tag">Team</span>
            </footer>
          </article>
        ))}
      </div>
      {!visible.length && (
        <div className="empty">
          <Server size={36} />
          <h3>
            {search ? "Eşleşen kayıt yok" : "Bu klasörde erişilebilir host yok"}
          </h3>
        </div>
      )}
    </div>
  );
}
