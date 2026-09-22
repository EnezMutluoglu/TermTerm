import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  FolderOpen,
  FileArchive,
  ArrowRight,
  ShieldCheck,
  Download,
  Upload,
  CheckCircle2,
  AlertTriangle,
} from "lucide-react";
import { Modal, Field, Select, Check, Busy } from "./components";
import { call, chooseFile, saveFile, errorText, desktop } from "./api";
import type { Vault, ImportPreview, ImportApplyResult, Entity } from "./types";
export type DataMode = "import" | "export" | "backup" | "restore" | "portable";
export default function DataTools({
  mode,
  vault,
  onClose,
  onVault,
  onNotice,
  recordIds = [],
}: {
  mode: DataMode;
  vault: Vault | null;
  onClose: () => void;
  onVault: (v: Vault) => void;
  onNotice: (s: string) => void;
  recordIds?: string[];
}) {
  const [format, setFormat] = useState(mode === "import" ? "auto" : "openssh");
  const [encoding, setEncoding] = useState("auto");
  const [path, setPath] = useState("");
  const [password, setPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [profiles, setProfiles] = useState(false);
  const [secrets, setSecrets] = useState(false);
  const [policy, setPolicy] = useState("copy");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [exportData, setExportData] = useState<{
    text: string;
    warnings: string[];
  } | null>(null);
  const [backup, setBackup] = useState<{
    vaults: { name: string; records: Entity[] }[];
  } | null>(null);
  const [index, setIndex] = useState("0");
  const [includeFiles, setIncludeFiles] = useState(false);
  const [sources, setSources] = useState<{ path: string; password: string }[]>(
    [],
  );
  const [mapping, setMapping] = useState<Record<string, string> | null>(null);
  const [backupReport, setBackupReport] = useState<string[] | null>(null);
  const working = useRef(false);
  const activeOperation = useRef<string | null>(null);
  const mounted = useRef(true);
  const [stage, setStage] = useState("");
  const [page, setPage] = useState(0);
  const items = useMemo(
    () => new Map(preview?.items?.map((i) => [i.recordId, i])),
    [preview],
  );
  useEffect(() => {
    mounted.current = true;
    const events = desktop
      ? listen<{ id: string; stage: string }>(
          "operation-event",
          ({ payload }) => {
            if (payload.id === activeOperation.current) setStage(payload.stage);
          },
        )
      : Promise.resolve(() => {});
    return () => {
      mounted.current = false;
      void events.then((stop) => stop());
      if (activeOperation.current)
        void call("operation_cancel", { id: activeOperation.current }).catch(
          () => {},
        );
    };
  }, []);
  function changePath(value: string) {
    setPath(value);
    setMapping(null);
    setPreview(null);
    setBackup(null);
    setPage(0);
  }
  async function close() {
    if (!working.current) {
      onClose();
      return;
    }
    if (!activeOperation.current) {
      setStage("Finishing current operation…");
      return;
    }
    try {
      const cancelled = await call<boolean>("operation_cancel", {
        id: activeOperation.current,
      });
      setStage(
        cancelled ? "Cancellation requested…" : "Finishing atomic save…",
      );
    } catch (e) {
      setError(errorText(e));
    }
  }
  const titles = {
    import: "Import connections",
    export: "Export connections",
    backup: "Create encrypted backup",
    restore: "Restore backup",
    portable: "Save portable copy",
  };
  async function choose() {
    try {
      const p = await chooseFile(mode === "restore" ? ["ttbackup"] : ["*"]);
      if (p) {
        changePath(p);
      }
    } catch (e) {
      setError(errorText(e));
    }
  }
  async function action() {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setError("");
    setStage("");
    try {
      if (mode === "import") {
        activeOperation.current = crypto.randomUUID();
        if (!preview) {
          const result = await call<ImportPreview>("import_preview", {
            path,
            format,
            password,
            mapping,
            encoding,
            operationId: activeOperation.current,
          });
          if (mounted.current) {
            setPreview(result);
            setPage(0);
          }
        } else {
          const result = await call<ImportApplyResult>("import_apply", {
            records: preview.records,
            policy,
            vaultId: vault?.id,
            operationId: activeOperation.current,
          });
          if (mounted.current) {
            onVault(result.vault);
            onNotice(
              `Import complete: ${result.added} added, ${result.updated} updated, ${result.skipped} skipped, ${result.failed} failed.`,
            );
            onClose();
          }
        }
      }
      if (mode === "export") {
        if (!exportData) {
          setExportData(await call("export_preview", { format, secrets }));
        } else {
          const file = await saveFile(
            format === "csv" ? "csv" : "txt",
            format === "csv"
              ? "TermTerm-hosts.csv"
              : format === "known_hosts"
                ? "known_hosts.txt"
                : "ssh-config.txt",
          );
          if (file) {
            await call("export_write", { path: file, format, secrets });
            onNotice("Export saved.");
            onClose();
          }
        }
      }
      if (mode === "backup") {
        const file = await saveFile(
          "ttbackup",
          `${vault?.name ?? "TermTerm"}-${new Date().toISOString().slice(0, 10)}.ttbackup`,
        );
        if (file) {
          const report = await call<{
            warnings: string[];
            records: number;
            vaults: number;
          }>("backup_bundle", {
            path: file,
            password,
            includeProfiles: profiles,
            includeFiles,
            sources,
            ids: recordIds,
          });
          onNotice(
            report.vaults +
              " vaults, " +
              report.records +
              " records backed up.",
          );
          if (report.warnings.length) setBackupReport(report.warnings);
          else onClose();
        }
      }
      if (mode === "portable") {
        const file = await saveFile(
          "ttvault",
          `${vault?.name ?? "Vault"}-portable.ttvault`,
        );
        if (file) {
          await call("vault_copy", { path: file });
          onNotice(
            "Consistent portable copy saved. Use your existing vault password to open it.",
          );
          onClose();
        }
      }
      if (mode === "restore") {
        if (!backup) {
          setBackup(await call("backup_preview", { path, password }));
        } else {
          const target = await saveFile(
            "ttvault",
            `${backup.vaults[Number(index)].name}-restored.ttvault`,
          );
          if (target) {
            const v = await call<Vault>("backup_restore", {
              source: path,
              password,
              path: target,
              newPassword: newPassword || password,
              index: Number(index),
            });
            onVault(v);
            onNotice(
              "Restored into a new local vault. Sync profiles are inactive.",
            );
            onClose();
          }
        }
      }
    } catch (e) {
      if (mounted.current) setError(errorText(e));
    } finally {
      activeOperation.current = null;
      working.current = false;
      if (mounted.current) {
        setBusy(false);
        setStage("");
      }
    }
  }
  return (
    <Modal
      title={titles[mode]}
      onClose={() => void close()}
      wide={!!preview || !!backup || !!exportData}
    >
      <div className="modal-body" aria-busy={busy}>
        <fieldset
          disabled={busy}
          style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}
        >
          <div className="data-hero">
            <div className="data-icon">
              {mode === "import" ? (
                <Upload size={25} />
              ) : (
                <FileArchive size={25} />
              )}
            </div>
            <p>
              {mode === "import"
                ? "Bring your existing connections into this vault. Review every record before making changes."
                : mode === "backup"
                  ? "A password-protected backup of every portable record, including identities, host chains, snippets and stored logs."
                  : mode === "restore"
                    ? "Preview an encrypted backup, then restore it into a new local vault."
                    : mode === "export"
                      ? "Choose a compatible format. Use an encrypted backup to preserve every relationship and setting."
                      : "Create a consistent copy while the vault is open. The copy uses your existing vault password and a new device identity."}
            </p>
          </div>
          {mode === "import" && !preview && (
            <Select
              label="File format"
              value={format}
              onChange={setFormat}
              options={[
                { value: "auto", label: "Detect from file" },
                {
                  value: "ttvault",
                  label: "TermTerm encrypted vault (.ttvault)",
                },
                {
                  value: "ttbackup",
                  label: "TermTerm encrypted backup (.ttbackup)",
                },
                {
                  value: "openssh",
                  label: "OpenSSH config (including Termius exports)",
                },
                { value: "known_hosts", label: "OpenSSH known_hosts" },
                { value: "csv", label: "CSV host table" },
                { value: "putty", label: "PuTTY registry export (.reg)" },
                {
                  value: "mobaxterm",
                  label: "MobaXterm (.ini / .mobaconf / .mxtsessions)",
                },
                { value: "securecrt", label: "SecureCRT settings (.xml)" },
                { value: "ansible", label: "Ansible inventory (.ini / .yml)" },
              ]}
            />
          )}
          {mode === "import" && !preview && (
            <Select
              label="Text encoding"
              value={encoding}
              onChange={(value) => {
                setEncoding(value);
                setMapping(null);
              }}
              options={[
                { value: "auto", label: "Unicode (UTF-8 / UTF-16 BOM)" },
                { value: "windows-1254", label: "Turkish (Windows-1254)" },
                {
                  value: "windows-1252",
                  label: "Western European (Windows-1252)",
                },
                { value: "windows-1251", label: "Cyrillic (Windows-1251)" },
              ]}
            />
          )}
          {(mode === "import" || mode === "restore") && !preview && !backup && (
            <div className="file-field">
              <Field
                label="Source file"
                value={path}
                onChange={changePath}
                placeholder="Choose a file…"
              />
              <button className="secondary" onClick={() => void choose()}>
                <FolderOpen size={17} />
              </button>
            </div>
          )}
          {mode === "import" &&
            !preview &&
            ["ttvault", "ttbackup", "auto", "mobaxterm", "securecrt"].includes(
              format,
            ) && (
              <Field
                label="Source password (if encrypted)"
                value={password}
                onChange={setPassword}
                type="password"
                hint="Vendor encryption without a verified decoder is reported as unsupported; encrypted fields are never treated as plaintext credentials."
              />
            )}
          {mode === "import" && format === "csv" && !preview && (
            <>
              <button
                className="secondary"
                disabled={!path}
                onClick={async () => {
                  try {
                    const headers = await call<string[]>("csv_headers", {
                      path,
                      encoding,
                    });
                    setMapping(
                      Object.fromEntries(
                        headers.map((h) => [h, h.toLowerCase()]),
                      ),
                    );
                  } catch (e) {
                    setError(errorText(e));
                  }
                }}
              >
                Map CSV columns
              </button>
              {mapping &&
                Object.entries(mapping).map(([column, field]) => (
                  <Select
                    key={column}
                    label={column}
                    value={field}
                    onChange={(v) => setMapping((m) => ({ ...m, [column]: v }))}
                    options={[
                      { value: "", label: "Ignore" },
                      ...[
                        "label",
                        "address",
                        "port",
                        "username",
                        "password",
                        "protocol",
                        "group",
                        "tags",
                      ].map((v) => ({ value: v, label: v })),
                    ]}
                  />
                ))}
            </>
          )}
          {mode === "backup" && (
            <>
              {recordIds.length > 0 && (
                <p className="notice">
                  Selected export: {recordIds.length} records, plus their
                  required references.
                </p>
              )}
              <Field
                label="Backup password"
                value={password}
                onChange={setPassword}
                type="password"
                hint="At least 8 characters. This can differ from your vault password."
              />
              <Check checked={profiles} onChange={setProfiles}>
                Include PostgreSQL profiles and integration credentials
              </Check>
              <Check checked={includeFiles} onChange={setIncludeFiles}>
                Include referenced SSH key and certificate files
              </Check>
              <button
                className="secondary"
                disabled={recordIds.length > 0}
                onClick={async () => {
                  const path = await chooseFile(
                    ["ttvault"],
                    "Add another closed vault",
                  );
                  if (path) setSources((s) => [...s, { path, password: "" }]);
                }}
              >
                Add another vault to backup
              </button>
              {sources.map((s, i) => (
                <div key={s.path}>
                  <p className="muted small">{s.path}</p>
                  <Field
                    label="Additional vault password"
                    type="password"
                    value={s.password}
                    onChange={(password) =>
                      setSources((a) =>
                        a.map((s, n) => (i === n ? { ...s, password } : s)),
                      )
                    }
                  />
                </div>
              ))}
              <div className="panel-tip">
                <ShieldCheck size={17} />
                Hardware private keys stay on their device. Missing referenced
                files are listed in the backup report.
              </div>
            </>
          )}
          {mode === "restore" && !backup && (
            <Field
              label="Backup password"
              value={password}
              onChange={setPassword}
              type="password"
            />
          )}
          {backup && (
            <>
              <div className="success-line">
                <CheckCircle2 size={18} />
                Backup verified and decrypted
              </div>
              <Select
                label="Vault to restore"
                value={index}
                onChange={setIndex}
                options={backup.vaults.map((v, i) => ({
                  value: String(i),
                  label: `${v.name} · ${v.records.length} records`,
                }))}
              />
              <div className="preview-counts">
                {Object.entries(
                  backup.vaults[Number(index)].records.reduce<
                    Record<string, number>
                  >((a, r) => {
                    a[r.kind] = (a[r.kind] ?? 0) + 1;
                    return a;
                  }, {}),
                ).map(([k, n]) => (
                  <span key={k}>
                    <strong>{n}</strong>
                    {k}
                  </span>
                ))}
              </div>
              <Field
                label="New vault password (optional)"
                value={newPassword}
                onChange={setNewPassword}
                type="password"
                placeholder="Use backup password"
              />
              <div className="panel-tip">
                Restore creates a new vault and device identity. Remote profiles
                will not connect automatically.
              </div>
            </>
          )}
          {preview && (
            <>
              <div className="preview-summary">
                <strong>{preview.records.length} records ready</strong>
                <span>{preview.warnings.length} notes to review</span>
              </div>
              <div className="import-preview">
                {preview.records
                  .slice(page * 100, (page + 1) * 100)
                  .map((r) => (
                    <div key={r.id}>
                      <span className="pill">{r.kind}</span>
                      <strong>{r.data.label ?? r.data.address}</strong>
                      <span>{r.data.address ?? ""}</span>
                      <span
                        className="pill"
                        title={items.get(r.id)?.notes.join("\n")}
                      >
                        {items.get(r.id)?.status ?? "ready"}
                      </span>
                    </div>
                  ))}
              </div>
              {preview.records.length > 100 && (
                <div className="button-row">
                  <button
                    className="secondary"
                    disabled={!page}
                    onClick={() => setPage((p) => p - 1)}
                  >
                    Previous
                  </button>
                  <span>
                    Page {page + 1} / {Math.ceil(preview.records.length / 100)}
                  </span>
                  <button
                    className="secondary"
                    disabled={(page + 1) * 100 >= preview.records.length}
                    onClick={() => setPage((p) => p + 1)}
                  >
                    Next
                  </button>
                </div>
              )}
              <Select
                label="Duplicate records"
                value={policy}
                onChange={setPolicy}
                options={[
                  {
                    value: "copy",
                    label: "Create new copies (preserve existing records)",
                  },
                  { value: "skip", label: "Skip matching records" },
                  { value: "update", label: "Update matching records" },
                ]}
              />
              {!!preview.warnings.length && (
                <details open className="warnings">
                  <summary>
                    <AlertTriangle size={15} />
                    Import report
                  </summary>
                  <ul>
                    {preview.warnings.map((w, i) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                </details>
              )}
            </>
          )}
          {mode === "export" && !exportData && (
            <>
              <Select
                label="Export format"
                value={format}
                onChange={setFormat}
                options={[
                  { value: "openssh", label: "OpenSSH config" },
                  { value: "csv", label: "CSV host table" },
                  { value: "known_hosts", label: "OpenSSH known_hosts" },
                ]}
              />
              {format === "csv" && (
                <Check checked={secrets} onChange={setSecrets}>
                  Include passwords in plain text
                </Check>
              )}
            </>
          )}
          {exportData && (
            <>
              <div className="panel-tip">
                Review the exported data before saving the file.
              </div>
              <pre className="export-preview">{exportData.text}</pre>
              {!!exportData.warnings.length && (
                <details open className="warnings">
                  <summary>Fields not represented in this format</summary>
                  <ul>
                    {exportData.warnings.map((w, i) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                </details>
              )}
            </>
          )}
          {backupReport && (
            <div className="notice">
              <strong>Backup saved — review these notes</strong>
              <ul>
                {backupReport.map((w, i) => (
                  <li key={i}>{w}</li>
                ))}
              </ul>
            </div>
          )}
        </fieldset>
        {stage && (
          <div className="notice" role="status">
            {stage}
          </div>
        )}
        {error && <div className="notice error">{error}</div>}
      </div>
      <footer>
        <button
          className="secondary"
          onClick={() => void close()}
          disabled={busy && mode !== "import"}
        >
          {busy ? "Cancel operation" : "Close"}
        </button>
        <button
          className="primary"
          disabled={
            busy || (mode === "import" && !!preview && !preview.records.length)
          }
          onClick={() => void action()}
        >
          {busy ? (
            <Busy />
          ) : (
            <>
              {mode === "import"
                ? preview
                  ? "Import records"
                  : "Preview import"
                : mode === "restore"
                  ? backup
                    ? "Restore to new vault"
                    : "Preview backup"
                  : mode === "export"
                    ? exportData
                      ? "Save export"
                      : "Preview export"
                    : "Save file"}
              <ArrowRight size={16} />
            </>
          )}
        </button>
      </footer>
    </Modal>
  );
}
