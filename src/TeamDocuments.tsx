import { useEffect, useRef, useState, type MouseEvent } from "react";
import { Folder, FileCode2, PanelsTopLeft, Plus, Search } from "lucide-react";
import { call, errorText } from "./api";

export type TeamDocument =
  | { type: "snippet"; command: string }
  | {
      type: "workspace";
      layout: "tabs" | "grid";
      entries: {
        hostId: string;
        startupCommand: string;
        statsEnabled: boolean;
      }[];
    };
export type TeamContent = {
  kind: "host" | "folder" | "snippet" | "workspace";
  parentId: string | null;
  name: string;
  address: string | null;
  protocol: string | null;
  port: number | null;
  usernamePolicy: string | null;
  username: string | null;
  deleted: boolean;
  proxy?: { kind: "http" | "socks5"; host: string; port: number; authenticate: boolean } | null;
  document?: TeamDocument;
};
export type TeamResource = {
  id: string;
  revision: number;
  localGeneration?: number;
  content?: TeamContent;
  permissions?: string[];
  unavailable?: boolean;
  navigationOnly?: boolean;
};
export type DocumentRequest = {
  requestId: string;
  documentId: string;
  targetId: string | null;
  expectedRevision: number;
  localGeneration: number | null;
};
export default function TeamDocuments({
  rows,
  vault,
  owner,
  parent,
  onFolder,
  onSaved,
  onStart,
  onHistory,
  onManage,
}: {
  rows: TeamResource[];
  vault: string | null;
  owner: boolean;
  parent: string | null;
  onFolder: (id: string | null) => void;
  onSaved: () => Promise<void>;
  onStart: (r: DocumentRequest) => void;
  onHistory: (r: TeamResource) => void;
  onManage: (e: MouseEvent, r: TeamResource) => void;
}) {
  const [search, setSearch] = useState(""),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false),
    [editing, setEditing] = useState<TeamResource | null>(null),
    [draft, setDraft] = useState<TeamContent | null>(null);
  const [selected, setSelected] = useState<TeamResource | null>(null),
    [target, setTarget] = useState("");
  const [menu, setMenu] = useState<{
      x: number;
      y: number;
      row: TeamResource;
    } | null>(null),
    [deleting, setDeleting] = useState(false);
  const gate = useRef(false),
    live = useRef(true),
    draftId = useRef(crypto.randomUUID());
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  useEffect(() => {
    setMenu(null);
    setSelected(null);
    setDraft(null);
    setSearch("");
  }, [vault, parent]);
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menu]);
  const hosts = rows.filter(
    (r) =>
      r.content?.kind === "host" &&
      r.permissions?.includes("connect") &&
      ["ssh", "mosh"].includes(r.content.protocol ?? ""),
  );
  const canCreate =
    !!vault &&
    (owner || rows.find((r) => r.id === parent)?.permissions?.includes("edit"));
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
  function create(kind: "snippet" | "workspace") {
    draftId.current = crypto.randomUUID();
    setEditing(null);
    setDeleting(false);
    setDraft({
      kind,
      parentId: parent,
      name: "",
      address: null,
      protocol: null,
      port: null,
      username: null,
      usernamePolicy: null,
      deleted: false,
      document:
        kind === "snippet"
          ? { type: "snippet", command: "" }
          : { type: "workspace", layout: "tabs", entries: [] },
    });
  }
  async function save(content: TeamContent) {
    const receipt = await call<{ status: string }>("team_v2_action", {
      action: {
        kind: "saveResource",
        vaultId: vault,
        id: editing?.id ?? draftId.current,
        expectedRevision: editing?.revision ?? 0,
        localGeneration: editing?.localGeneration ?? null,
        clientAt: new Date().toISOString(),
        content,
      },
    });
    if (!live.current) return;
    if (!["applied", "queued", "conflict"].includes(receipt.status))
      throw Error("İşlem sonucu doğrulanamadı; kaydı yenileyin.");
    setNotice(
      receipt.status === "queued"
        ? "Şifreli yerel günlüğe alındı; eşitleme bekleniyor."
        : receipt.status === "conflict"
          ? "Çakışma Bekleyen kararlar ekranına gönderildi."
          : "Kayıt kaydedildi.",
    );
    setDraft(null);
    setSelected(null);
    setDeleting(false);
    await onSaved();
  }
  const items = rows.filter(
    (r) =>
      r.content &&
      !r.content.deleted &&
      r.content.parentId === parent &&
      ["folder", "snippet", "workspace"].includes(r.content.kind) &&
      r.content.name
        .toLocaleLowerCase("tr")
        .includes(search.toLocaleLowerCase("tr")),
  );
  return (
    <section className="ts-documents" aria-label="Komutlar ve çalışma alanları">
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      {!vault ? (
        <p>Önce bir ortak kasa seçin.</p>
      ) : (
        <>
          <div className="ts-actions">
            <button onClick={() => onFolder(null)}>Kasa kökü</button>
            {parent && (
              <button
                onClick={() =>
                  onFolder(
                    rows.find((r) => r.id === parent)?.content?.parentId ??
                      null,
                  )
                }
              >
                Üst klasör
              </button>
            )}
            <span>
              {rows.find((r) => r.id === parent)?.content?.name ?? "Kasa kökü"}
            </span>
            {canCreate && (
              <>
                <button disabled={busy} onClick={() => create("snippet")}>
                  <Plus size={15} />
                  Komut paketi
                </button>
                <button disabled={busy} onClick={() => create("workspace")}>
                  <Plus size={15} />
                  Çalışma alanı
                </button>
              </>
            )}
          </div>
          <label className="search-field">
            <Search size={16} />
            <input
              aria-label="Komut veya çalışma alanı ara"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Bu klasörde ara…"
            />
          </label>
          <div className="ts-document-list">
            {items.map((r) => (
              <button
                key={r.id}
                className={selected?.id === r.id ? "active" : ""}
                onClick={() => {
                  if (r.content!.kind === "folder") onFolder(r.id);
                  else {
                    setSelected(r);
                    setTarget("");
                  }
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setMenu({
                    x: Math.min(e.clientX, window.innerWidth - 240),
                    y: Math.min(e.clientY, window.innerHeight - 280),
                    row: r,
                  });
                }}
              >
                {r.content!.kind === "folder" ? (
                  <Folder size={18} />
                ) : r.content!.kind === "snippet" ? (
                  <FileCode2 size={18} />
                ) : (
                  <PanelsTopLeft size={18} />
                )}
                <span>{r.content!.name}</span>
                <small>
                  {r.content!.kind === "folder"
                    ? "Klasör"
                    : r.content!.kind === "snippet"
                      ? "Komut paketi"
                      : "Çalışma alanı"}
                </small>
              </button>
            ))}
          </div>
          {!items.length && <p>Bu klasörde kayıt yok.</p>}
          {selected?.content?.document && (
            <article className="ts-document-details">
              <h2>{selected.content.name}</h2>
              {selected.content.document.type === "snippet" ? (
                <>
                  <pre>{selected.content.document.command}</pre>
                  <label>
                    Hedef host
                    <select
                      aria-label="Komut hedefi"
                      value={target}
                      onChange={(e) => setTarget(e.target.value)}
                    >
                      <option value="">Host seçin</option>
                      {hosts.map((h) => (
                        <option key={h.id} value={h.id}>
                          {h.content!.name}
                        </option>
                      ))}
                    </select>
                  </label>
                </>
              ) : (
                <ul>
                  {selected.content.document.entries.map((e) => (
                    <li key={e.hostId}>
                      {hosts.find((h) => h.id === e.hostId)?.content?.name ??
                        "Hedef erişimi doğrulanacak"}
                      <pre>{e.startupCommand || "Başlangıç komutu yok"}</pre>
                    </li>
                  ))}
                </ul>
              )}
              <p>
                Yeni terminal açılır. Her hedefin güncel izni doğrulanır; komut
                yalnız bağlantı kurulduktan sonra gönderilir.
              </p>
              <button
                disabled={
                  busy ||
                  (selected.content.document.type === "snippet" && !target)
                }
                onClick={() =>
                  onStart({
                    requestId: crypto.randomUUID(),
                    documentId: selected.id,
                    expectedRevision: selected.revision,
                    localGeneration: selected.localGeneration ?? null,
                    targetId:
                      selected.content!.document!.type === "snippet"
                        ? target
                        : null,
                  })
                }
              >
                Önizlenen bağlantıları aç
              </button>
            </article>
          )}
        </>
      )}
      {menu && (
        <div
          className="ts-context"
          role="menu"
          style={{ left: menu.x, top: menu.y }}
        >
          {menu.row.content?.document &&
            menu.row.permissions?.includes("edit") && (
              <>
                <button
                  role="menuitem"
                  onClick={() => {
                    setEditing(menu.row);
                    setDraft(structuredClone(menu.row.content!));
                    setDeleting(false);
                  }}
                >
                  Düzenle
                </button>
                <button
                  role="menuitem"
                  onClick={() => {
                    setEditing(menu.row);
                    setDraft(structuredClone(menu.row.content!));
                    setDeleting(true);
                  }}
                >
                  Sil
                </button>
              </>
            )}
          {menu.row.permissions?.includes("read") && (
            <button role="menuitem" onClick={() => onHistory(menu.row)}>
              Kayıt geçmişi
            </button>
          )}
          <button
            role="menuitem"
            onClick={(e) => {
              e.stopPropagation();
              onManage(e, menu.row);
              setMenu(null);
            }}
          >
            İzinler ve taşıma
          </button>
        </div>
      )}
      {draft && (
        <div className="ts-overlay">
          <form
            className="ts-dialog ts-document-editor"
            role="dialog"
            aria-modal="true"
            aria-label={deleting ? "Kaydı sil" : "Team belge düzenle"}
            onSubmit={(e) => {
              e.preventDefault();
              void run(() => save({ ...draft, deleted: deleting }));
            }}
          >
            <h2>
              {deleting
                ? "Kaydı sil"
                : draft.kind === "snippet"
                  ? "Komut paketi"
                  : "Çalışma alanı"}
            </h2>
            {deleting ? (
              <p>{draft.name} silinecek. Geçmiş sürümler korunur.</p>
            ) : (
              <>
                <label>
                  Ad
                  <input
                    required
                    autoFocus
                    value={draft.name}
                    onChange={(e) =>
                      setDraft({ ...draft, name: e.target.value })
                    }
                  />
                </label>
                {draft.document?.type === "snippet" && (
                  <label>
                    Komut
                    <textarea
                      aria-label="Komut metni"
                      required
                      spellCheck={false}
                      value={draft.document.command}
                      onChange={(e) =>
                        setDraft({
                          ...draft,
                          document: {
                            type: "snippet",
                            command: e.target.value,
                          },
                        })
                      }
                    />
                  </label>
                )}
                {draft.document?.type === "workspace" && (
                  <>
                    <label>
                      Görünüm
                      <select
                        aria-label="Çalışma alanı görünümü"
                        value={draft.document.layout}
                        onChange={(e) =>
                          setDraft({
                            ...draft,
                            document: {
                              ...(draft.document as Extract<
                                TeamDocument,
                                { type: "workspace" }
                              >),
                              layout: e.target.value as "tabs" | "grid",
                            },
                          })
                        }
                      >
                        <option value="tabs">Sekmeler</option>
                        <option value="grid">Bölünmüş görünüm</option>
                      </select>
                    </label>
                    <label>
                      Host ekle
                      <select
                        aria-label="Çalışma alanına host ekle"
                        value=""
                        onChange={(e) => {
                          if (
                            e.target.value &&
                            draft.document?.type === "workspace"
                          )
                            setDraft({
                              ...draft,
                              document: {
                                ...draft.document,
                                entries: [
                                  ...draft.document.entries,
                                  {
                                    hostId: e.target.value,
                                    startupCommand: "",
                                    statsEnabled: true,
                                  },
                                ],
                              },
                            });
                        }}
                      >
                        <option value="">Host seçin (en fazla 16)</option>
                        {hosts
                          .filter(
                            (h) =>
                              draft.document?.type === "workspace" &&
                              draft.document.entries.length < 16 &&
                              !draft.document.entries.some(
                                (e) => e.hostId === h.id,
                              ),
                          )
                          .map((h) => (
                            <option key={h.id} value={h.id}>
                              {h.content!.name}
                            </option>
                          ))}
                      </select>
                    </label>
                    {draft.document.entries.map((entry, index) => (
                      <fieldset key={entry.hostId}>
                        <legend>
                          {index + 1}.{" "}
                          {hosts.find((h) => h.id === entry.hostId)?.content
                            ?.name ?? "Hedef erişimi doğrulanacak"}
                        </legend>
                        <label>
                          Başlangıç komutu
                          <textarea
                            spellCheck={false}
                            value={entry.startupCommand}
                            onChange={(e) => {
                              if (draft.document?.type === "workspace")
                                setDraft({
                                  ...draft,
                                  document: {
                                    ...draft.document,
                                    entries: draft.document.entries.map(
                                      (v, i) =>
                                        i === index
                                          ? {
                                              ...v,
                                              startupCommand: e.target.value,
                                            }
                                          : v,
                                    ),
                                  },
                                });
                            }}
                          />
                        </label>
                        <label>
                          <input
                            type="checkbox"
                            checked={entry.statsEnabled}
                            onChange={(e) => {
                              if (draft.document?.type === "workspace")
                                setDraft({
                                  ...draft,
                                  document: {
                                    ...draft.document,
                                    entries: draft.document.entries.map(
                                      (v, i) =>
                                        i === index
                                          ? {
                                              ...v,
                                              statsEnabled: e.target.checked,
                                            }
                                          : v,
                                    ),
                                  },
                                });
                            }}
                          />
                          Kaynak göstergesi
                        </label>
                        <button
                          type="button"
                          onClick={() => {
                            if (draft.document?.type === "workspace")
                              setDraft({
                                ...draft,
                                document: {
                                  ...draft.document,
                                  entries: draft.document.entries.filter(
                                    (_, i) => i !== index,
                                  ),
                                },
                              });
                          }}
                        >
                          Kaldır
                        </button>
                      </fieldset>
                    ))}
                  </>
                )}
              </>
            )}
            {error && <p role="alert">{error}</p>}
            <div className="ts-actions">
              <button
                disabled={
                  busy ||
                  (!deleting &&
                    draft.document?.type === "workspace" &&
                    !draft.document.entries.length)
                }
              >
                {busy
                  ? "Kaydediliyor…"
                  : deleting
                    ? "Silme işlemini onayla"
                    : "Kaydet"}
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => setDraft(null)}
              >
                İptal
              </button>
            </div>
          </form>
        </div>
      )}
    </section>
  );
}
