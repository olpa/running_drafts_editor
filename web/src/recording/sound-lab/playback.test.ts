import { flushPromises } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { createCaptureStopAlarm } from "../../alarm/capture-stop-alarm.ts";
import { ManualScheduler } from "../../test-support/manual-scheduler.ts";
import type { Cues } from "../cues.ts";
import { createScenarioPlayback } from "./playback.ts";
import { createLabScenario, scenarios, type ScenarioName } from "./scenario.ts";

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});
function harness(name: ScenarioName, speed = 5) {
  const wall = new ManualScheduler();
  const sounds: string[] = [];
  const log: string[] = [];
  const errors: unknown[] = [];
  const cues: Cues = {
    prepare() {},
    play(cue) {
      sounds.push(cue);
    },
    loopInterrupted() {
      sounds.push("alarm");
      return () => {};
    },
    dispose() {},
  };
  const alarm = createCaptureStopAlarm(cues);
  const scenario = createLabScenario({
    name,
    manual: true,
    intervalMs: 2000,
    deadlineMs: 500,
    cues,
    alarm,
    log: (message) => log.push(message),
  });
  let rate = speed;
  const player = createScenarioPlayback(
    scenario,
    name,
    () => rate,
    (message) => log.push(message),
    (error) => errors.push(error),
    wall,
  );
  cleanups.push(() => {
    player.dispose();
    scenario.dispose();
    alarm.dispose();
  });
  async function elapse(ms: number) {
    for (let left = ms; left > 0; left -= 40) {
      await wall.advanceBy(Math.min(left, 40));
      await flushPromises();
    }
  }
  return {
    scenario,
    player,
    sounds,
    errors,
    log,
    wall,
    elapse,
    rate(value: number) {
      rate = value;
    },
  };
}

describe("arranged scenario playback", () => {
  it("advances smoothly, pauses, resumes, changes speed, and cancels work on disposal", async () => {
    const h = harness("healthy");
    await h.player.play();
    await h.elapse(200);
    expect(h.player.positionMs.value).toBe(1000);
    h.player.pause();
    await h.elapse(400);
    expect(h.player.positionMs.value).toBe(1000);
    h.rate(2);
    await h.player.play();
    await h.elapse(200);
    expect(h.player.positionMs.value).toBe(1400);
    h.player.dispose();
    const entries = h.log.length;
    await h.elapse(2000);
    expect(h.log).toHaveLength(entries);
    expect(h.wall.pendingCount).toBe(0);
    expect(h.errors).toEqual([]);
  });
  it.each((Object.keys(scenarios) as ScenarioName[]).filter((name) => name !== "palette"))(
    "runs %s from Capture start through the arranged final state",
    async (name) => {
      const h = harness(name, 10);
      await h.player.play();
      await h.elapse(2000);
      expect(h.player.status.value).toBe("finished");
      expect(h.scenario.app.snapshot.value.capturing).toBe(false);
      expect(h.errors).toEqual([]);
      expect(h.sounds).toContain("start");
      expect(h.sounds).toContain("stop");
      if (name === "older" || name === "repeated")
        expect(h.scenario.app.snapshot.value.sections).toHaveLength(2);
      if (["duration", "coexist", "repeated", "failure", "two", "unavailable"].includes(name))
        expect(h.sounds).toContain("alarm");
      const before = h.player.positionMs.value;
      await h.elapse(1000);
      expect(h.player.positionMs.value).toBe(before);
    },
  );
});
