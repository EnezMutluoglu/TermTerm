import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { desktop } from "./api";

// Serialize writes so an older selection cannot overwrite a more recent one.
let writes: Promise<void> = Promise.resolve();
export function copyText(text: string) {
  if (!text) return Promise.resolve();
  const next = writes.catch(() => {}).then(() => desktop ? writeText(text) : navigator.clipboard.writeText(text));
  writes = next;
  return next;
}
export async function readClipboard() {
  await writes.catch(() => {});
  return desktop ? readText() : navigator.clipboard.readText();
}
export async function pasteTextField(target: HTMLInputElement | HTMLTextAreaElement) {
  const text = await readClipboard();
  if (!target.isConnected || document.activeElement !== target || target.readOnly || target.disabled) return;
  if (document.execCommand("insertText", false, text)) return;
  const start = target.selectionStart ?? target.value.length;
  const end = target.selectionEnd ?? start;
  const setter = Object.getOwnPropertyDescriptor(target instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value")!.set!;
  setter.call(target, target.value.slice(0, start) + text + target.value.slice(end));
  target.setSelectionRange(start + text.length, start + text.length);
  target.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertFromPaste", data: text }));
}
