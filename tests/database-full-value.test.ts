import { describe, it, expect } from "vitest";
import { valuePreview, type FullValue } from "../src/database/LobViewer";
import { transactionObservation } from "../src/database/evidence";
const sample = (bytes: Uint8Array, binary: boolean): FullValue => ({
  base64: Buffer.from(bytes).toString("base64"),
  bytes: bytes.length,
  binary,
  isNull: false,
  encoding: binary ? "binary" : "UTF-8",
  observedAt: 0,
  sha256: "",
});
describe("Full values and transaction observations", () => {
  it("renders UTF8 literally and handles clipped multibyte characters", () => {
    const text = "<script>İstanbul 😀</script>";
    expect(valuePreview(sample(new TextEncoder().encode(text), false))).toBe(
      text,
    );
    const long = "😀".repeat(20000);
    expect(
      valuePreview(sample(new TextEncoder().encode(long), false)),
    ).not.toContain("�");
  });
  it("limits binary preview without altering data", () => {
    const v = sample(
      Uint8Array.from({ length: 1024 }, (_, i) => i % 256),
      true,
    );
    expect(valuePreview(v).split(" ")).toHaveLength(256);
    expect(v.bytes).toBe(1024);
    expect(valuePreview(sample(new Uint8Array(), false))).toBe("");
  });
  it("does not equate missing or invalid transaction age to zero", () => {
    for (const age of [null, "", "-1", "NaN"])
      expect(transactionObservation({ age_seconds: age }, 300).state).toBe(
        "unknown",
      );
    expect(transactionObservation({ age_seconds: "300" }, 300).state).toBe(
      "warning",
    );
    expect(transactionObservation({ age_seconds: "299" }, 300).state).toBe(
      "observed",
    );
    expect(transactionObservation({ age_seconds: "1" }, 0).state).toBe(
      "unknown",
    );
  });
});
