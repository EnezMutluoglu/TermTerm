import type { QueryResult } from "./types";
export type Health =
  | "ok"
  | "warning"
  | "critical"
  | "unknown"
  | "stale"
  | "unsupported"
  | "permission_denied"
  | "partial"
  | "failed";
export type Sample = {
  actionId: string;
  observedAt: number;
  resetEpoch: unknown;
  status: string;
  series: { key: string; label?: string; values: Record<string, number> }[];
};
export function rows(result?: QueryResult) {
  return (
    result?.results.flatMap(
      (r) =>
        r.rows?.map((row) =>
          Object.fromEntries(
            r.columns.map((c, i) => [c.name.toLowerCase(), row[i]]),
          ),
        ) ?? [],
    ) ?? []
  );
}
export function capacity(
  samples: { at: number; value: number | null }[],
  warning = 80,
  critical = 90,
  now = Date.now(),
): Health {
  if (!samples.length) return "unknown";
  const latest = samples.at(-1)!;
  if (now - latest.at > 90000) return "stale";
  if (latest.value === null || !Number.isFinite(latest.value)) return "unknown";
  let state: Health = "unknown",
    candidate: Health = "unknown",
    count = 0,
    previous = 0;
  for (const sample of samples) {
    if (sample.at <= previous || sample.at - previous > 90000) count = 0;
    previous = sample.at;
    if (sample.value === null || !Number.isFinite(sample.value)) {
      count = 0;
      state = "unknown";
      continue;
    }
    const next: Health =
      sample.value >= critical
        ? "critical"
        : sample.value >= warning
          ? "warning"
          : "ok";
    count = next === candidate ? count + 1 : 1;
    candidate = next;
    if (count >= 3) state = next;
  }
  return state;
}
export function growth(samples: { at: number; value: number }[]) {
  if (samples.length < 3) return null;
  const first = samples[0],
    last = samples.at(-1)!;
  if (
    last.at - first.at < 60000 ||
    samples.some(
      (s, i) =>
        i > 0 && (s.at <= samples[i - 1].at || s.value < samples[i - 1].value),
    )
  )
    return null;
  return (last.value - first.value) / ((last.at - first.at) / 1000);
}
export function compare(a: Sample, b: Sample) {
  if (
    a.status !== "ok" ||
    b.status !== "ok" ||
    a.actionId !== b.actionId ||
    a.observedAt >= b.observedAt ||
    JSON.stringify(a.resetEpoch) !== JSON.stringify(b.resetEpoch)
  )
    return {
      valid: false,
      reason: "Sunucu yeniden başlamış veya örnek kapsamı/zamanı değişmiş.",
      deltas: [],
    };
  const deltas: {
    key: string;
    label?: string;
    values: Record<string, number>;
  }[] = [];
  for (const current of b.series) {
    const previous = a.series.find((s) => s.key === current.key);
    if (!previous) continue;
    const values: Record<string, number> = {};
    for (const [name, value] of Object.entries(current.values)) {
      if (previous.values[name] === undefined) continue;
      if (value < previous.values[name])
        return {
          valid: false,
          reason: "Sayaç sıfırlanmış veya kapsam değişmiş.",
          deltas: [],
        };
      values[name] = value - previous.values[name];
    }
    deltas.push({ key: current.key, label: current.label, values });
  }
  return {
    valid: deltas.length > 0,
    reason: deltas.length ? "" : "Eşleşen seri yok.",
    deltas,
  };
}
export function blocking(result: QueryResult | undefined, engine: string) {
  const records = rows(result);
  return records.flatMap((row, i) => {
    const prefix = engine === "oracle" ? String(row.inst_id) + ":" : "";
    const session =
      engine === "postgresql"
        ? (row.pid ?? row.blocked_pid)
        : engine === "mssql"
          ? (row.blocked_session ?? row.session_id)
          : row.sid;
    const parents =
      engine === "postgresql"
        ? String(row.blockers ?? row.blocking_pids ?? "")
            .replace(/[{}]/g, "")
            .split(",")
        : [
            String(
              engine === "mssql"
                ? (row.blocking_session_id ?? "")
                : (row.blocking_session ?? row.final_blocking_session ?? ""),
            ),
          ];
    return parents.map((parent) => ({
      id: prefix + String(session ?? i),
      parent:
        parent && parent !== "0"
          ? (engine === "oracle"
              ? String(row.blocking_instance ?? row.inst_id) + ":"
              : "") + parent
          : "",
      wait: String(row.wait_event ?? row.wait_type ?? row.event ?? ""),
      age: String(
        row.transaction_age ??
          row.query_age ??
          row.wait_time ??
          row.seconds_in_wait ??
          "",
      ),
      row,
    }));
  });
}
export function blockingTree(result: QueryResult | undefined, engine: string) {
  const nodes = blocking(result, engine);
  const out: (ReturnType<typeof blocking>[number] & {
    depth: number;
    cycle?: boolean;
  })[] = [];
  const visited = new Set<string>();
  function walk(parent: string, depth: number, path: Set<string>) {
    for (const n of nodes.filter((n) => n.parent === parent)) {
      const key = n.parent + ">" + n.id;
      if (visited.has(key)) continue;
      visited.add(key);
      const cycle = path.has(n.id);
      out.push({ ...n, depth, cycle });
      if (!cycle) walk(n.id, depth + 1, new Set([...path, n.id]));
    }
  }
  const roots = new Set(
    nodes
      .filter((n) => !nodes.some((p) => p.id === n.parent))
      .map((n) => n.parent),
  );
  for (const root of roots) {
    if (root)
      out.push({
        id: root,
        parent: "",
        depth: 0,
        wait: "Kaynak oturum",
        age: "",
        row: {},
      });
    walk(root, root ? 1 : 0, new Set([root]));
  }
  for (const n of nodes)
    if (!visited.has(n.parent + ">" + n.id))
      walk(n.parent, 0, new Set([n.parent]));
  return out;
}
export function compareWindows(a: Sample, b: Sample, c: Sample, d: Sample) {
  if (b.observedAt > c.observedAt)
    return { valid: false, reason: "Pencereler çakışıyor.", windows: [] };
  const x = compare(a, b),
    y = compare(c, d);
  if (!x.valid || !y.valid)
    return { valid: false, reason: x.valid ? y.reason : x.reason, windows: [] };
  return {
    valid: true,
    reason: "",
    windows: [
      { seconds: (b.observedAt - a.observedAt) / 1000, deltas: x.deltas },
      { seconds: (d.observedAt - c.observedAt) / 1000, deltas: y.deltas },
    ],
  };
}
