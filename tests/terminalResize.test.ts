import { describe, it, expect, vi } from "vitest";
import { terminalResizeQueue } from "../src/terminalResize";
const tick = () => new Promise<void>((r) => setTimeout(r, 0));
describe("terminal PTY resize delivery", () => {
  it("serializes slow IPC and replaces intermediate drag sizes with the newest", async () => {
    const releases: (() => void)[] = [];
    const send = vi.fn(() => new Promise<void>((r) => releases.push(r)));
    const q = terminalResizeQueue(send, vi.fn());
    q.request({ cols: 100, rows: 30 });
    q.request({ cols: 200, rows: 50 });
    q.request({ cols: 240, rows: 60 });
    expect(send).toHaveBeenCalledTimes(1);
    releases.shift()!();
    await tick();
    expect(send).toHaveBeenLastCalledWith({ cols: 240, rows: 60 });
    releases.shift()!();
    await tick();
    q.request({ cols: 240, rows: 60 });
    expect(send).toHaveBeenCalledTimes(2);
    q.request({ cols: 240, rows: 60 }, true);
    expect(send).toHaveBeenCalledTimes(3);
    releases.shift()!();
    q.dispose();
  });
  it("retries a failed unchanged size and stops queued sends on disposal", async () => {
    const error = vi.fn();
    const send = vi
      .fn()
      .mockRejectedValueOnce(Error("IPC failed"))
      .mockResolvedValue(undefined);
    const q = terminalResizeQueue(send, error);
    q.request({ cols: 120, rows: 40 });
    await tick();
    expect(error).toHaveBeenCalledTimes(1);
    q.request({ cols: 120, rows: 40 });
    await tick();
    expect(send).toHaveBeenCalledTimes(2);
    q.dispose();
    q.request({ cols: 200, rows: 50 }, true);
    expect(send).toHaveBeenCalledTimes(2);
  });
});
