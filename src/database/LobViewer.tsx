import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { call, errorText } from "../api";
import type { RowValues } from "./rowEditing";
export interface FullValue {
  base64: string;
  bytes: number;
  sha256: string;
  binary: boolean;
  isNull: boolean;
  encoding: string;
  observedAt: number;
}
export function valuePreview(value: FullValue) {
  const raw = atob(value.base64.slice(0, Math.ceil(65536 / 3) * 4));
  const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
  return value.binary
    ? Array.from(bytes.slice(0, 256), (b) =>
        b.toString(16).padStart(2, "0"),
      ).join(" ")
    : new TextDecoder("utf-8").decode(bytes, {
        stream: value.bytes > bytes.length,
      });
}
export default function LobViewer({
  sessionId,
  schema,
  table,
  column,
  keys,
  onClose,
}: {
  sessionId: string;
  schema: string;
  table: string;
  column: string;
  keys: RowValues;
  onClose: () => void;
}) {
  const [value, setValue] = useState<FullValue>(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [saved, setSaved] = useState(false);
  const live = useRef(true),
    pending = useRef(false);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  async function load() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setValue(undefined);
    setSaved(false);
    try {
      const r = await call<FullValue>("database_request", {
        request: { op: "lobRead", sessionId, schema, table, column, keys },
      });
      if (live.current) setValue(r);
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (live.current) setBusy(false);
    }
  }
  async function download() {
    if (!value || value.isNull || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const extension = value.binary ? "bin" : "txt";
      const path = await save({
        defaultPath:
          column.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_") + "." + extension,
        filters: [
          {
            name: value.binary ? "Binary" : "UTF-8 metin",
            extensions: [extension],
          },
        ],
      });
      if (!path || !live.current) return;
      await call("database_file", {
        action: "writeBytes",
        sessionId,
        path,
        content: value.base64,
      });
      if (live.current) setSaved(true);
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      pending.current = false;
      if (live.current) setBusy(false);
    }
  }
  return (
    <section className="db-lob-viewer" aria-label="Tam değer">
      <header>
        <h3>{column} · Tam değer</h3>
        <button onClick={onClose}>Kapat</button>
      </header>
      <p>
        İçerik yalnız isteğinizle yeniden okunur; tablonun ilk gösterildiği
        andan farklı olabilir. En fazla 8 MiB, ayrı bağlantıda salt okunur. Kasa
        ve geçmişe kaydedilmez.
      </p>
      <button className="primary" disabled={busy} onClick={() => void load()}>
        {busy ? "İşlem sürüyor…" : "Tam değeri yükle"}
      </button>
      {error && <p role="alert">{error}</p>}
      {value && (
        <>
          <p>
            {value.isNull
              ? "NULL · dosya içeriği yok"
              : `${value.bytes.toLocaleString("tr")} bayt · ${value.encoding} · tam içerik yüklendi`}
          </p>
          {!value.isNull && (
            <>
              <small>
                Önizleme {value.binary ? "ilk 256 bayt (hex)" : "ilk 64 KiB"};
                dosyaya yüklü içeriğin tamamı yazılır.
              </small>
              <pre>{valuePreview(value)}</pre>
              <p className="db-value-hash">SHA-256: {value.sha256}</p>
              <button disabled={busy} onClick={() => void download()}>
                Dosyaya kaydet
              </button>
              {saved && <span> Dosya kaydedildi.</span>}
            </>
          )}
        </>
      )}
    </section>
  );
}
