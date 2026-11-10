import { afterEach, describe, expect, it, vi } from "vitest";
import { createCaptureAlerts } from "../../alerts/capture-alerts.ts";
import type { Cues } from "../cues.ts";
import { createScenarioPlayback } from "./playback.ts";
import { createLabScenario, type ScenarioKind } from "./scenario.ts";

const kinds: ScenarioKind[] = [
  "healthy",
  "short",
  "older",
  "single",
  "two",
  "burst",
  "final",
  "failure",
  "finalization",
  "sealing",
  "repeated",
  "allowance-storage-warning",
  "allowance-duration-warning",
  "allowance-overlap",
  "allowance-storage-rejection",
  "allowance-duration-rejection",
  "allowance-duration-zero",
];

function cues(): Cues {
  return {
    prepare() {},
    play() {},
    loopProblem() {
      return () => {};
    },
    loopAllowanceWarning() {
      return () => {};
    },
    dispose() {},
  };
}

describe("arranged Capture playback", () => {
  afterEach(() => vi.useRealTimers());

  it("keeps the final two frame cycles visible at the fixed review pace", async () => {
    vi.useFakeTimers();
    const sound = cues();
    const alerts = createCaptureAlerts(sound);
    const scenario = createLabScenario({
      name: "healthy",
      manual: true,
      intervalMs: 5000,
      deadlineMs: 2000,
      cues: sound,
      alerts,
      log() {},
    });
    const playback = createScenarioPlayback(scenario, "healthy", () => {});
    const finished = playback.play();
    await vi.advanceTimersByTimeAsync(0);
    expect(playback.status.value).toBe("playing");
    await vi.advanceTimersByTimeAsync(2500);
    expect(playback.status.value).toBe("playing");
    await vi.advanceTimersByTimeAsync(2500);
    await finished;
    expect(playback.status.value).toBe("finished");
    scenario.dispose();
    alerts.dispose();
  });

  it.each(kinds)("finishes the %s scenario through production modules", async (name) => {
    vi.useFakeTimers();
    const sound = cues();
    const alerts = createCaptureAlerts(sound);
    const scenario = createLabScenario({
      name,
      manual: true,
      intervalMs: 5000,
      deadlineMs: 2000,
      cues: sound,
      alerts,
      log() {},
    });
    const playback = createScenarioPlayback(scenario, name, () => {});
    const finished = playback.play();
    await vi.runAllTimersAsync();
    await finished;
    expect(playback.status.value).toBe("finished");
    playback.dispose();
    scenario.dispose();
    alerts.dispose();
  });
});
