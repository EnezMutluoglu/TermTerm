import { describe, it, expect } from "vitest";
import { shortcutFor } from "../src/shortcuts";
const key = (code: string, extra: Record<string, boolean> = {}) => ({
  code,
  ctrlKey: true,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  isComposing: false,
  ...extra,
});
describe("application versus terminal keyboard ownership", () => {
  it("leaves Ctrl+K/N, function keys and terminal Control combinations to xterm", () => {
    for (const code of ["KeyK", "KeyN", "KeyC", "KeyD", "F1", "F12", "ArrowUp"])
      expect(shortcutFor(key(code), true, false)).toBeUndefined();
    expect(shortcutFor(key("KeyK"), false, false)).toBe("hosts");
  });
  it("uses Command for macOS without stealing shell Control", () => {
    expect(shortcutFor(key("KeyK"), false, true)).toBeUndefined();
    expect(
      shortcutFor(key("KeyK", { ctrlKey: false, metaKey: true }), false, true),
    ).toBe("hosts");
    expect(
      shortcutFor(key("KeyC", { ctrlKey: false, metaKey: true }), true, true),
    ).toBe("copy");
  });
  it("preserves AltGr, composition and non-shortcut international keys", () => {
    expect(
      shortcutFor(key("KeyK", { altKey: true }), false, false),
    ).toBeUndefined();
    expect(
      shortcutFor(
        { ...key("KeyK"), getModifierState: (s: string) => s === "AltGraph" },
        false,
        false,
      ),
    ).toBeUndefined();
    expect(
      shortcutFor(key("KeyK", { isComposing: true }), false, false),
    ).toBeUndefined();
    expect(shortcutFor(key("IntlBackslash"), true, false)).toBeUndefined();
  });
  it("reserves only explicit application actions inside a terminal", () => {
    expect(shortcutFor(key("KeyV", { shiftKey: true }), true, false)).toBe(
      "paste",
    );
    expect(
      shortcutFor(key("BracketRight", { shiftKey: true }), true, false),
    ).toBe("next");
    expect(shortcutFor(key("KeyB", { shiftKey: true }), true, false)).toBe(
      "broadcast",
    );
  });
});
