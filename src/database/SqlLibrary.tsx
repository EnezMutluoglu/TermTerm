import { useEffect, useRef, useState } from "react";
import { call, errorText } from "../api";
import { Modal } from "../components";
import { newEntity, type Entity, type Vault } from "../types";
import { sqlAllowed } from "./teamPermissions";
import { documentData, type SqlStep } from "./sqlDocuments";

export default function SqlLibrary({
  vault,
  connection,
  teamMode,
  sql,
  canLoad,
  onVault,
  onLoad,
  onClose,
}: {
  vault: Vault;
  connection: Entity;
  teamMode: boolean;
  sql: string;
  canLoad: boolean;
  onVault: (v: Vault) => void;
  onLoad: (sql: string, label: string) => void;
  onClose: () => void;
}) {
  const [record, setRecord] = useState<Entity | null>(null);
  const [kind, setKind] = useState<"savedQuery" | "databaseRunbook">(
    "savedQuery",
  );
  const [label, setLabel] = useState("");
  const [description, setDescription] = useState("");
  const [groupId, setGroupId] = useState(connection.data.groupId ?? "");
  const [steps, setSteps] = useState<SqlStep[]>([
    { title: "Adım 1", sql, description: "" },
  ]);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [message, setMessage] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const live = useRef(true),
    running = useRef(false);
  useEffect(
    () => () => {
      live.current = false;
    },
    [],
  );
  const entries = vault.records.filter(
    (r) =>
      ["savedQuery", "databaseRunbook"].includes(r.kind) &&
      r.data.connectionId === connection.id,
  );
  const scope = record ?? vault.records.find((r) => r.id === groupId);
  const editable =
    !teamMode ||
    (scope
      ? sqlAllowed(true, scope, "edit")
      : !!vault.records
          .find((r) => r.id === connection.id)
          ?.data._teamPermissions?.includes("manage"));
  async function work(fn: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await fn();
    } catch (e) {
      if (live.current) setError(errorText(e));
    } finally {
      running.current = false;
      if (live.current) setBusy(false);
    }
  }
  function fresh() {
    setRecord(null);
    setLabel("");
    setDescription("");
    setKind("savedQuery");
    setGroupId(connection.data.groupId ?? "");
    setSteps([{ title: "Adım 1", sql, description: "" }]);
    setConfirmDelete(false);
    setMessage("");
  }
  async function load(id: string) {
    await work(async () => {
      const loaded = await call<Entity>("database_request", {
        request: { op: "libraryLoad", recordId: id },
      });
      if (!live.current) return;
      if (loaded.data.connectionId !== connection.id)
        throw Error("Belgenin bağlantısı değişti. Kitaplığı yeniden açın.");
      const cached = vault.records.find((r) => r.id === id);
      setRecord({
        ...loaded,
        data: {
          ...loaded.data,
          _teamPermissions: cached?.data._teamPermissions,
        },
      });
      setKind(loaded.kind as typeof kind);
      setLabel(loaded.data.label);
      setDescription(loaded.data.description ?? "");
      setGroupId(loaded.data.groupId ?? "");
      setConfirmDelete(false);
      setSteps(
        loaded.kind === "savedQuery"
          ? [
              {
                title: loaded.data.label,
                sql: loaded.data.sql,
                description: "",
              },
            ]
          : loaded.data.steps,
      );
    });
  }
  async function save() {
    await work(async () => {
      const data = documentData(
        kind,
        label,
        description,
        connection.id,
        groupId,
        steps,
      );
      const next = record
        ? {
            ...record,
            data: {
              ...data,
              ...(teamMode ? { _teamRevision: record.data._teamRevision } : {}),
            },
          }
        : newEntity(kind, data);
      const updated = await call<Vault>("records_save", { records: [next] });
      if (!live.current) return;
      onVault(updated);
      setRecord(updated.records.find((r) => r.id === next.id) ?? next);
      setMessage(
        teamMode
          ? "Şifreli yerel kuyruğa kaydedildi. Team eşitlemesiyle paylaşılacak."
          : "Şifreli kasaya kaydedildi.",
      );
    });
  }
  async function useStep(index: number) {
    await work(async () => {
      // Recheck the server even after the document was opened in this dialog.
      // Do not substitute unsaved text for a stored shared document silently.
      if (record) {
        const current = await call<Entity>("database_request", {
          request: { op: "libraryLoad", recordId: record.id },
        });
        if (current.data.connectionId !== connection.id)
          throw Error("Belgenin bağlantısı değişti.");
        const currentStep =
          current.kind === "savedQuery"
            ? { sql: current.data.sql, title: current.data.label }
            : current.data.steps[index];
        if (
          !currentStep ||
          currentStep.sql !== steps[index].sql ||
          currentStep.title !==
            (kind === "savedQuery" ? label : steps[index].title)
        )
          throw Error(
            "Belge değişti veya kaydedilmemiş düzenleme var. Kaydedip yeniden açın.",
          );
      }
      if (!live.current) return;
      onLoad(
        steps[index].sql,
        `${label || "Taslak"}${kind === "databaseRunbook" ? ` · ${index + 1}/${steps.length}: ${steps[index].title}` : ""}`,
      );
      onClose();
    });
  }
  function step(index: number, delta: Partial<SqlStep>) {
    setSteps((old) =>
      old.map((s, i) => (i === index ? { ...s, ...delta } : s)),
    );
  }
  return (
    <Modal title="SQL kitaplığı" onClose={busy ? () => {} : onClose} wide>
      <div className="db-library">
        <aside>
          <strong>{connection.data.label}</strong>
          <button disabled={busy} onClick={fresh}>
            Yeni belge
          </button>
          {entries.length === 0 && (
            <p>Bu bağlantıda henüz kayıtlı belge yok.</p>
          )}
          {entries.map((r) => (
            <button
              key={r.id}
              className={record?.id === r.id ? "active" : ""}
              disabled={busy}
              onClick={() => void load(r.id)}
            >
              {r.data.label}
              <small>
                {r.kind === "savedQuery" ? "Sorgu" : "Runbook"}
                {r.data._teamPending ? " · eşitleme bekliyor" : ""}
              </small>
            </button>
          ))}
        </aside>
        <section>
          <p className="db-library-note">
            Adımları tek tek editöre yükleyin ve gözden geçirip çalıştırın.
            Otomatik yürütme yok. Parametre değerleri ve sonuçlar bu belgeye
            kaydedilmez.
          </p>
          <fieldset disabled={busy || !editable}>
            <label>
              Belge türü
              <select
                aria-label="SQL belge türü"
                value={kind}
                disabled={!!record}
                onChange={(e) => {
                  setKind(e.target.value as typeof kind);
                  setSteps(steps.slice(0, 1));
                }}
              >
                <option value="savedQuery">Kayıtlı sorgu</option>
                <option value="databaseRunbook">
                  Runbook · sıralı adımlar
                </option>
              </select>
            </label>
            <label>
              Ad
              <input
                aria-label="SQL belge adı"
                maxLength={160}
                value={label}
                onChange={(e) => setLabel(e.target.value)}
              />
            </label>
            <label>
              Açıklama
              <textarea
                aria-label="SQL belge açıklaması"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
            </label>
            <label>
              Klasör
              <select
                aria-label="SQL belge klasörü"
                disabled={!!record}
                value={groupId}
                onChange={(e) => setGroupId(e.target.value)}
              >
                <option value="">Kasa kökü</option>
                {vault.records
                  .filter((r) => r.kind === "group" && !r.data._teamPathOnly)
                  .map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.data.label}
                    </option>
                  ))}
              </select>
            </label>
          </fieldset>
          {steps.map((s, i) => (
            <article key={i} className="db-library-step">
              {kind === "databaseRunbook" && (
                <div className="db-toolbar">
                  <strong>{i + 1}.</strong>
                  <input
                    aria-label={`Adım ${i + 1} başlığı`}
                    disabled={busy || !editable}
                    value={s.title}
                    maxLength={160}
                    onChange={(e) => step(i, { title: e.target.value })}
                  />
                  <button
                    title="Adımı yukarı taşı"
                    disabled={busy || !editable || i === 0}
                    onClick={() =>
                      setSteps((old) => {
                        const next = [...old];
                        [next[i - 1], next[i]] = [next[i], next[i - 1]];
                        return next;
                      })
                    }
                  >
                    ↑
                  </button>
                  <button
                    title="Adımı sil"
                    disabled={busy || !editable || steps.length === 1}
                    onClick={() =>
                      setSteps((old) => old.filter((_, j) => j !== i))
                    }
                  >
                    Sil
                  </button>
                </div>
              )}
              {kind === "databaseRunbook" && (
                <textarea
                  aria-label={`Adım ${i + 1} açıklaması`}
                  placeholder="Beklenen sonuç / dikkat edilecekler"
                  disabled={busy || !editable}
                  value={s.description}
                  onChange={(e) => step(i, { description: e.target.value })}
                />
              )}
              <textarea
                className="db-library-sql"
                aria-label={`Adım ${i + 1} SQL`}
                spellCheck={false}
                disabled={busy || !editable}
                value={s.sql}
                onChange={(e) => step(i, { sql: e.target.value })}
              />
              <button
                disabled={busy || !canLoad || !s.sql.trim()}
                title={canLoad ? "" : "Editöre yüklemek için bu bağlantının SQL sekmesini açın"}
                onClick={() => void useStep(i)}
              >
                Editöre yükle
                {kind === "databaseRunbook" ? ` · adım ${i + 1}` : ""}
              </button>
            </article>
          ))}
          {kind === "databaseRunbook" && (
            <button
              disabled={busy || !editable || steps.length >= 50}
              onClick={() =>
                setSteps((old) => [
                  ...old,
                  { title: `Adım ${old.length + 1}`, sql: "", description: "" },
                ])
              }
            >
              Adım ekle
            </button>
          )}
          {error && <p role="alert">{error}</p>}
          {message && <p role="status">{message}</p>}
          <footer className="db-toolbar">
            <button
              className="primary"
              disabled={busy || !editable}
              onClick={() => void save()}
            >
              {busy ? "İşleniyor…" : "Belgeyi kaydet"}
            </button>
            {record && (
              <button
                disabled={busy || !editable}
                onClick={() => setConfirmDelete(!confirmDelete)}
              >
                Belgeyi sil…
              </button>
            )}
            {confirmDelete && record && (
              <button
                disabled={busy || !editable}
                onClick={() =>
                  void work(async () => {
                    const v = await call<Vault>("records_delete", {
                      ids: [record.id],
                    });
                    if (live.current) {
                      onVault(v);
                      fresh();
                    }
                  })
                }
              >
                Silmeyi onayla
              </button>
            )}
          </footer>
        </section>
      </div>
    </Modal>
  );
}
