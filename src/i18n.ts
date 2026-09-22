import en from "./locales/en.json";
import turkish from "./locales/tr.json";
// Stable message IDs keep persisted records independent from display language.
// Additional locale dictionaries can be supplied without changing the vault format.
type Message = keyof typeof en;
const dictionaries: Record<string, Partial<Record<Message, string>>> = {
  en,
  tr: Object.fromEntries(
    Object.entries(en).map(([key, value]) => [
      key,
      (turkish as Record<string, string>)[value] ?? value,
    ]),
  ),
};
let locale = "tr";
try {
  if (localStorage.getItem("termterm.locale") === "en") locale = "en";
} catch {
  /* System default remains Turkish when storage is unavailable. */
}
if (typeof document !== "undefined") document.documentElement.lang = locale;
export function registerLocale(
  name: string,
  messages: Partial<Record<Message, string>>,
) {
  dictionaries[name] = messages;
}
export function setLocale(name: string) {
  locale = dictionaries[name] ? name : "tr";
  if (typeof document !== "undefined") document.documentElement.lang = locale;
}
const entityLabels: Record<string, string> = {
  host: "Host",
  group: "Group",
  credential: "Identity",
  snippet: "Snippet",
  workspace: "Workspace",
  tunnel: "Port forwarding",
  knownHost: "Known host",
  log: "Session log",
  settings: "Settings",
  syncProfile: "PostgreSQL sync",
  integration: "Integration",
};
export const entityLabel = (kind: string) => tr(entityLabels[kind] ?? kind);
// Translate only application-owned messages, never user names, terminal output,
// commands, file paths or persisted protocol identifiers.
export function tr(
  message: string,
  parameters: Record<string, string | number> = {},
) {
  const translated =
    locale === "tr"
      ? ((turkish as Record<string, string>)[message] ?? message)
      : message;
  return translated.replace(/\{(\w+)\}/g, (_, key) =>
    String(parameters[key] ?? `{${key}}`),
  );
}
export function t(
  key: Message,
  parameters: Record<string, string | number> = {},
) {
  return (dictionaries[locale][key] ?? en[key]).replace(
    /\{(\w+)\}/g,
    (_, key) => String(parameters[key] ?? `{${key}}`),
  );
}
