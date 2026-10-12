import ContextMenu, { menuPosition, type MenuPosition } from "./ContextMenu";
import { InputQueue } from "./inputQueue";
import { copyText } from "./clipboard";
import type { DocumentRequest } from "./TeamDocuments";
import { createPortal } from "react-dom";
import { HardDrive, ArrowLeftRight, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  call,
  errorText,
  forgetSession,
  onSession,
  prepareEvents,
  sessionState,
} from "./api";
import TerminalPane from "./TerminalPane";
import type { Prompt, Session } from "./types";
import { isTextEditing, shortcutFor, shortcutLabel } from "./shortcuts";
export type TeamConnectRequest = {
  requestId: string;
  hostId: string;
  label: string;
};
export default function TeamTerminals({
  request,
  documentRequest,
  onActive,
  tabsTarget,
  panelsTarget,
  workspace,
  navigation,
  onWorkspace,
  onLock,
}: {
  request: TeamConnectRequest | null;
  documentRequest: DocumentRequest | null;
  onActive: (active: boolean) => void;
  tabsTarget: HTMLElement | null;
  panelsTarget: HTMLElement | null;
  workspace: string;
  navigation: number;
  onWorkspace: (view: "vaults" | "sftp") => void;
  onLock: () => void;
}) {
  const live = useRef(true),
    seen = useRef(""),
    ids = useRef(new Set<string>());
  const [sessions, setSessions] = useState<Session[]>([]),
    [active, setActive] = useState<string | null>(null),
    [prompts, setPrompts] = useState<Prompt[]>([]),
    [answers, setAnswers] = useState<string[]>([]),
    [error, setError] = useState("");
  const [grid, setGrid] = useState(false);
  const [broadcast, setBroadcast] = useState(false);
  const [menu, setMenu] = useState<{
    id: string;
    position: MenuPosition;
  } | null>(null);
  const inputQueue = useRef(
    new InputQueue((id, data) => call("session_input", { id, data })),
  );
  const connectedCount = sessions.filter(
    (s) => s.connected && !s.closed,
  ).length;
  useEffect(() => {
    if (connectedCount < 2) setBroadcast(false);
  }, [connectedCount]);
  function sendInput(id: string, data: string) {
    const targets = broadcast
      ? sessions.filter((s) => s.connected && !s.closed).map((s) => s.id)
      : [id];
    for (const target of targets) {
      if (
        !live.current ||
        !ids.current.has(target) ||
        sessionState.get(target)?.closed
      )
        continue;
      inputQueue.current.send(target, data, (e) => {
        if (live.current) setError(errorText(e));
      });
    }
  }
  function showMenu(
    event: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>,
    id: string,
  ) {
    event.preventDefault();
    setMenu({ id, position: menuPosition(event) });
  }
  const docSeen = useRef(""),
    docBusy = useRef(false);
  const callback = useRef(onActive);
  callback.current = onActive;
  useEffect(() => {
    callback.current(active !== null);
  }, [active]);
  useEffect(() => {
    live.current = true;
    void prepareEvents();
    const un = onSession((e) => {
      if (e.kind === "closed") {
        inputQueue.current.cancel(e.id);
        setPrompts((p) => p.filter((v) => v.sessionId !== e.id));
      }
      if (ids.current.has(e.id)) {
        setSessions((s) =>
          s.map((row) =>
            row.id === e.id ? { ...row, ...sessionState.get(e.id) } : row,
          ),
        );
      }
    });
    const prompts = listen<Prompt>("session-prompt", ({ payload }) => {
      setPrompts((p) => [...p, payload]);
    });
    return () => {
      live.current = false;
      inputQueue.current.cancel();
      un();
      void prompts.then((f) => f());
      for (const id of ids.current) {
        void call("session_input", { id, close: true }).catch(() => {});
        forgetSession(id);
      }
      ids.current.clear();
    };
  }, []);
  useEffect(() => {
    setAnswers([]);
  }, [prompts[0]?.id]);
  useEffect(() => {
    if (!request || seen.current === request.requestId) return;
    seen.current = request.requestId;
    void (async () => {
      try {
        await prepareEvents();
        const id = await call<string>("team_v2_start", {
          hostId: request.hostId,
        });
        if (!live.current) {
          void call("session_input", { id, close: true });
          return;
        }
        ids.current.add(id);
        setSessions((s) => [
          ...s,
          {
            id,
            label: request.label,
            hostId: request.hostId,
            status: "Bağlanıyor",
            connected: false,
            closed: false,
            ...sessionState.get(id),
          },
        ]);
        setActive(id);
      } catch (e) {
        if (live.current) setError(errorText(e));
      }
    })();
  }, [request]);
  useEffect(() => {
    if (!documentRequest || docSeen.current === documentRequest.requestId)
      return;
    docSeen.current = documentRequest.requestId;
    if (docBusy.current) {
      setError("Çalışma alanı açılıyor; işlemin bitmesini bekleyin.");
      return;
    }
    docBusy.current = true;
    setError("");
    void (async () => {
      try {
        await prepareEvents();
        const result = await call<{
          layout: "tabs" | "grid";
          sessions: {
            id: string;
            hostId: string;
            label: string;
            statsEnabled: boolean;
          }[];
        }>("team_v2_document_start", {
          documentId: documentRequest.documentId,
          targetId: documentRequest.targetId,
          expectedRevision: documentRequest.expectedRevision,
          localGeneration: documentRequest.localGeneration,
        });
        if (!live.current) {
          for (const row of result.sessions)
            void call("session_input", { id: row.id, close: true });
          return;
        }
        for (const row of result.sessions) ids.current.add(row.id);
        setSessions((old) => [
          ...old,
          ...result.sessions.map((row) => ({
            ...row,
            status: "Bağlanıyor",
            connected: false,
            closed: false,
            ...sessionState.get(row.id),
          })),
        ]);
        setGrid(result.layout === "grid");
        setActive(result.sessions[0]?.id ?? null);
      } catch (e) {
        if (live.current) setError(errorText(e));
      } finally {
        docBusy.current = false;
      }
    })();
  }, [documentRequest]);
  useEffect(() => {
    setActive(null);
  }, [navigation]);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (
        e.defaultPrevented ||
        document.querySelector('[role="dialog"], [role="menu"]')
      )
        return;
      const editing =
        isTextEditing(e.target) && !(e.target as Element).closest(".xterm");
      const action = shortcutFor(e, !!active && !editing, undefined, editing);
      if (
        !action ||
        ![
          "tabNext",
          "tabPrevious",
          "previous",
          "next",
          "close",
          "broadcast",
          "lock",
        ].includes(action)
      )
        return;
      e.preventDefault();
      if (e.repeat) return;
      if (action === "lock") {
        inputQueue.current.cancel();
        onLock();
        return;
      }
      if (action === "broadcast") {
        if (active && connectedCount > 1) setBroadcast((v) => !v);
        return;
      }
      if (action === "close" && active) {
        close(active);
        return;
      }
      const terminalsOnly = action === "previous" || action === "next";
      const tabs = [
        ...(terminalsOnly ? [] : ["vaults", "sftp"]),
        ...sessions.map((s) => s.id),
      ];
      if (!tabs.length) return;
      const current = active ?? (workspace === "sftp" ? "sftp" : "vaults");
      const next =
        tabs[
          (tabs.indexOf(current) +
            (["previous", "tabPrevious"].includes(action) ? -1 : 1) +
            tabs.length) %
            tabs.length
        ];
      if (next === "vaults" || next === "sftp") {
        setActive(null);
        onWorkspace(next);
      } else setActive(next);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [active, sessions, workspace, onWorkspace, onLock, connectedCount]);
  useEffect(() => {
    tabsTarget
      ?.querySelector(".active")
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active, workspace, sessions.length, tabsTarget]);
  function close(id: string) {
    inputQueue.current.cancel(id);
    void call("session_input", { id, close: true }).catch(() => {});
    ids.current.delete(id);
    forgetSession(id);
    setSessions((s) => s.filter((v) => v.id !== id));
    if (active === id) {
      const index = sessions.findIndex((s) => s.id === id);
      const next = sessions[index + 1]?.id ?? sessions[index - 1]?.id ?? null;
      setActive(next);
      if (!next) onWorkspace("vaults");
    }
    setPrompts((p) => p.filter((v) => v.sessionId !== id));
  }
  async function answer(values: string[]) {
    const p = prompts[0];
    if (!p) return;
    setAnswers([]);
    setPrompts((s) => s.filter((v) => v.id !== p.id));
    await call("prompt_answer", { id: p.id, answers: values });
  }
  return (
    <div className="ts-terminals">
      {tabsTarget &&
        createPortal(
          <nav className="top-tabs" aria-label="Team terminal sekmeleri">
            <button
              className={!active && workspace !== "sftp" ? "active" : ""}
              onClick={() => {
                setActive(null);
                onWorkspace("vaults");
              }}
            >
              <HardDrive size={14} />
              Kasa
            </button>
            <button
              className={!active && workspace === "sftp" ? "active" : ""}
              onClick={() => {
                setActive(null);
                onWorkspace("sftp");
              }}
            >
              <ArrowLeftRight size={14} />
              SFTP
            </button>
            {sessions.map((s) => (
              <div
                key={s.id}
                onContextMenu={(e) => showMenu(e, s.id)}
                className={
                  "top-session-tab " + (active === s.id ? "active" : "")
                }
              >
                <button
                  className="session-tab-label"
                  title={s.label}
                  onClick={() => setActive(s.id)}
                  onKeyDown={(e) => {
                    if (
                      e.key === "ContextMenu" ||
                      (e.key === "F10" && e.shiftKey)
                    )
                      showMenu(e, s.id);
                  }}
                >
                  <span
                    className={"status-dot " + (s.connected ? "live" : "")}
                  />
                  <span>
                    {s.label}
                    {s.closed ? " · Kapandı" : ""}
                  </span>
                </button>
                <button
                  className="tab-close"
                  aria-label={`${s.label} kapat`}
                  onClick={() => close(s.id)}
                >
                  <X size={13} />
                </button>
              </div>
            ))}
          </nav>,
          tabsTarget,
        )}
      {menu && sessions.some((s) => s.id === menu.id) && (
        <ContextMenu
          position={menu.position}
          title={sessions.find((s) => s.id === menu.id)!.label}
          onClose={() => setMenu(null)}
          onError={(e) => setError(errorText(e))}
          actions={[
            {
              id: "focus",
              label: "Terminale odaklan",
              run: () => {
                setGrid(false);
                setActive(menu.id);
              },
            },
            {
              id: "split",
              label: "Bölünmüş görünüm",
              disabled: sessions.length < 2,
              run: () => {
                setGrid(true);
                setActive(menu.id);
              },
            },
            {
              id: "copy",
              label: "Sekme adını kopyala",
              run: () =>
                copyText(sessions.find((s) => s.id === menu.id)!.label),
            },
            {
              id: "close",
              label: "Terminali kapat",
              run: () => close(menu.id),
            },
          ]}
        />
      )}
      {error && <p role="alert">{error}</p>}
      {panelsTarget &&
        createPortal(
          <>
            {active && (
              <div
                className="ts-terminal-tools"
                role="toolbar"
                aria-label="Terminal araçları"
              >
                <button
                  aria-pressed={grid}
                  disabled={sessions.length < 2}
                  onClick={() => setGrid((v) => !v)}
                >
                  Bölünmüş görünüm
                </button>
                <button
                  aria-pressed={broadcast}
                  disabled={connectedCount < 2}
                  title={shortcutLabel("broadcast")}
                  onClick={() => setBroadcast((v) => !v)}
                >
                  Toplu giriş
                </button>
                {broadcast && (
                  <span role="status">
                    Toplu giriş açık: {connectedCount} terminale yazılıyor
                  </span>
                )}
              </div>
            )}
            <div
              className="ts-terminal-grid"
              style={{
                display: active ? "grid" : "none",
                ["--team-cols" as string]: grid
                  ? Math.ceil(Math.sqrt(sessions.length))
                  : 1,
              }}
            >
              {sessions.map((s) => (
                <div
                  className="ts-terminal-panel"
                  key={s.id}
                  style={{
                    display:
                      active && (grid || active === s.id) ? "flex" : "none",
                  }}
                >
                  <TerminalPane
                    session={s}
                    focused={active === s.id}
                    visible={!!active && (grid || active === s.id)}
                    onFocus={() => setActive(s.id)}
                    onClose={() => close(s.id)}
                    onInput={sendInput}
                    onStats={(enabled) =>
                      setSessions((rows) =>
                        rows.map((r) =>
                          r.id === s.id ? { ...r, statsEnabled: enabled } : r,
                        ),
                      )
                    }
                    onError={(e) => setError(errorText(e))}
                  />
                </div>
              ))}
            </div>
          </>,
          panelsTarget,
        )}
      {prompts[0] && (
        <div className="ts-overlay">
          <section
            className="ts-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="SSH doğrulaması"
          >
            <h2>
              {prompts[0].kind === "hostKey"
                ? "Sunucu kimliğini doğrula"
                : prompts[0].detail.name || "Bağlantı bilgileri"}
            </h2>
            {prompts[0].kind === "hostKey" ? (
              <>
                <p>{prompts[0].detail.address}</p>
                <code>{prompts[0].detail.fingerprint}</code>
                <p>
                  Bu parmak izini güvenilir bir kaynaktan doğrula. Kabul edilen
                  sunucu anahtarı kişisel şifreli yerel depoda saklanır.
                </p>
              </>
            ) : (
              <>
                <p>{prompts[0].detail.instructions}</p>
                {prompts[0].detail.prompts?.map((p, i) => (
                  <label key={i}>
                    {p.prompt}
                    <input
                      type={p.echo ? "text" : "password"}
                      autoComplete="off"
                      value={answers[i] ?? ""}
                      onChange={(e) =>
                        setAnswers((a) => {
                          const b = [...a];
                          b[i] = e.target.value;
                          return b;
                        })
                      }
                    />
                  </label>
                ))}
              </>
            )}
            <div className="ts-actions">
              <button
                onClick={() =>
                  void answer(
                    prompts[0].kind === "hostKey"
                      ? ["trust"]
                      : (prompts[0].detail.prompts ?? []).map(
                          (_, i) => answers[i] ?? "",
                        ),
                  )
                }
              >
                {prompts[0].kind === "hostKey"
                  ? "Parmak izine güven"
                  : "Gönder"}
              </button>
              <button onClick={() => void answer([])}>İptal</button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
