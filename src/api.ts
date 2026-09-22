import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Session } from "./types";
import { tr } from "./i18n";
export const desktop = isTauri();
export async function call<T>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!desktop)
    throw new Error(
      tr(
        "Open the desktop application to use the encrypted vault and connections.",
      ),
    );
  return invoke<T>(command, args);
}
export const errorText = (e: unknown) =>
  tr(e instanceof Error ? e.message : String(e));
export async function chooseFile(
  extensions: string[],
  title = tr("Open file"),
): Promise<string | null> {
  const p = await open({
    title,
    multiple: false,
    filters: [{ name: tr("Supported files"), extensions }],
  });
  return typeof p === "string" ? p : null;
}
export async function saveFile(
  extension: string,
  defaultPath: string,
): Promise<string | null> {
  return save({
    defaultPath,
    filters: [{ name: `TermTerm ${extension}`, extensions: [extension] }],
  });
}
type SessionEvent = { id: string; kind: string; detail: any };
export const terminalHistory = new Map<string, Uint8Array[]>();
export const sessionState = new Map<string, Partial<Session>>();
const forgotten = new Set<string>();
export function forgetSession(id: string) {
  forgotten.add(id);
  for (const bytes of terminalHistory.get(id) ?? []) bytes.fill(0);
  terminalHistory.delete(id);
  sessionState.delete(id);
}
export function clearSessions() {
  for (const id of new Set([...terminalHistory.keys(), ...sessionState.keys()]))
    forgetSession(id);
}
const listeners = new Set<(event: SessionEvent) => void>();
let ready: Promise<void> | undefined;
export function prepareEvents() {
  return (ready ??= (async () => {
    if (!desktop) return;
    await listen<SessionEvent>("session-event", ({ payload: e }) => {
      if (forgotten.has(e.id)) {
        if (e.kind === "closed") forgotten.delete(e.id);
        return;
      }
      if (e.kind === "data") {
        const bytes = Uint8Array.from(atob(e.detail), (c) => c.charCodeAt(0));
        const history = terminalHistory.get(e.id) ?? [];
        history.push(bytes);
        if (history.length > 4096) history.shift();
        terminalHistory.set(e.id, history);
      }
      const patch =
        e.kind === "connected"
          ? { connected: true, status: "Connected" }
          : e.kind === "closed"
            ? { closed: true, connected: false }
            : e.kind === "error"
              ? { status: String(e.detail) }
              : e.kind === "status"
                ? { status: String(e.detail) }
                : {};
      sessionState.set(e.id, { ...sessionState.get(e.id), ...patch });
      listeners.forEach((l) => l(e));
    });
  })());
}
export function onSession(fn: (event: SessionEvent) => void) {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}
