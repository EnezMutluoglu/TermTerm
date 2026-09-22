import { tr } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, desktop, errorText } from "./api";
import { Cpu, MemoryStick, HardDrive, ChevronDown, X } from "lucide-react";
export interface ResourceSnapshot {
  sessionId: string;
  source: string;
  os: string;
  timestamp: number;
  cpuPercent: number | null;
  memory: { total: number; used: number; available: number } | null;
  disks: {
    mountPoint: string;
    device: string;
    filesystem: string;
    total: number;
    used: number;
    available: number;
    root: boolean;
    virtual: boolean;
  }[];
  sampledAt: number | null;
  disksAt: number | null;
  error: string | null;
  diskError: string | null;
  supported: boolean;
}
type Point = { time: number; cpu: number | null; memory: number | null };
const percent = (used: number, total: number) =>
  total > 0 ? Math.min(100, Math.max(0, (used / total) * 100)) : 0;
export function size(n: number) {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i > 1 ? 1 : 0)} ${units[i]}`;
}
function Spark({
  points,
  field,
}: {
  points: Point[];
  field: "cpu" | "memory";
}) {
  const end = points.at(-1)?.time ?? Date.now();
  let d = "";
  let last = 0;
  for (const p of points) {
    const value = p[field];
    if (value === null) {
      last = 0;
      continue;
    }
    const x = Math.max(0, ((p.time - end + 60000) / 60000) * 72),
      y = 22 - (Math.max(0, Math.min(100, value)) / 100) * 20;
    d += `${last && p.time - last < 7000 ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)} `;
    last = p.time;
  }
  return (
    <svg className="resource-spark" viewBox="0 0 72 24" aria-hidden="true">
      <path d={d} fill="none" stroke="currentColor" strokeWidth="1.6" />
    </svg>
  );
}
export default function ResourceMonitor({
  sessionId,
  enabled,
  visible,
  connected,
}: {
  sessionId: string;
  enabled: boolean;
  visible: boolean;
  connected: boolean;
}) {
  const [sample, setSample] = useState<ResourceSnapshot | null>(null),
    [points, setPoints] = useState<Point[]>([]),
    [details, setDetails] = useState(false),
    [virtual, setVirtual] = useState(false),
    [error, setError] = useState("");
  const [foreground, setForeground] = useState(!document.hidden),
    [now, setNow] = useState(Date.now());
  const last = useRef<number | null>(null);
  const control = useRef(Promise.resolve());
  useEffect(() => {
    const fn = () => setForeground(!document.hidden);
    document.addEventListener("visibilitychange", fn);
    return () => document.removeEventListener("visibilitychange", fn);
  }, []);
  useEffect(() => {
    if (!enabled || !visible) return;
    const timer = setInterval(() => setNow(Date.now()), 2000);
    return () => clearInterval(timer);
  }, [enabled, visible]);
  useEffect(() => {
    if (!desktop) return;
    let live = true;
    const un = listen<ResourceSnapshot>("session-metrics", ({ payload: s }) => {
      if (!live || s.sessionId !== sessionId) return;
      setSample(s);
      setError("");
      setNow(Date.now());
      if (s.sampledAt && s.sampledAt !== last.current) {
        last.current = s.sampledAt;
        setPoints((p) =>
          [
            ...p.filter((p) => p.time > s.sampledAt! - 60000),
            {
              time: s.sampledAt!,
              cpu: s.cpuPercent,
              memory: s.memory ? percent(s.memory.used, s.memory.total) : null,
            },
          ].slice(-32),
        );
      }
    });
    return () => {
      live = false;
      void un.then((f) => f());
    };
  }, [sessionId]);
  useEffect(() => {
    if (!desktop) return;
    let live = true;
    control.current = control.current
      .then(() =>
        call<void>("session_metrics_set", {
          sessionId,
          enabled: enabled && visible && foreground && connected,
        }),
      )
      .catch((e) => {
        if (live) setError(errorText(e));
      });
    return () => {
      live = false;
      control.current = control.current
        .then(() =>
          call<void>("session_metrics_set", { sessionId, enabled: false }),
        )
        .catch(() => {});
    };
  }, [sessionId, enabled, visible, foreground, connected]);
  useEffect(() => {
    if (!enabled || !visible) setDetails(false);
  }, [enabled, visible]);
  if (!enabled) return null;
  const disks = (sample?.disks ?? []).filter(
      (d) => virtual || !d.virtual || d.root,
    ),
    root = disks.find((d) => d.root) ?? disks[0];
  const stale = !!sample?.sampledAt && now - sample.sampledAt > 6500;
  const diskStale = !!sample?.disksAt && now - sample.disksAt > 25000;
  return (
    <aside
      className={"resource-monitor" + (stale ? " stale" : "")}
      data-testid="resource-monitor"
      aria-label={tr("Terminal resources")}
    >
      <div
        className="resource-source"
        title={[sample?.source, sample?.os, error || sample?.error]
          .filter(Boolean)
          .join(" · ")}
      >
        {sample && !sample.supported ? (
          <span>{sample.error ?? sample.source}</span>
        ) : (
          <>
            <span className="resource-origin">
              {sample?.source ?? tr("Resources")}
            </span>
            <span className={stale ? "stale-badge" : ""}>
              {!connected
                ? tr("Disconnected")
                : !visible || !foreground
                  ? tr("Paused")
                  : error || sample?.error
                    ? tr("Unavailable")
                    : stale
                      ? tr("Stale")
                      : sample?.sampledAt
                        ? tr("Live")
                        : tr("Sampling…")}
            </span>
          </>
        )}
      </div>
      {sample?.supported !== false && (
        <div className="resource-summary">
          <div
            className="resource-value resource-cpu"
            title={tr("Total CPU usage")}
          >
            <Cpu size={12} />
            <span>
              CPU{" "}
              <b>
                {sample?.cpuPercent == null
                  ? "—"
                  : `${sample.cpuPercent.toFixed(0)}%`}
              </b>
            </span>
            <Spark points={points} field="cpu" />
          </div>
          <div
            className="resource-value resource-memory"
            title={
              sample?.memory
                ? `${size(sample.memory.used)} / ${size(sample.memory.total)}`
                : tr("Memory unavailable")
            }
          >
            <MemoryStick size={12} />
            <span>
              RAM{" "}
              <b>
                {sample?.memory
                  ? `${percent(sample.memory.used, sample.memory.total).toFixed(0)}%`
                  : "—"}
              </b>
            </span>
            {sample?.memory && (
              <span className="resource-memory-amount">
                {size(sample.memory.used)} / {size(sample.memory.total)}
              </span>
            )}
            <Spark points={points} field="memory" />
          </div>
          <button
            className="resource-disk"
            onClick={() => setDetails(!details)}
            aria-expanded={details}
            title={sample?.diskError ?? tr("Mounted filesystems")}
          >
            <HardDrive size={12} />
            <span>
              {root?.mountPoint ?? tr("Disks")}{" "}
              <b>
                {root ? `${percent(root.used, root.total).toFixed(0)}%` : "—"}
              </b>
            </span>
            <span className="capacity-track">
              <i
                className={
                  root && percent(root.used, root.total) > 90
                    ? "critical"
                    : undefined
                }
                style={{
                  width: `${root ? percent(root.used, root.total) : 0}%`,
                }}
              />
            </span>
            <small>{disks.length > 1 ? `+${disks.length - 1}` : ""}</small>
            <ChevronDown size={12} />
          </button>
        </div>
      )}
      {details && (
        <div
          className="resource-details"
          role="dialog"
          aria-label={tr("Mounted filesystems")}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Escape") setDetails(false);
          }}
        >
          <header>
            <strong>{tr("Mounted filesystems")}</strong>
            <button
              className="icon-btn"
              aria-label={tr("Close filesystem details")}
              onClick={() => setDetails(false)}
            >
              <X size={14} />
            </button>
          </header>
          <label className="resource-check">
            <input
              type="checkbox"
              checked={virtual}
              onChange={(e) => setVirtual(e.target.checked)}
            />{" "}
            {tr("Show virtual mounts")}
          </label>
          {(sample?.diskError || diskStale) && (
            <p className="resource-warning">
              {sample?.diskError ?? tr("Stale disk counters")}
            </p>
          )}
          <div className="resource-mount-list">
            {disks.map((d) => (
              <div
                className="resource-mount"
                key={`${d.device}:${d.mountPoint}`}
              >
                <div>
                  <strong title={d.mountPoint}>{d.mountPoint}</strong>
                  <b>{percent(d.used, d.total).toFixed(0)}%</b>
                </div>
                <span title={d.device}>
                  {d.filesystem || tr("Filesystem")} · {d.device}
                </span>
                <span className="capacity-track">
                  <i
                    className={
                      percent(d.used, d.total) > 90 ? "critical" : undefined
                    }
                    style={{
                      width: `${percent(d.used, d.total)}%`,
                    }}
                  />
                </span>
                <small>
                  {size(d.used)} / {size(d.total)} · {size(d.available)}{" "}
                  {tr("available")}
                </small>
              </div>
            ))}
            {!disks.length && <p>{tr("Waiting for filesystem counters…")}</p>}
          </div>
          <footer>
            {tr(
              "Shared filesystem capacity is shown per mount, never added together.",
            )}
          </footer>
        </div>
      )}
    </aside>
  );
}
