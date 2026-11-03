import { readonly, shallowRef } from "vue";
import { browserScheduler, type Scheduler } from "../frame-timing.ts";
import { settle } from "../../test-support/manual-scheduler.ts";
import type { LabScenario, ScenarioName } from "./scenario.ts";

type Status = "ready" | "playing" | "paused" | "finished";
interface Step {
  at: number;
  label: string;
  run(): void | Promise<void>;
}

/** A reviewer-initiated performance of real product actions over the injected simulation clock. */
export function createScenarioPlayback(
  scenario: LabScenario,
  name: ScenarioName,
  speed: () => number,
  log: (message: string) => void,
  onError: (error: unknown) => void,
  wallClock: Scheduler = browserScheduler,
) {
  const status = shallowRef<Status>("ready");
  const positionMs = shallowRef(0);
  const pending = new Set<() => void>();
  let cancelTick: (() => void) | null = null;
  let lastWall = 0;
  let startedAt = 0;
  let duration = 0;
  let disposed = false;
  let generation = 0;
  function pause(): void {
    if (status.value !== "playing") return;
    status.value = "paused";
    generation++;
    cancelTick?.();
    cancelTick = null;
    log("Scenario paused; repeating sounds retain real audio time until Acknowledge.");
  }
  function scheduleTick(): void {
    const current = generation;
    cancelTick = wallClock.schedule(40, () => {
      cancelTick = null;
      if (disposed || status.value !== "playing" || current !== generation) return;
      const now = wallClock.now();
      const delta = Math.min(Math.max(0, now - lastWall) * speed(), duration - positionMs.value);
      lastWall = now;
      void scenario
        .advance(delta, true)
        .then(() => {
          if (disposed) return;
          positionMs.value = scenario.scheduler.now() - startedAt;
          if (status.value === "playing" && current === generation) scheduleTick();
        })
        .catch((error) => {
          pause();
          onError(error);
        });
    });
  }
  async function play(): Promise<void> {
    if (disposed || status.value === "playing" || status.value === "finished") return;
    if (!scenario.manual)
      throw new Error("Scenario playback requires the injected simulation clock.");
    const fresh = status.value === "ready";
    status.value = "playing";
    generation++;
    if (fresh) {
      if (!scenario.app.snapshot.value.capturing) scenario.app.toggle();
      await settle();
      if (disposed || status.value !== "playing") return;
      if (!scenario.app.snapshot.value.capturing) {
        pause();
        throw new Error("Simulated Capture did not start.");
      }
      startedAt = scenario.scheduler.now();
      const plan = stepsFor(scenario, name);
      duration = plan.duration;
      for (const step of plan.steps) {
        const cancel = scenario.scheduler.schedule(step.at, () => {
          pending.delete(cancel);
          if (disposed) return;
          log(`Scenario action: ${step.label}`);
          void Promise.resolve(step.run()).catch((error) => {
            pause();
            onError(error);
          });
        });
        pending.add(cancel);
      }
      const cancel = scenario.scheduler.schedule(duration, () => {
        pending.delete(cancel);
        status.value = "finished";
        cancelTick?.();
        cancelTick = null;
        log("Scenario finished. Its clock is paused; active alerts still require Acknowledge.");
      });
      pending.add(cancel);
    }
    if (disposed || status.value !== "playing") return;
    lastWall = wallClock.now();
    log(
      `Scenario playback ${fresh ? "started" : "resumed"} at ${speed()}×. Actions run automatically.`,
    );
    scheduleTick();
  }
  return {
    status: readonly(status),
    positionMs: readonly(positionMs),
    play,
    pause,
    dispose() {
      disposed = true;
      generation++;
      cancelTick?.();
      cancelTick = null;
      for (const cancel of pending) cancel();
      pending.clear();
    },
  };
}
export type ScenarioPlayback = ReturnType<typeof createScenarioPlayback>;

function stepsFor(scenario: LabScenario, name: ScenarioName): { duration: number; steps: Step[] } {
  const interval = scenario.config.frameIntervalMs;
  const deadline = scenario.config.secureDeadlineMs;
  const limit = scenario.config.maxRecordingMs;
  const shortCapture = Math.min(2000, interval / 2);
  const steps: Step[] = [];
  const action = (at: number, label: string, run: Step["run"]) => steps.push({ at, label, run });
  const stop = () => {
    if (scenario.app.snapshot.value.capturing) scenario.app.toggle();
  };
  const start = () => {
    if (!scenario.app.snapshot.value.capturing) scenario.app.toggle();
  };
  let duration: number;
  switch (name) {
    case "palette":
    case "healthy":
    case "accelerated":
      action(3 * interval + 300, "Stop after three secured frames", stop);
      duration = 3 * interval + 2200;
      break;
    case "single":
      action(interval + deadline + 2000, "Stop after the overdue frame recovers", stop);
      duration = interval + deadline + 4000;
      break;
    case "two":
      action(2 * interval + deadline + 2000, "Recover the two delayed frames", scenario.recover);
      duration = 2 * interval + deadline + 4000;
      break;
    case "burst":
      action(2 * interval + 500, "Stop after the recovery burst", stop);
      duration = 2 * interval + 2500;
      break;
    case "stop":
      action(shortCapture, "User Stop", stop);
      action(shortCapture + 600, "Backend confirms sealing", scenario.confirmSealing);
      duration = shortCapture + 2400;
      break;
    case "final":
      action(shortCapture, "User Stop; final frame is held", stop);
      action(shortCapture + deadline + 1200, "Recover final-frame delivery", scenario.recover);
      action(shortCapture + deadline + 1500, "Backend confirms sealing", scenario.confirmSealing);
      duration = shortCapture + deadline + 3500;
      break;
    case "duration":
    case "unavailable":
      duration = limit + 2000;
      break;
    case "failure":
      action(
        interval + deadline + 1500,
        "Recover retained audio after recorder failure",
        scenario.recover,
      );
      duration = interval + deadline + 3500;
      break;
    case "coexist":
      action(
        limit + 500,
        "Backend confirms sealing while the alarm remains active",
        scenario.confirmSealing,
      );
      duration = limit + 2500;
      break;
    case "older":
      action(shortCapture, "Stop the older Section", stop);
      action(shortCapture + 600, "Start a newer Section", start);
      action(
        shortCapture + 1200,
        "Seal the older Section during newer Capture",
        scenario.confirmSealing,
      );
      action(shortCapture + 2400, "Stop the newer Section", stop);
      duration = shortCapture + 4400;
      break;
    case "repeated":
      action(limit + 500, "Start another Section without acknowledging the alarm", start);
      duration = 2 * limit + 2500;
      break;
  }
  return { steps, duration };
}
