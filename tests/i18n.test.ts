import { afterEach, expect, test } from "vitest";
import { entityLabel, setLocale, t, tr } from "../src/i18n";
import turkish from "../src/locales/tr.json";

afterEach(() => setLocale("tr"));
test("Turkish catalog preserves interpolation fields", () => {
  const fields = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map(m => m[1]).sort();
  for (const [english, translated] of Object.entries(turkish)) {
    expect(translated.trim(), english).not.toBe("");
    expect(fields(translated), english).toEqual(fields(english));
  }
});
test("default Turkish, English fallback and entity labels do not translate interpolation data", () => {
  setLocale("tr");
  expect(t("nav.hosts")).toBe("Sunucular");
  expect(entityLabel("host")).toBe("Sunucu");
  expect(tr("{name} saved.", { name: "Production" })).toBe("Production kaydedildi.");
  expect(tr("unknown diagnostic")).toBe("unknown diagnostic");
  setLocale("en");
  expect(t("nav.hosts")).toBe("Hosts");
  expect(tr("Remember password using {store}", { store: "DPAPI" })).toBe("Remember password using DPAPI");
});
