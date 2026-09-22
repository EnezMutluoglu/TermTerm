import { X, Server, Folder, KeyRound, Info, Link } from "lucide-react";
import type { Entity } from "./types";
import { connectionDetails, containedRecords } from "./recordActions";
import type { ReactNode } from "react";

export default function RecordDetails({
  record,
  records,
  onClose,
  onContextMenu,
}: {
  record: Entity;
  records: Entity[];
  onClose: () => void;
  onContextMenu: React.MouseEventHandler<HTMLElement>;
}) {
  const data = ["host", "group"].includes(record.kind)
    ? connectionDetails(record, records)
    : record.data;
  const name = (id: string) =>
    records.find((r) => r.id === id)?.data.label || id || "None";
  const row = (label: string, value: ReactNode) => (
    <div className="detail-row" key={label}>
      <dt>{label}</dt>
      <dd>{value === "" || value == null ? "—" : value}</dd>
    </div>
  );
  const groupItems =
    record.kind === "group" ? containedRecords(records, [record.id]) : [];
  return (
    <aside
      className="editor record-details"
      aria-label="Record details"
      onContextMenu={onContextMenu}
    >
      <header className="editor-header">
        <div>
          <span className="eyebrow">
            {record.kind === "credential"
              ? "IDENTITY"
              : record.kind.toUpperCase()}{" "}
            DETAILS
          </span>
          <h2>{record.data.label || record.data.address}</h2>
        </div>
        <button
          className="icon-btn"
          aria-label="Close details"
          onClick={onClose}
        >
          <X size={19} />
        </button>
      </header>
      <div className="editor-body">
        <div className="record-symbol">
          {record.kind === "group" ? (
            <Folder size={28} />
          ) : record.kind === "credential" ? (
            <KeyRound size={28} />
          ) : (
            <Server size={28} />
          )}
        </div>
        <dl className="detail-list">
          {row("Name", record.data.label)}
          {["host", "group"].includes(record.kind) && (
            <>
              {row("Group", name(record.data.groupId))}
              {record.kind === "group" &&
                row(
                  "Contents",
                  `${groupItems.filter((r) => r.kind === "host").length} hosts · ${groupItems.filter((r) => r.kind === "group" && r.id !== record.id).length} subgroups`,
                )}
              {record.kind === "host" && row("Address", data.address)}
              {row("Protocol", String(data.protocol || "ssh").toUpperCase())}
              {row("Port", data.port || 22)}
              {row("Username", data.username)}
              {row("Identity", name(data.credentialId))}
              {row(
                "Authentication",
                data.privateKey || data.keyPath
                  ? "SSH key"
                  : data.agent
                    ? "SSH agent"
                    : data.password
                      ? "Password saved"
                      : "Prompt on connection",
              )}
              {row("Tags", (data.tags ?? []).join(", "))}
            </>
          )}
          {record.kind === "credential" && (
            <>
              {row("Username", data.username)}
              {row(
                "SSH key",
                data.privateKey
                  ? "Stored in encrypted vault"
                  : data.keyPath || "None",
              )}
              {row("Password", data.password ? "Saved · hidden" : "None")}
              {row("Public key", data.publicKey)}
            </>
          )}
          {record.kind === "tunnel" && (
            <>
              {row("Type", data.mode)}
              {row("Host", name(data.hostId))}
              {row(
                "Listen",
                `${data.bindAddress || "127.0.0.1"}:${data.bindPort}`,
              )}
              {row(
                "Destination",
                `${data.targetAddress || ""}:${data.targetPort || ""}`,
              )}
            </>
          )}
          {record.kind === "workspace" && (
            <>
              {row("View", data.layout)}
              {row("Hosts", (data.hostIds ?? []).map(name).join(", "))}
            </>
          )}
          {record.kind === "knownHost" && (
            <>
              {row("Address", data.address)}
              {row("Fingerprint", data.fingerprint)}
              {row("Algorithm", data.algorithm)}
            </>
          )}
          {row("Notes", data.notes)}
        </dl>
        {["host", "group"].includes(record.kind) && (
          <>
            <h3 className="section-title">
              <Link size={15} />
              Host chain
            </h3>
            {data.chain?.length ? (
              <ol className="detail-chain">
                {data.chain.map((id: string) => (
                  <li key={id}>{name(id)}</li>
                ))}
              </ol>
            ) : (
              <p className="muted small">Direct connection</p>
            )}
            {data.proxy && (
              <dl>
                {row(
                  "Proxy",
                  `${data.proxy.kind} · ${data.proxy.host}:${data.proxy.port}`,
                )}
              </dl>
            )}
          </>
        )}
        {record.kind === "snippet" && (
          <>
            <h3 className="section-title">Command</h3>
            <pre className="log-content">{data.command}</pre>
          </>
        )}
        {record.kind === "log" && (
          <>
            <h3 className="section-title">Session output</h3>
            <pre className="log-content">
              {data.content || "No output recorded."}
            </pre>
          </>
        )}
      </div>
      <footer className="details-hint">
        <Info size={14} />
        Right-click for actions · Shift+F10 from a card
      </footer>
    </aside>
  );
}
