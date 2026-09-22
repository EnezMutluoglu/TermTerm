import { describe, it, expect } from "vitest";
import { shortcutFor, shortcutLabel } from "../src/shortcuts";
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
  it("supports PuTTY clipboard keys without modifiers leaking into other actions", () => {
    expect(shortcutFor(key("Insert"), true, false)).toBe("copy");
    expect(shortcutFor(key("Insert", {ctrlKey: false, shiftKey: true}), true, false)).toBe("paste");
    expect(shortcutFor(key("Insert", {shiftKey: true}), true, false)).toBeUndefined();
    expect(shortcutLabel("paste", false)).toContain("Shift + Insert");
  });
  it("cycles the top tabs and leaves editing fields to their native shortcuts", () => {
    expect(shortcutFor(key("Tab"), true, false)).toBe("tabNext");
    expect(shortcutFor(key("Tab", {shiftKey: true}), false, false, true)).toBe("tabPrevious");
    for (const code of ["KeyK", "KeyN", "KeyC", "KeyV", "KeyW"]) {
      expect(shortcutFor(key(code), false, false, true)).toBeUndefined();
      expect(shortcutFor(key(code, {shiftKey: true}), false, false, true)).toBeUndefined();
    }
  });
  it("closes and scrolls only terminals and keeps Command-W platform-specific", () => {
    expect(shortcutFor(key("KeyW", {shiftKey: true}), true, false)).toBe("close");
    expect(shortcutFor(key("KeyW", {shiftKey: true}), false, false)).toBeUndefined();
    expect(shortcutFor(key("KeyW", {ctrlKey:false, metaKey:true}), true, true)).toBe("close");
    expect(shortcutFor(key("PageUp", {ctrlKey:false, shiftKey:true}), true, false)).toBe("scrollUp");
  });
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
