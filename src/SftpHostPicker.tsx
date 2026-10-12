import { useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Folder,
  HardDrive,
  Search,
  Server,
  X,
} from "lucide-react";
import type { Entity } from "./types";
import { tr } from "./i18n";

export function sftpCatalog(records: Entity[]) {
  const groups = new Map(
    records.filter((r) => r.kind === "group").map((r) => [r.id, r]),
  );
  const hosts = records.filter(
    (r) =>
      r.kind === "host" &&
      (!r.data.protocol || ["ssh", "mosh"].includes(r.data.protocol)),
  );
  const parent = (r: Entity) =>
    groups.has(r.data.groupId) && r.data.groupId !== r.id
      ? (r.data.groupId as string)
      : "";
  const trail = (r: Entity) => {
    const result: Entity[] = [],
      seen = new Set([r.id]);
    let id = parent(r);
    while (id && !seen.has(id)) {
      seen.add(id);
      const g = groups.get(id)!;
      result.unshift(g);
      id = parent(g);
    }
    return result;
  };
  return { groups, hosts, parent, trail };
}

export default function SftpHostPicker({
  records,
  value,
  recent,
  label,
  onSelect,
  visible,
}: {
  records: Entity[];
  value: string;
  recent: string[];
  label: string;
  onSelect: (id: string) => void;
  visible: boolean;
}) {
  const [open, setOpen] = useState(false),
    [search, setSearch] = useState(""),
    [folder, setFolder] = useState(""),
    [limit, setLimit] = useState(100);
  const root = useRef<HTMLDivElement>(null),
    trigger = useRef<HTMLButtonElement>(null);
  const catalog = useMemo(() => sftpCatalog(records), [records]);
  const selected = catalog.hosts.find((r) => r.id === value);
  const query = search.trim().toLocaleLowerCase("tr");
  useEffect(() => {
    if (!visible) setOpen(false);
  }, [visible]);
  useEffect(() => {
    setLimit(100);
  }, [search, folder]);
  useEffect(() => {
    if (folder && !catalog.groups.has(folder)) setFolder("");
  }, [folder, catalog]);
  useEffect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  const ranked = useMemo(() => {
    const rank = new Map(recent.map((id, i) => [id, i]));
    return [...catalog.hosts].sort(
      (a, b) =>
        (rank.get(a.id) ?? Infinity) - (rank.get(b.id) ?? Infinity) ||
        String(a.data.label).localeCompare(String(b.data.label), "tr"),
    );
  }, [catalog, recent]);
  const matches = query
    ? ranked.filter((r) =>
        String(r.data.label ?? "")
          .toLocaleLowerCase("tr")
          .includes(query),
      )
    : ranked.filter((r) => catalog.parent(r) === folder);
  const children = [...catalog.groups.values()]
    .filter((g) => catalog.parent(g) === folder)
    .sort((a, b) =>
      String(a.data.label).localeCompare(String(b.data.label), "tr"),
    );
  const current = catalog.groups.get(folder);
  const trail = current ? [...catalog.trail(current), current] : [];
  const choose = (id: string) => {
    setOpen(false);
    trigger.current?.focus();
    onSelect(id);
  };
  const host = (r: Entity) => (
    <button
      type="button"
      className="sftp-host-choice"
      key={r.id}
      aria-label={tr("Connect to {name}", { name: r.data.label })}
      onClick={() => choose(r.id)}
    >
      <Server size={15} />
      <span>
        <strong>{r.data.label}</strong>
        <small>
          {catalog
            .trail(r)
            .map((g) => g.data.label)
            .join(" / ") || tr("Vault root")}
          {r.data.address ? ` · ${r.data.address}` : ""}
        </small>
      </span>
    </button>
  );
  return (
    <div
      className="sftp-host-picker"
      ref={root}
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          setOpen(false);
          trigger.current?.focus();
        }
      }}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node)) setOpen(false);
      }}
    >
      <button
        type="button"
        ref={trigger}
        className="sftp-host-trigger"
        aria-label={label}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          setOpen((v) => !v);
          setSearch("");
          setFolder("");
        }}
      >
        <span>
          {value === "local"
            ? tr("Local computer")
            : (selected?.data.label ?? tr("Select a connection…"))}
        </span>
        <ChevronDown size={15} />
      </button>
      {open && (
        <div
          className="sftp-host-popover"
          role="dialog"
          aria-label={tr("Choose SFTP host")}
        >
          <div className="sftp-host-search">
            <Search size={16} />
            <input
              autoFocus
              aria-label={tr("Search host names")}
              placeholder={tr("Search host names")}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <button
              type="button"
              aria-label={tr("Close")}
              onClick={() => {
                setOpen(false);
                trigger.current?.focus();
              }}
            >
              <X size={15} />
            </button>
          </div>
          <div className="sftp-host-browser">
            <nav aria-label={tr("Host folders")}>
              <button type="button" onClick={() => choose("local")}>
                <HardDrive size={15} />
                {tr("Local computer")}
              </button>
              <button
                type="button"
                onClick={() => {
                  setFolder("");
                  setSearch("");
                }}
              >
                <Folder size={15} />
                {tr("Vault root")}
              </button>
              {trail.map((g) => (
                <button
                  type="button"
                  key={g.id}
                  className={g.id === folder ? "active" : ""}
                  onClick={() => {
                    setFolder(g.id);
                    setSearch("");
                  }}
                >
                  <ChevronRight size={13} />
                  <span>{g.data.label}</span>
                </button>
              ))}
              {children.map((g) => (
                <button
                  type="button"
                  key={g.id}
                  onClick={() => {
                    setFolder(g.id);
                    setSearch("");
                  }}
                >
                  <Folder size={15} />
                  <span>{g.data.label}</span>
                  <ChevronRight size={13} />
                </button>
              ))}
            </nav>
            <div className="sftp-host-results">
              {!query &&
                !folder &&
                recent.some((id) => catalog.hosts.some((r) => r.id === id)) && (
                  <section aria-label={tr("Recent connections")}>
                    <h3>{tr("Recent connections")}</h3>
                    {ranked
                      .filter((r) => recent.includes(r.id))
                      .slice(0, 5)
                      .map(host)}
                  </section>
                )}
              <section aria-label={tr("Hosts in this folder")}>
                <h3>
                  {query
                    ? tr("Search results")
                    : (current?.data.label ?? tr("Hosts in this folder"))}
                </h3>
                {matches.slice(0, limit).map(host)}
                {!matches.length && <p>{tr("No matching hosts")}</p>}
                {matches.length > limit && (
                  <button
                    type="button"
                    onClick={() => setLimit((n) => n + 100)}
                  >
                    {tr("Show more")}
                  </button>
                )}
              </section>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
