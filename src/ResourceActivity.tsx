import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, Network, HardDrive, X } from "lucide-react";
import { tr } from "./i18n";
import {
  throughput,
  type ResourceActivityData,
  type ThroughputRate,
} from "./resourceMetrics";
function preferred(values: ThroughputRate[], name?: string | null) {
  return (
    values.find((v) => v.name === name) ??
    [...values].sort(
      (a, b) =>
        (b.readBytesPerSecond ?? 0) +
        (b.writeBytesPerSecond ?? 0) -
        ((a.readBytesPerSecond ?? 0) + (a.writeBytesPerSecond ?? 0)),
    )[0]
  );
}
export default function ResourceActivity({
  activity,
  unavailable,
  open,
  onToggle,
}: {
  activity?: ResourceActivityData | null;
  unavailable?: string | null;
  open: boolean;
  onToggle: () => void;
}) {
  const [networkName, setNetwork] = useState<string>(),
    [diskName, setDisk] = useState<string>();
  const network = preferred(
    activity?.network ?? [],
    networkName ?? activity?.defaultInterface,
  );
  const disk = preferred(activity?.disks ?? [], diskName);
  // Keep the chosen device stable after its first measured rate; devices are never summed.
  useEffect(() => {
    if (network?.readBytesPerSecond != null && network.name !== networkName)
      setNetwork(network.name);
  }, [network, networkName]);
  useEffect(() => {
    if (disk?.readBytesPerSecond != null && disk.name !== diskName)
      setDisk(disk.name);
  }, [disk, diskName]);
  const networkError = unavailable || activity?.networkError;
  const diskError = unavailable || activity?.diskError;
  const values = (
    v: ThroughputRate | undefined,
    net: boolean,
    error?: string | null,
  ) => (
    <>
      <span title={tr(net ? "Incoming" : "Disk read")}>
        <ArrowDown size={12} />
        <span className="resource-flow-label">
          {tr(net ? "Incoming" : "Read")}
        </span>
        <b>{throughput(error ? null : v?.readBytesPerSecond, net)}</b>
      </span>
      <span title={tr(net ? "Outgoing" : "Disk write")}>
        <ArrowUp size={12} />
        <span className="resource-flow-label">
          {tr(net ? "Outgoing" : "Write")}
        </span>
        <b>{throughput(error ? null : v?.writeBytesPerSecond, net)}</b>
      </span>
    </>
  );
  return (
    <>
      <div className="resource-throughput">
        <button
          className="resource-flow"
          data-testid="network-throughput"
          aria-expanded={open}
          onClick={onToggle}
          title={networkError ?? network?.name ?? tr("Network traffic")}
        >
          <Network size={13} />
          <span className="resource-flow-name">{tr("Network")}</span>
          {values(network, true, networkError)}
        </button>
        <button
          className="resource-flow"
          data-testid="disk-throughput"
          aria-expanded={open}
          onClick={onToggle}
          title={diskError ?? disk?.name ?? tr("Disk I/O")}
        >
          <HardDrive size={13} />
          <span className="resource-flow-name">{tr("Disk I/O")}</span>
          {values(disk, false, diskError)}
        </button>
      </div>
      {open && (
        <div
          className="resource-details resource-activity-details"
          role="dialog"
          aria-label={tr("Network and disk activity")}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Escape") onToggle();
          }}
        >
          <header>
            <strong>{tr("Network and disk activity")}</strong>
            <button
              className="icon-btn"
              aria-label={tr("Close activity details")}
              onClick={onToggle}
            >
              <X size={14} />
            </button>
          </header>
          <label>
            {tr("Network interface")}
            <select
              aria-label={tr("Network interface")}
              value={network?.name ?? ""}
              onChange={(e) => setNetwork(e.target.value)}
            >
              {!network && <option value="">—</option>}
              {activity?.network.map((n) => (
                <option key={n.name} value={n.name}>
                  {n.name}
                </option>
              ))}
            </select>
          </label>
          <div className="resource-flow-detail">
            {values(network, true, networkError)}
          </div>
          {networkError && (
            <p className="resource-warning">{tr(networkError)}</p>
          )}
          <label>
            {tr("Disk device")}
            <select
              aria-label={tr("Disk device")}
              value={disk?.name ?? ""}
              onChange={(e) => setDisk(e.target.value)}
            >
              {!disk && <option value="">—</option>}
              {activity?.disks.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.name}
                </option>
              ))}
            </select>
          </label>
          <div className="resource-flow-detail">
            {values(disk, false, diskError)}
          </div>
          {diskError && <p className="resource-warning">{tr(diskError)}</p>}
          <footer>
            {tr(
              "Rates belong to the selected interface and disk on the connected machine. Devices and mounts are not added together.",
            )}
            <br />
            {tr(
              "Network: Mbit/s. Disk read/write: MiB/s. Disk activity is separate from capacity.",
            )}
          </footer>
        </div>
      )}
    </>
  );
}
