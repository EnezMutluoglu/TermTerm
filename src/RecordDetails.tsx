import { tr, entityLabel } from "./i18n";
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
    records.find((r) => r.id === id)?.data.label || id || tr("None");
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
      aria-label={tr("Record details")}
      onContextMenu={onContextMenu}
    >
      <header className="editor-header">
        <div>
          <span className="eyebrow">
            {record.kind === "credential"
              ? tr("IDENTITY")
              : entityLabel(record.kind)}{" "}
            {tr("DETAILS")}
          </span>
          <h2>{record.data.label || record.data.address}</h2>
        </div>
        <button
          className="icon-btn"
          aria-label={tr("Close details")}
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
          {row(tr("Name"), record.data.label)}
          {["host", "group"].includes(record.kind) && (
            <>
              {row(tr("Group"), name(record.data.groupId))}
              {record.kind === "group" &&
                row(
                  tr("Contents"),
                  tr("{hosts} hosts · {groups} subgroups", {
                    hosts: groupItems.filter((r) => r.kind === "host").length,
                    groups: groupItems.filter(
                      (r) => r.kind === "group" && r.id !== record.id,
                    ).length,
                  }),
                )}
              {record.kind === "host" && row(tr("Address"), data.address)}
              {row(
                tr("Protocol"),
                String(data.protocol || "ssh").toUpperCase(),
              )}
              {row("Port", data.port || 22)}
              {row(tr("Username"), data.username)}
              {row(tr("Identity"), name(data.credentialId))}
              {row(
                tr("Authentication"),
                data.privateKey || data.keyPath
                  ? tr("SSH key")
                  : data.agent
                    ? tr("SSH agent")
                    : data.password
                      ? tr("Password saved")
                      : tr("Prompt on connection"),
              )}
              {row(tr("Tags"), (data.tags ?? []).join(", "))}
            </>
          )}
          {record.kind === "credential" && (
            <>
              {row(tr("Username"), data.username)}
              {row(
                tr("SSH key"),
                data.privateKey
                  ? tr("Stored in encrypted vault")
                  : data.keyPath || tr("None"),
              )}
              {row(
                tr("Password"),
                data.password ? tr("Saved · hidden") : tr("None"),
              )}
              {row(tr("Public key"), data.publicKey)}
            </>
          )}
          {record.kind === "tunnel" && (
            <>
              {row(tr("Type"), data.mode)}
              {row(tr("Host"), name(data.hostId))}
              {row(
                tr("Listen"),
                `${data.bindAddress || "127.0.0.1"}:${data.bindPort}`,
              )}
              {row(
                tr("Destination"),
                `${data.targetAddress || ""}:${data.targetPort || ""}`,
              )}
            </>
          )}
          {record.kind === "workspace" && (
            <>
              {row(tr("View"), data.layout)}
              {row(tr("Hosts"), (data.hostIds ?? []).map(name).join(", "))}
            </>
          )}
          {record.kind === "knownHost" && (
            <>
              {row(tr("Address"), data.address)}
              {row(tr("Fingerprint"), data.fingerprint)}
              {row(tr("Algorithm"), data.algorithm)}
            </>
          )}
          {row(tr("Notes"), data.notes)}
        </dl>
        {["host", "group"].includes(record.kind) && (
          <>
            <h3 className="section-title">
              <Link size={15} />
              {tr("Host chain")}
            </h3>
            {data.chain?.length ? (
              <ol className="detail-chain">
                {data.chain.map((id: string) => (
                  <li key={id}>{name(id)}</li>
                ))}
              </ol>
            ) : (
              <p className="muted small">{tr("Direct connection")}</p>
            )}
            {data.proxy && (
              <dl>
                {row(
                  tr("Proxy"),
                  `${data.proxy.kind} · ${data.proxy.host}:${data.proxy.port}`,
                )}
              </dl>
            )}
          </>
        )}
        {record.kind === "snippet" && (
          <>
            <h3 className="section-title">{tr("Command")}</h3>
            <pre className="log-content">{data.command}</pre>
          </>
        )}
        {record.kind === "log" && (
          <>
            <h3 className="section-title">{tr("Session output")}</h3>
            <pre className="log-content">
              {data.content || tr("No output recorded.")}
            </pre>
          </>
        )}
      </div>
      <footer className="details-hint">
        <Info size={14} />
        {tr("Right-click for actions · Shift+F10 from a card")}
      </footer>
    </aside>
  );
}
