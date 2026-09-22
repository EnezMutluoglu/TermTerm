export const macPlatform = () => /mac/i.test(navigator.platform);
type Binding = { code: string; mod?: "primary" | "ctrl"; shift?: boolean; platform?: "mac" | "other" };
type Shortcut = { id: string; label: string; terminal: boolean; scope: "global" | "terminal" | "browse"; bindings: Binding[] };
const primary = (code: string, shift = true): Binding => ({ code, mod: "primary", shift });
export const shortcuts: Shortcut[] = [
  { id: "hosts", label: "Search hosts", terminal: false, scope: "browse", bindings: [primary("KeyK", false)] },
  { id: "newHost", label: "New host", terminal: false, scope: "browse", bindings: [primary("KeyN", false)] },
  { id: "local", label: "Local terminal", terminal: true, scope: "global", bindings: [primary("KeyT")] },
  { id: "lock", label: "Lock vault", terminal: true, scope: "global", bindings: [primary("KeyL")] },
  { id: "find", label: "Search terminal", terminal: true, scope: "terminal", bindings: [primary("KeyF")] },
  { id: "copy", label: "Copy terminal selection", terminal: true, scope: "terminal", bindings: [
    { ...primary("KeyC"), platform: "other" }, { ...primary("KeyC", false), platform: "mac" },
    { code: "Insert", mod: "ctrl", platform: "other" },
  ] },
  { id: "paste", label: "Paste into terminal", terminal: true, scope: "terminal", bindings: [
    { ...primary("KeyV"), platform: "other" }, { ...primary("KeyV", false), platform: "mac" },
    { code: "Insert", shift: true },
  ] },
  { id: "tabPrevious", label: "Previous top tab", terminal: true, scope: "global", bindings: [{ code: "Tab", mod: "ctrl", shift: true }] },
  { id: "tabNext", label: "Next top tab", terminal: true, scope: "global", bindings: [{ code: "Tab", mod: "ctrl" }] },
  { id: "previous", label: "Previous terminal / panel", terminal: true, scope: "global", bindings: [primary("BracketLeft")] },
  { id: "next", label: "Next terminal / panel", terminal: true, scope: "global", bindings: [primary("BracketRight")] },
  { id: "close", label: "Close active terminal", terminal: true, scope: "terminal", bindings: [
    { ...primary("KeyW"), platform: "other" }, { ...primary("KeyW", false), platform: "mac" },
  ] },
  { id: "broadcast", label: "Toggle broadcast", terminal: true, scope: "global", bindings: [primary("KeyB")] },
  { id: "scrollUp", label: "Scroll terminal history up", terminal: true, scope: "terminal", bindings: [{ code: "PageUp", shift: true }] },
  { id: "scrollDown", label: "Scroll terminal history down", terminal: true, scope: "terminal", bindings: [{ code: "PageDown", shift: true }] },
];
type KeyLike = Pick<KeyboardEvent, "code" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey" | "isComposing"> & { getModifierState?: (key: string) => boolean };
function matches(e: KeyLike, b: Binding, mac: boolean) {
  if (b.platform && (b.platform === "mac") !== mac) return false;
  const meta = b.mod === "primary" && mac;
  const ctrl = b.mod === "ctrl" || (b.mod === "primary" && !mac);
  return e.code === b.code && e.metaKey === meta && e.ctrlKey === ctrl && e.shiftKey === !!b.shift;
}
export function shortcutFor(e: KeyLike, inTerminal: boolean, mac = macPlatform(), editing = false) {
  if (e.isComposing || e.altKey || e.getModifierState?.("AltGraph")) return;
  return shortcuts.find(s =>
    (!inTerminal || s.terminal) && (s.scope !== "terminal" || inTerminal) &&
    (!editing || ["tabNext", "tabPrevious", "local", "lock"].includes(s.id)) &&
    s.bindings.some(b => matches(e, b, mac)),
  )?.id;
}
export function shortcutLabel(id: string, mac = macPlatform()) {
  const shortcut = shortcuts.find(s => s.id === id);
  return shortcut?.bindings.filter(b => !b.platform || (b.platform === "mac") === mac).map(b => [
    b.mod === "primary" ? (mac ? "⌘" : "Ctrl") : b.mod === "ctrl" ? "Ctrl" : "",
    b.shift ? "Shift" : "",
    b.code.replace("Key", "").replace("BracketLeft", "[").replace("BracketRight", "]"),
  ].filter(Boolean).join(" + ")).join(" / ") ?? "";
}
export function isTextEditing(target: EventTarget | null) {
  return target instanceof Element && !!target.closest('input, textarea, [contenteditable="true"]');
}
