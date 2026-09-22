export const macPlatform = () => /mac/i.test(navigator.platform);
export const shortcuts = [
  {
    id: "hosts",
    label: "Search hosts",
    code: "KeyK",
    shift: false,
    terminal: false,
  },
  {
    id: "newHost",
    label: "New host",
    code: "KeyN",
    shift: false,
    terminal: false,
  },
  {
    id: "local",
    label: "Local terminal",
    code: "KeyT",
    shift: true,
    terminal: true,
  },
  {
    id: "lock",
    label: "Lock vault",
    code: "KeyL",
    shift: true,
    terminal: true,
  },
  {
    id: "find",
    label: "Search terminal",
    code: "KeyF",
    shift: true,
    terminal: true,
  },
  {
    id: "copy",
    label: "Copy terminal selection",
    code: "KeyC",
    shift: true,
    terminal: true,
  },
  {
    id: "paste",
    label: "Paste into terminal",
    code: "KeyV",
    shift: true,
    terminal: true,
  },
  {
    id: "previous",
    label: "Previous terminal",
    code: "BracketLeft",
    shift: true,
    terminal: true,
  },
  {
    id: "next",
    label: "Next terminal",
    code: "BracketRight",
    shift: true,
    terminal: true,
  },
  {
    id: "broadcast",
    label: "Toggle broadcast",
    code: "KeyB",
    shift: true,
    terminal: true,
  },
] as const;
type KeyLike = Pick<
  KeyboardEvent,
  "code" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey" | "isComposing"
> & { getModifierState?: (key: string) => boolean };
export function shortcutFor(
  e: KeyLike,
  inTerminal: boolean,
  mac = macPlatform(),
) {
  if (e.isComposing || e.altKey || e.getModifierState?.("AltGraph")) return;
  if (!(mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey)) return;
  return shortcuts.find(
    (s) =>
      s.code === e.code &&
      (mac && (s.id === "copy" || s.id === "paste") ? false : s.shift) ===
        e.shiftKey &&
      (!inTerminal || s.terminal),
  )?.id;
}
export function shortcutLabel(id: string) {
  const s = shortcuts.find((s) => s.id === id),
    mac = macPlatform();
  return s
    ? `${mac ? "⌘" : "Ctrl"}${s.shift && !(mac && (id === "copy" || id === "paste")) ? " + Shift" : ""} + ${s.code.replace("Key", "").replace("BracketLeft", "[").replace("BracketRight", "]")}`
    : "";
}
