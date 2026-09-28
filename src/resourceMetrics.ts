export interface ThroughputRate {
  name: string;
  readBytesPerSecond: number | null;
  writeBytesPerSecond: number | null;
}
export interface ResourceActivityData {
  network: ThroughputRate[];
  disks: ThroughputRate[];
  networkError: string | null;
  diskError: string | null;
  defaultInterface: string | null;
}
export function throughput(value: number | null | undefined, network = false) {
  if (value == null || !Number.isFinite(value) || value < 0) return "—";
  const rate = network ? (value * 8) / 1_000_000 : value / 1_048_576;
  return `${rate.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${network ? "Mbit/s" : "MiB/s"}`;
}
export function diskSeverity(d: {
  total: number;
  used: number;
  available: number;
}): 0 | 1 | 2 {
  if (d.total <= 0) return 0;
  if (d.available === 0 || d.used >= d.total) return 2;
  return d.used / d.total >= 0.9 ? 1 : 0;
}
