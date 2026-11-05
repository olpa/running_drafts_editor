import { describe, expect, it } from "vitest";
import { DEFAULT_CONFIG, loadConfig } from "./config.ts";

describe("production configuration", () => {
  it("uses the reviewed Capture timing and retry policy", () => {
    expect(DEFAULT_CONFIG).toMatchObject({
      frameIntervalMs: 5_000,
      secureDeadlineMs: 2_000,
      retryFirstMs: 3_000,
      retrySecondMs: 10_000,
      retryThirdMs: 17_000,
    });
  });

  it("rejects a production frame-interval override while retaining other explicit settings", () => {
    expect(() => loadConfig({ frameIntervalMs: 10_000 })).toThrow(
      "frameIntervalMs must be the production value 5000",
    );
    expect(loadConfig({ frameIntervalMs: 5_000, requestTimeoutMs: 45_000 })).toMatchObject({
      frameIntervalMs: 5_000,
      requestTimeoutMs: 45_000,
    });
  });
});
