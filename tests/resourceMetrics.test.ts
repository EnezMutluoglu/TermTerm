import { describe, it, expect } from "vitest";
import { diskSeverity, throughput } from "../src/resourceMetrics";
describe("resource units and capacity alerts", () => {
  it("uses decimal megabits for network and binary mebibytes for disk", () => {
    expect(throughput(1_000_000, true)).toMatch(/8[.,]00 Mbit\/s/);
    expect(throughput(1_048_576)).toMatch(/1[.,]00 MiB\/s/);
    expect(throughput(0, true)).toMatch(/0[.,]00 Mbit\/s/);
    for (const v of [null, undefined, NaN, Infinity, -1])
      expect(throughput(v)).toBe("—");
  });
  it("marks all full mounts including reserved space, and the exact 90% threshold", () => {
    const d = { total: 100, used: 89, available: 11 };
    expect(diskSeverity(d)).toBe(0);
    expect(diskSeverity({ ...d, used: 90, available: 10 })).toBe(1);
    expect(diskSeverity({ ...d, used: 95, available: 0 })).toBe(2);
    expect(diskSeverity({ ...d, used: 100 })).toBe(2);
    expect(diskSeverity({ total: 0, used: 0, available: 0 })).toBe(0);
  });
});
