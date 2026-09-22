import { tr } from "./i18n";
import { useState } from "react";
import { call, errorText } from "./api";
import { Field, Select, Busy } from "./components";
import {
  newEntity,
  type Vault,
  type ImportPreview,
  type ImportApplyResult,
} from "./types";
export default function Integrations({
  vault,
  onVault,
}: {
  vault: Vault;
  onVault: (v: Vault) => void;
}) {
  const saved = vault.records.find(
    (r) => r.kind === "integration" && r.data.provider === "digitalocean",
  );
  const [provider, setProvider] = useState("aws");
  const [profile, setProfile] = useState("default");
  const [region, setRegion] = useState("eu-central-1");
  const [token, setToken] = useState(saved?.data.token ?? "");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [bridge, setBridge] = useState<{ url: string; token: string } | null>(
    null,
  );
  async function run(fn: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <h2>{tr("Integrations")}</h2>
      <div className="settings-card">
        <h3>{tr("Cloud host discovery")}</h3>
        <p className="muted">
          {tr(
            "List your existing cloud instances, review them, then import selected connection details.",
          )}
        </p>
        <Select
          label={tr("Provider")}
          value={provider}
          onChange={(v) => {
            setProvider(v);
            setPreview(null);
          }}
          options={[
            { value: "aws", label: "Amazon EC2 (AWS CLI v2)" },
            { value: "digitalocean", label: "DigitalOcean Droplets" },
          ]}
        />
        {provider === "aws" ? (
          <>
            <Field
              label={tr("Configured AWS CLI profile")}
              value={profile}
              onChange={setProfile}
            />
            <Field label={tr("Region")} value={region} onChange={setRegion} />
          </>
        ) : (
          <>
            <Field
              label={tr("Read-only DigitalOcean API token")}
              value={token}
              onChange={setToken}
              type="password"
            />
            <button
              className="text-btn"
              onClick={() =>
                void run(async () =>
                  onVault(
                    await call<Vault>("records_save", {
                      records: [
                        {
                          ...(saved ?? newEntity("integration")),
                          data: { label: "DigitalOcean", provider, token },
                        },
                      ],
                    }),
                  ),
                )
              }
            >
              {tr("Save token in encrypted vault")}
            </button>
          </>
        )}
        <button
          className="primary"
          disabled={busy}
          onClick={() =>
            void run(async () =>
              setPreview(
                await call<ImportPreview>("cloud_discover", {
                  provider,
                  profile,
                  region,
                  token,
                }),
              ),
            )
          }
        >
          {tr("Discover hosts")}
        </button>
        {preview && (
          <>
            <div className="import-preview">
              {preview.records.map((r) => (
                <div key={r.id}>
                  <strong>{r.data.label}</strong>
                  <span>{r.data.address}</span>
                </div>
              ))}
            </div>
            <p className="muted small">{preview.warnings.join(" ")}</p>
            <button
              className="secondary"
              disabled={!preview.records.length || busy}
              onClick={() =>
                void run(async () => {
                  onVault(
                    (
                      await call<ImportApplyResult>("import_apply", {
                        records: preview.records,
                        policy: "skip",
                        vaultId: vault.id,
                      })
                    ).vault,
                  );
                  setPreview(null);
                })
              }
            >
              {tr("Import discovered hosts")}
            </button>
          </>
        )}
      </div>
      <div className="settings-card">
        <h3>{tr("Localhost API Bridge")}</h3>
        <p className="muted">
          {tr(
            "Allow your scripts to list and update host connection details. The bridge never returns passwords or private keys and does not execute commands.",
          )}
        </p>
        <p className="muted small">
          {tr(
            "A new token and local port are generated each time. Locking the vault stops the bridge.",
          )}
        </p>
        {bridge && (
          <>
            <Field
              label={tr("Base URL")}
              value={bridge.url}
              onChange={() => {}}
              readOnly
            />
            <Field
              label={tr("Bearer token")}
              value={bridge.token}
              readOnly
              type="password"
              onChange={() => {}}
            />
            <button
              className="text-btn"
              onClick={() =>
                void navigator.clipboard
                  .writeText(bridge.token)
                  .catch((e) => setError(errorText(e)))
              }
            >
              {tr("Copy bearer token")}
            </button>
            <pre className="export-preview">
              GET /v1/hosts{"\n"}POST /v1/hosts{"\n"}Authorization: Bearer
              YOUR_TOKEN{"\n"}
              {"\n"}
              {
                '{"label":"web-01","address":"10.0.0.1","port":22,"username":"ubuntu"}'
              }
            </pre>
          </>
        )}
        <div className="button-row">
          <button
            className="secondary"
            disabled={busy}
            onClick={() =>
              void run(async () =>
                setBridge(await call("api_bridge", { enabled: true })),
              )
            }
          >
            {bridge
              ? tr("Restart with new token")
              : tr("Start / restart bridge")}
          </button>
          <button
            className="secondary"
            onClick={() =>
              void run(async () => {
                await call("api_bridge", { enabled: false });
                setBridge(null);
              })
            }
          >
            {tr("Stop bridge")}
          </button>
        </div>
      </div>
      {busy && <Busy />}
      {error && <div className="notice error">{error}</div>}
    </>
  );
}
