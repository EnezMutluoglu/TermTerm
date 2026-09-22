import { useState } from "react";
import {
  X,
  Server,
  KeyRound,
  Folder,
  ArrowUp,
  ArrowDown,
  Plus,
  Trash2,
  Shield,
  Link,
  Terminal,
  FileKey,
  Download,
  Save,
} from "lucide-react";
import { Field, Select, TextArea, Check, Busy } from "./components";
import { call, chooseFile, errorText } from "./api";
import type { Entity } from "./types";
const titles: Record<string, string> = {
  host: "Host",
  group: "Group",
  credential: "Identity",
  snippet: "Snippet",
  workspace: "Workspace",
  tunnel: "Port forwarding",
  knownHost: "Known host",
  log: "Session log",
  settings: "Settings",
  integration: "Integration",
};
export default function Editor({
  record,
  records,
  onClose,
  onSave,
  onDelete,
  onConnect,
}: {
  record: Entity;
  records: Entity[];
  onClose: () => void;
  onSave: (r: Entity) => Promise<void>;
  onDelete: (id: string) => void;
  onConnect: (id: string) => void;
}) {
  const [data, setData] = useState(structuredClone(record.data));
  const [tab, setTab] = useState("general");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [jump, setJump] = useState("");
  const [agentKeys, setAgentKeys] = useState<any[]>([]);
  const [serialPorts, setSerialPorts] = useState<string[]>([]);
  const [installHost, setInstallHost] = useState("");
  const update = (k: string, v: any) => setData((d) => ({ ...d, [k]: v }));
  const groups = records.filter(
    (r) => r.kind === "group" && r.id !== record.id,
  );
  const hosts = records.filter((r) => r.kind === "host" && r.id !== record.id);
  const credentials = records.filter((r) => r.kind === "credential");
  const isHost = record.kind === "host",
    isGroup = record.kind === "group",
    isConnection = isHost || isGroup;
  async function save(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      if (!String(data.label ?? "").trim())
        throw new Error("A label is required.");
      if (isHost && !data.address) throw new Error("An address is required.");
      if (isConnection && data.groupId) {
        let current = data.groupId;
        const seen = new Set([record.id]);
        while (current) {
          if (seen.has(current))
            throw new Error("Groups cannot contain a cycle.");
          seen.add(current);
          current = records.find((r) => r.id === current)?.data.groupId;
        }
      }
      await onSave({ ...record, data });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  const changeProxy = (k: string, v: any) =>
    update("proxy", {
      ...(data.proxy ?? { kind: "socks5", port: 1080 }),
      [k]: v,
    });
  return (
    <aside className="editor">
      <form onSubmit={save}>
        <header className="editor-header">
          <div>
            <span className="eyebrow">
              {records.some((r) => r.id === record.id) ? "EDIT" : "NEW"}{" "}
              {titles[record.kind]?.toUpperCase()}
            </span>
            <h2>{data.label || `New ${titles[record.kind]?.toLowerCase()}`}</h2>
          </div>
          <button
            type="button"
            className="icon-btn"
            aria-label="Close editor"
            onClick={onClose}
          >
            <X size={19} />
          </button>
        </header>
        {isConnection && (
          <div className="editor-tabs">
            {["general", "connection", "advanced"].map((t) => (
              <button
                type="button"
                key={t}
                className={tab === t ? "active" : ""}
                onClick={() => setTab(t)}
              >
                {t[0].toUpperCase() + t.slice(1)}
              </button>
            ))}
          </div>
        )}
        <div className="editor-body">
          {(tab === "general" || !isConnection) && (
            <>
              <div className="record-symbol">
                {record.kind === "credential" ? (
                  <KeyRound size={28} />
                ) : isGroup ? (
                  <Folder size={28} />
                ) : record.kind === "snippet" ? (
                  <Terminal size={28} />
                ) : (
                  <Server size={28} />
                )}
              </div>
              <Field
                label="Label"
                value={data.label}
                onChange={(v) => update("label", v)}
                placeholder={
                  isHost ? "e.g. Production web server" : "Give it a name"
                }
                required
              />
              {isConnection && (
                <>
                  <Select
                    label="Parent group"
                    value={data.groupId ?? ""}
                    onChange={(v) => update("groupId", v)}
                    options={[
                      { value: "", label: "No group" },
                      ...groups.map((r) => ({
                        value: r.id,
                        label: r.data.label,
                      })),
                    ]}
                  />
                  {isHost && (
                    <Field
                      label="Address"
                      value={data.address}
                      onChange={(v) => update("address", v)}
                      placeholder="Hostname, IP address or device path"
                      required
                    />
                  )}
                  <Field
                    label="Tags"
                    value={(data.tags ?? []).join(", ")}
                    onChange={(v) =>
                      update(
                        "tags",
                        v
                          .split(",")
                          .map((s: string) => s.trim())
                          .filter(Boolean),
                      )
                    }
                    placeholder="production, web, europe"
                  />
                  <TextArea
                    label="Notes"
                    value={data.notes}
                    onChange={(v) => update("notes", v)}
                    rows={3}
                    placeholder="Anything useful to remember"
                  />
                  <div className="panel-tip">
                    <Shield size={16} />
                    <span>Connection details are encrypted in your vault.</span>
                  </div>
                </>
              )}
              {record.kind === "credential" && (
                <>
                  <Field
                    label="Username"
                    value={data.username}
                    onChange={(v) => update("username", v)}
                  />
                  <Field
                    label="Password"
                    value={data.password}
                    onChange={(v) => update("password", v)}
                    type="password"
                  />
                  <div className="section-title">
                    <FileKey size={15} /> SSH key
                  </div>
                  <div className="button-row">
                    <button
                      type="button"
                      className="secondary"
                      onClick={async () => {
                        setBusy(true);
                        try {
                          const key =
                            await call<Record<string, any>>("key_generate");
                          setData((d) => ({ ...d, ...key }));
                        } catch (e) {
                          setError(errorText(e));
                        } finally {
                          setBusy(false);
                        }
                      }}
                    >
                      Generate Ed25519
                    </button>
                    <button
                      type="button"
                      className="secondary"
                      onClick={async () => {
                        try {
                          const path = await chooseFile(
                            ["*"],
                            "Import OpenSSH, PEM or PuTTY key",
                          );
                          if (path) {
                            const key = await call<Record<string, any>>(
                              "key_import",
                              { path, passphrase: data.passphrase ?? "" },
                            );
                            setData((d) => ({ ...d, ...key }));
                          }
                        } catch (e) {
                          setError(errorText(e));
                        }
                      }}
                    >
                      <Download size={14} />
                      Import key
                    </button>
                  </div>
                  <Field
                    label="Key passphrase"
                    value={data.passphrase}
                    onChange={(v) => update("passphrase", v)}
                    type="password"
                    hint="Enter this before importing an encrypted key."
                  />
                  <TextArea
                    label="Private key"
                    value={data.privateKey}
                    onChange={(v) => update("privateKey", v)}
                    rows={6}
                    placeholder="Paste an OpenSSH, PEM or PPK private key"
                  />
                  <TextArea
                    label="SSH certificate"
                    value={data.certificate}
                    onChange={(v) => update("certificate", v)}
                    rows={3}
                  />
                  {data.fingerprint && (
                    <div className="fingerprint">{data.fingerprint}</div>
                  )}
                  {data.publicKey && (
                    <TextArea
                      label="Public key"
                      value={data.publicKey}
                      onChange={(v) => update("publicKey", v)}
                      rows={3}
                    />
                  )}
                  <div className="section-title">Signing agent</div>
                  <Select
                    label="External agent"
                    value={data.agent ?? ""}
                    onChange={(v) => update("agent", v)}
                    options={[
                      { value: "", label: "Use stored key / password" },
                      {
                        value: "openssh",
                        label: /win/i.test(navigator.platform)
                          ? "Windows OpenSSH agent"
                          : "SSH agent (SSH_AUTH_SOCK)",
                      },
                      ...(/win/i.test(navigator.platform)
                        ? [
                            {
                              value: "pageant",
                              label:
                                "Pageant (including compatible hardware agents)",
                            },
                          ]
                        : []),
                    ]}
                  />
                  {data.agent && (
                    <>
                      <button
                        type="button"
                        className="secondary"
                        onClick={async () => {
                          try {
                            setAgentKeys(
                              await call<any[]>("agent_identities", {
                                kind: data.agent,
                              }),
                            );
                          } catch (e) {
                            setError(errorText(e));
                          }
                        }}
                      >
                        Load agent keys
                      </button>
                      <Select
                        label="Signing key"
                        value={data.agentKey ?? ""}
                        onChange={(v) => {
                          const key = agentKeys.find((k) => k.agentKey === v);
                          if (key)
                            setData((d) => ({ ...d, ...key, label: d.label }));
                        }}
                        options={[
                          { value: "", label: "Let agent offer its keys" },
                          ...agentKeys.map((k) => ({
                            value: k.agentKey,
                            label: k.label || k.agentKey,
                          })),
                        ]}
                      />
                      <div className="panel-tip">
                        The private key stays in the agent or hardware.
                        Reconnect the same signing device on another computer.
                        Native hardware enrollment is not included.
                      </div>
                    </>
                  )}
                  {data.publicKey && (
                    <>
                      <div className="section-title">Install public key</div>
                      <Select
                        label="Target SSH host"
                        value={installHost}
                        onChange={setInstallHost}
                        options={[
                          { value: "", label: "Select a POSIX host" },
                          ...hosts.map((h) => ({
                            value: h.id,
                            label: h.data.label,
                          })),
                        ]}
                      />
                      <button
                        type="button"
                        className="secondary"
                        disabled={!installHost || busy}
                        onClick={async () => {
                          setBusy(true);
                          try {
                            await call("key_install", {
                              hostId: installHost,
                              publicKey: data.publicKey,
                            });
                            setError("Public key installed successfully.");
                          } catch (e) {
                            setError(errorText(e));
                          } finally {
                            setBusy(false);
                          }
                        }}
                      >
                        Add to authorized_keys
                      </button>
                    </>
                  )}
                </>
              )}
              {record.kind === "snippet" && (
                <>
                  <TextArea
                    label="Command"
                    value={data.command}
                    onChange={(v) => update("command", v)}
                    rows={10}
                    placeholder="Type the command or shell script to send"
                  />
                  <Field
                    label="Package / category"
                    value={data.package}
                    onChange={(v) => update("package", v)}
                  />
                  <Check
                    checked={data.newline !== false}
                    onChange={(v) => update("newline", v)}
                  >
                    Execute after inserting (append Enter)
                  </Check>
                  <div className="panel-tip">
                    Run snippets from their card while a terminal is active.
                    Broadcast sends to every open panel.
                  </div>
                </>
              )}
              {record.kind === "tunnel" && (
                <>
                  <Select
                    label="SSH host"
                    value={data.hostId ?? ""}
                    onChange={(v) => update("hostId", v)}
                    options={[
                      { value: "", label: "Select a host" },
                      ...hosts.map((r) => ({
                        value: r.id,
                        label: r.data.label,
                      })),
                    ]}
                  />
                  <Select
                    label="Forwarding type"
                    value={data.mode ?? "local"}
                    onChange={(v) => update("mode", v)}
                    options={[
                      { value: "local", label: "Local forwarding" },
                      { value: "remote", label: "Remote forwarding" },
                      { value: "dynamic", label: "Dynamic SOCKS5" },
                    ]}
                  />
                  <div className="form-row">
                    <Field
                      label="Bind address"
                      value={data.bindAddress ?? "127.0.0.1"}
                      onChange={(v) => update("bindAddress", v)}
                    />
                    <Field
                      label="Bind port"
                      value={data.bindPort}
                      onChange={(v) => update("bindPort", v)}
                      type="number"
                    />
                  </div>
                  {data.mode !== "dynamic" && (
                    <div className="form-row">
                      <Field
                        label="Destination address"
                        value={data.targetAddress}
                        onChange={(v) => update("targetAddress", v)}
                      />
                      <Field
                        label="Port"
                        value={data.targetPort}
                        onChange={(v) => update("targetPort", v)}
                        type="number"
                      />
                    </div>
                  )}
                  <div className="panel-tip">
                    Listeners are restricted to loopback. Start and stop the
                    tunnel from its card.
                  </div>
                </>
              )}
              {record.kind === "workspace" && (
                <>
                  <p className="muted">
                    Select the hosts to reopen in this workspace. Connections
                    will authenticate again.
                  </p>
                  <div className="workspace-options">
                    {hosts.map((h) => (
                      <Check
                        key={h.id}
                        checked={(data.hostIds ?? []).includes(h.id)}
                        onChange={(v) =>
                          setData((d) => {
                            const entries = (d.hostIds ?? []).map(
                              (id: string, i: number) => ({
                                id,
                                stats: d.statsEnabled?.[i] !== false,
                              }),
                            );
                            const next = v
                              ? [...entries, { id: h.id, stats: true }]
                              : entries.filter(
                                  (e: { id: string }) => e.id !== h.id,
                                );
                            return {
                              ...d,
                              hostIds: next.map((e: { id: string }) => e.id),
                              statsEnabled: next.map(
                                (e: { stats: boolean }) => e.stats,
                              ),
                            };
                          })
                        }
                      >
                        {h.data.label}
                      </Check>
                    ))}
                  </div>
                  <Select
                    label="Layout"
                    value={data.layout ?? "split"}
                    onChange={(v) => update("layout", v)}
                    options={[
                      { value: "split", label: "Split panels" },
                      { value: "focus", label: "Focus one terminal" },
                    ]}
                  />
                </>
              )}
              {record.kind === "knownHost" && (
                <>
                  <Field
                    label="Server address"
                    value={data.address}
                    onChange={(v) => update("address", v)}
                  />
                  <TextArea
                    label="Trusted public key"
                    value={data.publicKey}
                    onChange={(v) => update("publicKey", v)}
                  />
                  <div className="fingerprint">{data.fingerprint}</div>
                </>
              )}
              {record.kind === "log" && (
                <>
                  <div className="panel-tip">
                    {data.endedAt} {data.truncated && " · Truncated at 4 MB"}
                  </div>
                  <TextArea
                    label="Notes / bookmarks"
                    value={data.notes}
                    onChange={(v) => update("notes", v)}
                    rows={3}
                  />
                  <pre className="log-content">{data.content}</pre>
                </>
              )}
            </>
          )}
          {isConnection && tab === "connection" && (
            <>
              <div className="section-title">
                <Terminal size={15} />
                Protocol & authentication
              </div>
              <Select
                label="Protocol"
                value={data.protocol ?? "ssh"}
                onChange={(v) => update("protocol", v)}
                options={[
                  { value: "ssh", label: "SSH" },
                  { value: "mosh", label: "Mosh (SSH + UDP)" },
                  { value: "telnet", label: "Telnet" },
                  { value: "serial", label: "Serial port" },
                ]}
              />
              {data.protocol !== "serial" ? (
                <>
                  <Field
                    label="Port"
                    value={data.port}
                    onChange={(v) => update("port", v)}
                    type="number"
                    placeholder="22 (SSH) / 23 (Telnet)"
                    hint="Leave empty to inherit from the group."
                  />
                  <Select
                    label="Identity"
                    value={data.credentialId ?? ""}
                    onChange={(v) => update("credentialId", v)}
                    options={[
                      { value: "", label: "Enter credentials below" },
                      ...credentials.map((r) => ({
                        value: r.id,
                        label: r.data.label,
                      })),
                    ]}
                  />
                  <Field
                    label="Username"
                    value={data.username}
                    onChange={(v) => update("username", v)}
                    placeholder="Inherited or requested on connect"
                  />
                  <Field
                    label="Password"
                    value={data.password}
                    onChange={(v) => update("password", v)}
                    type="password"
                    placeholder="Inherited or requested on connect"
                  />
                  {data.protocol === "mosh" && (
                    <Field
                      label="Mosh UDP address (optional)"
                      value={data.moshAddress}
                      onChange={(v) => update("moshAddress", v)}
                      placeholder="Defaults to host address"
                      hint="UDP must be reachable directly; SSH proxies do not carry Mosh UDP."
                    />
                  )}
                  <div className="panel-tip">
                    MFA challenges appear when requested by the SSH server. Add
                    private keys and certificates in Keychain.
                  </div>
                </>
              ) : (
                <>
                  <button
                    type="button"
                    className="secondary"
                    onClick={() =>
                      void call<string[]>("serial_ports")
                        .then((ports) => {
                          setSerialPorts(ports);
                          if (!ports.length)
                            setError("No serial devices were found.");
                        })
                        .catch((e) => setError(errorText(e)))
                    }
                  >
                    Refresh serial devices
                  </button>
                  {!!serialPorts.length && (
                    <Select
                      label="Detected device"
                      value={data.address ?? ""}
                      onChange={(v) => update("address", v)}
                      options={[
                        { value: "", label: "Choose a serial device" },
                        ...serialPorts.map((port) => ({
                          value: port,
                          label: port,
                        })),
                      ]}
                    />
                  )}
                  <Field
                    label="Baud rate"
                    value={data.serialBaud ?? 115200}
                    onChange={(v) => update("serialBaud", v)}
                    type="number"
                  />
                  <Select
                    label="Data bits"
                    value={String(data.serialDataBits ?? 8)}
                    onChange={(v) => update("serialDataBits", Number(v))}
                    options={[5, 6, 7, 8].map((n) => ({
                      value: String(n),
                      label: String(n),
                    }))}
                  />
                  <Select
                    label="Parity"
                    value={data.serialParity ?? "none"}
                    onChange={(v) => update("serialParity", v)}
                    options={["none", "odd", "even"].map((v) => ({
                      value: v,
                      label: v,
                    }))}
                  />
                  <Select
                    label="Stop bits"
                    value={String(data.serialStopBits ?? 1)}
                    onChange={(v) => update("serialStopBits", Number(v))}
                    options={[1, 2].map((n) => ({
                      value: String(n),
                      label: String(n),
                    }))}
                  />
                  <Select
                    label="Flow control"
                    value={data.serialFlowControl ?? "none"}
                    onChange={(v) => update("serialFlowControl", v)}
                    options={["none", "hardware", "software"].map((v) => ({
                      value: v,
                      label: v,
                    }))}
                  />
                </>
              )}
            </>
          )}
          {isConnection && tab === "advanced" && (
            <>
              <div className="section-title">
                <Link size={15} />
                Host chain
              </div>
              <p className="muted small">
                Connect through these hosts in order. Group chains are inherited
                unless overridden.
              </p>
              {(data.chain ?? []).map((id: string, i: number) => (
                <div className="chain-step" key={`${id}-${i}`}>
                  <b>{i + 1}</b>
                  <span>
                    {records.find((r) => r.id === id)?.data.label ??
                      "Missing host"}
                  </span>
                  <button
                    type="button"
                    className="icon-btn"
                    disabled={!i}
                    onClick={() => {
                      const a = [...data.chain];
                      [a[i - 1], a[i]] = [a[i], a[i - 1]];
                      update("chain", a);
                    }}
                  >
                    <ArrowUp size={14} />
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    onClick={() =>
                      update(
                        "chain",
                        data.chain.filter((_: string, n: number) => n !== i),
                      )
                    }
                  >
                    <X size={14} />
                  </button>
                </div>
              ))}
              <div className="add-chain">
                <Select
                  label="Jump host"
                  value={jump}
                  onChange={setJump}
                  options={[
                    { value: "", label: "Select an intermediate host" },
                    ...hosts.map((r) => ({ value: r.id, label: r.data.label })),
                  ]}
                />
                <button
                  type="button"
                  className="secondary"
                  disabled={!jump}
                  onClick={() => {
                    update("chain", [...(data.chain ?? []), jump]);
                    setJump("");
                  }}
                >
                  <Plus size={16} />
                </button>
              </div>
              <button
                type="button"
                className="text-btn"
                onClick={() => update("chain", null)}
              >
                Use group chain
              </button>
              <div className="section-title">
                <Shield size={15} />
                Proxy
              </div>
              <Select
                label="Proxy type"
                value={data.proxy?.kind ?? "none"}
                onChange={(v) =>
                  update(
                    "proxy",
                    v === "none"
                      ? null
                      : { kind: v, host: "", port: v === "http" ? 8080 : 1080 },
                  )
                }
                options={[
                  { value: "none", label: "No proxy / inherit group" },
                  { value: "socks5", label: "SOCKS5" },
                  { value: "http", label: "HTTP CONNECT" },
                ]}
              />
              {data.proxy && (
                <>
                  <div className="form-row">
                    <Field
                      label="Proxy address"
                      value={data.proxy.host}
                      onChange={(v) => changeProxy("host", v)}
                    />
                    <Field
                      label="Port"
                      value={data.proxy.port}
                      onChange={(v) => changeProxy("port", v)}
                      type="number"
                    />
                  </div>
                  <Field
                    label="Proxy username"
                    value={data.proxy.username}
                    onChange={(v) => changeProxy("username", v)}
                  />
                  <Field
                    label="Proxy password"
                    value={data.proxy.password}
                    onChange={(v) => changeProxy("password", v)}
                    type="password"
                  />
                </>
              )}
              <div className="section-title">
                <Terminal size={15} />
                Session startup
              </div>
              <TextArea
                label="Startup command"
                value={data.startup}
                onChange={(v) => update("startup", v)}
                rows={3}
              />
              <TextArea
                label="Environment variables (KEY=value, one per line)"
                value={Object.entries(data.environment ?? {})
                  .map(([k, v]) => `${k}=${v}`)
                  .join("\n")}
                onChange={(v) =>
                  update(
                    "environment",
                    Object.fromEntries(
                      v
                        .split("\n")
                        .filter((l) => l.includes("="))
                        .map((l) => {
                          const n = l.indexOf("=");
                          return [l.slice(0, n), l.slice(n + 1)];
                        }),
                    ),
                  )
                }
              />
            </>
          )}
          {error && <div className="notice error">{error}</div>}
        </div>
        <footer className="editor-footer">
          {records.some((r) => r.id === record.id) && (
            <button
              type="button"
              className="icon-btn danger"
              title="Delete record"
              onClick={() => onDelete(record.id)}
            >
              <Trash2 size={17} />
            </button>
          )}
          <button type="button" className="secondary" onClick={onClose}>
            Cancel
          </button>
          <button disabled={busy} className="primary">
            {busy ? (
              <Busy label="Saving…" />
            ) : (
              <>
                <Save size={15} />
                Save
              </>
            )}
          </button>
        </footer>
      </form>
    </aside>
  );
}
