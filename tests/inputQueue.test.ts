import { it, expect } from "vitest";
import { InputQueue } from "../src/inputQueue";
it("serializes writes and preserves Unicode across chunks", async () => {
  const written: string[] = [];
  let active = 0,
    max = 0;
  const q = new InputQueue(async (_id, data) => {
    max = Math.max(max, ++active);
    await new Promise((r) => setTimeout(r, 2));
    written.push(data);
    active--;
  });
  const text = "A".repeat(32767) + "🌍" + "şİğ";
  q.send("one", text, () => {});
  q.send("one", "\x1bOP\x0b\x0e", () => {});
  await new Promise((r) => setTimeout(r, 40));
  expect(written.join("")).toBe(text + "\x1bOP\x0b\x0e");
  expect(max).toBe(1);
  expect(written.every((s) => !/[\uD800-\uDBFF]$/.test(s))).toBe(true);
});
it("cancels pending input on close without replaying it", async () => {
  const written: string[] = [];
  let release!: () => void;
  const q = new InputQueue(async (_id, data) => {
    written.push(data);
    await new Promise<void>((r) => (release = r));
  });
  q.send("one", "first", () => {});
  q.send("one", "must not run", () => {});
  q.cancel("one");
  release();
  await new Promise((r) => setTimeout(r, 5));
  expect(written).toEqual(["first"]);
});
