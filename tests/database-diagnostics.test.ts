import { describe, it, expect } from "vitest";
import {
  capacity,
  growth,
  compare,
  compareWindows,
  blockingTree,
  type Sample,
} from "../src/database/diagnostics";
describe("database diagnostics", () => {
  const now = 1_000_000;
  const samples = (values: (number | null)[]) =>
    values.map((value, i) => ({
      at: now - (values.length - 1 - i) * 1000,
      value,
    }));
  it("requires three consistent observations to raise, escalate and clear", () => {
    expect(capacity(samples([81, 82, 95]), 80, 90, now)).toBe("unknown");
    expect(capacity(samples([81, 82, 83]), 80, 90, now)).toBe("warning");
    expect(capacity(samples([95, 96, 97, 20, 20]), 80, 90, now)).toBe(
      "critical",
    );
    expect(capacity(samples([95, 96, 97, 20, 20, 20]), 80, 90, now)).toBe("ok");
  });
  it("does not turn missing and stale data into health", () => {
    expect(capacity(samples([0, 0, null]), 80, 90, now)).toBe("unknown");
    expect(capacity(samples([0, 0, 0]), 80, 90, now + 90001)).toBe("stale");
    expect(capacity([], 80, 90, now)).toBe("unknown");
  });
  it("requires enough time and detects capacity resets", () => {
    expect(
      growth([
        { at: 0, value: 1 },
        { at: 30000, value: 31 },
        { at: 60000, value: 61 },
      ]),
    ).toBe(1);
    expect(
      growth([
        { at: 0, value: 1 },
        { at: 100, value: 2 },
      ]),
    ).toBeNull();
    expect(
      growth([
        { at: 0, value: 1 },
        { at: 30000, value: 31 },
        { at: 60000, value: 2 },
      ]),
    ).toBeNull();
  });
  const s = (at: number, calls: number, epoch = 1): Sample => ({
    actionId: "test",
    observedAt: at,
    status: "ok",
    resetEpoch: epoch,
    series: [{ key: "a", values: { calls } }],
  });
  it("rejects resets, changed servers and overlapping windows", () => {
    expect(compare(s(1, 10), s(2, 2)).valid).toBe(false);
    expect(compare(s(1, 10), s(2, 20, 2)).valid).toBe(false);
    expect(compareWindows(s(1, 1), s(3, 3), s(2, 2), s(4, 4)).valid).toBe(
      false,
    );
    expect(compareWindows(s(1, 1), s(2, 2), s(2, 2), s(3, 4)).valid).toBe(true);
  });
  it("builds blocking ancestry without looping on cycles", () => {
    const r = {
      results: [
        {
          columns: ["pid", "blockers"].map((name) => ({ name, type: "text" })),
          rows: [
            ["2", "{1}"],
            ["3", "{2}"],
          ],
          truncated: false,
        },
      ],
      elapsedMs: 0,
      transaction: false,
      autoCommit: true,
    };
    expect(blockingTree(r, "postgresql").map((n) => [n.id, n.depth])).toEqual([
      ["1", 0],
      ["2", 1],
      ["3", 2],
    ]);
    r.results[0].rows.push(["1", "{3}"]);
    expect(blockingTree(r, "postgresql").length).toBeLessThan(5);
  });
});
