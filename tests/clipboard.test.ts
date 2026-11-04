import { expect, it, vi } from "vitest";
const clipboard = vi.hoisted(() => ({ write: vi.fn(), read: vi.fn() }));
vi.mock("../src/api", () => ({ desktop: true }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: clipboard.write, readText: clipboard.read }));
import { copyText } from "../src/clipboard";

it("drops a queued OTP write after its view locks and preserves existing copy callers", async () => {
  let release!: () => void;
  clipboard.write.mockImplementationOnce(() => new Promise<void>(resolve => { release = resolve; }));
  const first = copyText("previous selection");
  await vi.waitFor(() => expect(clipboard.write).toHaveBeenCalledOnce());
  let current = true;
  const otp = copyText("287082", () => current);
  current = false;
  release();
  await Promise.all([first, otp]);
  expect(clipboard.write.mock.calls.map(args => args[0])).toEqual(["previous selection"]);
  await copyText("new selection");
  expect(clipboard.write).toHaveBeenLastCalledWith("new selection");
});
