import en from "./locales/en.json";
// Stable message IDs keep persisted records independent from display language.
// Additional locale dictionaries can be supplied without changing the vault format.
type Message = keyof typeof en;
const dictionaries: Record<string, Partial<Record<Message, string>>> = { en };
let locale = "en";
export function registerLocale(
  name: string,
  messages: Partial<Record<Message, string>>,
) {
  dictionaries[name] = messages;
}
export function setLocale(name: string) {
  locale = dictionaries[name] ? name : "en";
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
