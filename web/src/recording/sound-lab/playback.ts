import { readonly, shallowRef } from "vue";
import { settle } from "../../test-support/manual-scheduler.ts";
import type { LabScenario, ScenarioKind } from "./scenario.ts";

type Status = "ready" | "playing" | "finished";
interface Step {
  at: number;
  label: string;
  run(): void | Promise<void>;
}

/** One explicit arranged performance over injected time. */
export function createScenarioPlayback(
  scenario: LabScenario,
  name: ScenarioKind,
  log: (message: string) => void,
) {
  const status = shallowRef<Status>("ready");
  const positionMs = shallowRef(0);
  let disposed = false;
  let failure: unknown = null;
  let cancelWait: (() => void) | null = null;

  function waitReviewTime(ms: number): Promise<void> {
    return new Promise((resolve) => {
      const timer = globalThis.setTimeout(() => {
        cancelWait = null;
        resolve();
      }, ms);
      cancelWait = () => {
        globalThis.clearTimeout(timer);
        cancelWait = null;
        resolve();
      };
    });
  }
  async function play(): Promise<void> {
    if (disposed || status.value !== "ready") return;
    if (!scenario.manual) throw new Error("Scenario playback requires injected time.");
    status.value = "playing";
    log("scenario playback started");
    await settle();
    if (!scenario.app.snapshot.value.capturing) scenario.app.toggle();
    await settle();
    if (!scenario.app.snapshot.value.capturing) throw new Error("Simulated Capture did not start.");
    const plan = stepsFor(scenario, name);
    for (const step of plan.steps) {
      scenario.scheduler.schedule(step.at, () => {
        if (disposed) return;
        log(`arranged action: ${step.label}`);
        void Promise.resolve(step.run()).catch((error) => {
          failure = error;
        });
      });
    }
    const visibleTail = Math.min(plan.duration, 2 * scenario.config.frameIntervalMs);
    const fastForward = plan.duration - visibleTail;
    if (fastForward > 0) {
      await scenario.advance(fastForward, true);
      positionMs.value = fastForward;
      log(`fast-forwarded to final ${visibleTail / 1000} simulated seconds`);
    }
    while (!disposed && positionMs.value < plan.duration) {
      const delta = Math.min(scenario.config.frameIntervalMs, plan.duration - positionMs.value);
      await waitReviewTime(delta / 2);
      if (disposed) return;
      await scenario.advance(delta, true);
      positionMs.value += delta;
    }
    if (failure) throw failure;
    if (!disposed) {
      status.value = "finished";
      log("scenario playback finished");
    }
  }
  return {
    status: readonly(status),
    positionMs: readonly(positionMs),
    play,
    dispose() {
      disposed = true;
      cancelWait?.();
    },
  };
}
export type ScenarioPlayback = ReturnType<typeof createScenarioPlayback>;

function stepsFor(scenario: LabScenario, name: ScenarioKind): { duration: number; steps: Step[] } {
  const interval = scenario.config.frameIntervalMs;
  const deadline = scenario.config.secureDeadlineMs;
  const shortCapture = Math.min(2000, interval / 2);
  const steps: Step[] = [];
  const action = (at: number, label: string, run: Step["run"]) => steps.push({ at, label, run });
  const stop = () => {
    if (scenario.app.snapshot.value.capturing) scenario.app.toggle();
  };
  const start = () => {
    if (!scenario.app.snapshot.value.capturing) scenario.app.toggle();
  };
  let duration = 0;
  switch (name) {
    case "healthy":
      action(3 * interval + 300, "user Stop after three secured frames", stop);
      action(3 * interval + 2200, "backend confirms sealing", scenario.confirmSealing);
      duration = 3 * interval + 3000;
      break;
    case "short":
      action(shortCapture, "user Stop", stop);
      action(shortCapture + 600, "backend confirms sealing", scenario.confirmSealing);
      duration = shortCapture + 1200;
      break;
    case "single": {
      const recovered = Math.min(interval + deadline + 1000, 2 * interval - 200);
      action(recovered, "secure overdue frame", scenario.recover);
      action(recovered + 100, "user Stop", stop);
      action(recovered + 800, "backend confirms sealing", scenario.confirmSealing);
      duration = recovered + 1200;
      break;
    }
    case "two":
      action(2 * interval + deadline + 100, "recover retained frames", scenario.recover);
      duration = 2 * interval + deadline + 600;
      break;
    case "burst":
      action(2 * interval + 500, "user Stop after recovery burst", stop);
      action(2 * interval + 900, "backend confirms sealing", scenario.confirmSealing);
      duration = 2 * interval + 1300;
      break;
    case "final":
      action(shortCapture, "user Stop; final frame remains delayed", stop);
      action(shortCapture + deadline + 100, "recover final frame", scenario.recover);
      action(shortCapture + deadline + 400, "backend confirms sealing", scenario.confirmSealing);
      duration = shortCapture + deadline + 800;
      break;
    case "failure":
      action(interval + deadline + 600, "recover after recorder failure", scenario.recover);
      duration = interval + deadline + 1200;
      break;
    case "finalization":
      action(shortCapture - 1, "arrange Capture finalization failure", scenario.failFinalization);
      action(shortCapture, "user Stop", stop);
      duration = shortCapture + 400;
      break;
    case "sealing":
      action(shortCapture - 1, "arrange Audio Asset sealing failure", scenario.failNextSeal);
      action(shortCapture, "user Stop", stop);
      duration = shortCapture + 800;
      break;
    case "repeated":
      action(interval + 900, "start a second Section", start);
      duration = 2 * interval + 1600;
      break;
    case "allowance-storage-warning":
    case "allowance-duration-warning":
    case "allowance-overlap":
      duration = 2 * interval + 200;
      break;
    case "allowance-storage-rejection":
    case "allowance-duration-rejection":
      duration = 3 * interval + 500;
      break;
    case "allowance-duration-zero":
      duration = 3 * interval + 200;
      break;
    case "older":
      action(shortCapture, "Stop older Section", stop);
      action(shortCapture + 600, "start newer Section", start);
      action(shortCapture + 1200, "seal older Section", scenario.confirmSealing);
      action(shortCapture + 2400, "Stop newer Section", stop);
      duration = shortCapture + 3200;
      break;
  }
  return { duration, steps };
}
