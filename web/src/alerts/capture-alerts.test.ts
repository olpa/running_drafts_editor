import { expect, it, vi } from "vitest";
import { captureAlertId, createCaptureAlerts } from "./capture-alerts.ts";

it("plays one Problem loop while independently identified Capture alerts remain", () => {
  const stop = vi.fn();
  const loopProblem = vi.fn(() => stop);
  const alerts = createCaptureAlerts({ loopProblem });
  const first = {
    kind: "transport-frame-overdue" as const,
    sectionId: "section-one",
    frameId: 3,
    elapsedSeconds: 2,
  };
  const second = {
    kind: "audio-asset-sealing-failed" as const,
    sectionId: "section-one",
  };

  alerts.raise(first);
  alerts.raise(first);
  alerts.raise(second);

  expect(alerts.alerts.value).toEqual([first, second]);
  expect(loopProblem).toHaveBeenCalledOnce();

  alerts.resolve(captureAlertId(first));
  expect(alerts.alerts.value).toEqual([second]);
  expect(stop).not.toHaveBeenCalled();

  alerts.resolve(captureAlertId(second));
  expect(alerts.alerts.value).toEqual([]);
  expect(stop).toHaveBeenCalledOnce();
});

it("acknowledges without ID history and ignores late audio failure after cleanup", () => {
  const stops: ReturnType<typeof vi.fn>[] = [];
  const unavailable: (() => void)[] = [];
  const alerts = createCaptureAlerts({
    loopProblem(onUnavailable) {
      unavailable.push(onUnavailable);
      const stop = vi.fn();
      stops.push(stop);
      return stop;
    },
  });
  const alert = { kind: "audio-asset-sealing-failed" as const, sectionId: "section-one" };

  alerts.raise(alert);
  alerts.acknowledgeAll();
  unavailable[0]!();
  expect(alerts.alerts.value).toEqual([]);
  expect(alerts.soundUnavailable.value).toBe(false);
  expect(stops[0]).toHaveBeenCalledOnce();

  alerts.raise(alert);
  expect(alerts.alerts.value).toEqual([alert]);
  expect(stops).toHaveLength(2);
  alerts.dispose();
  unavailable[1]!();
  expect(alerts.alerts.value).toEqual([alert]);
  expect(alerts.soundUnavailable.value).toBe(false);
  expect(stops[1]).toHaveBeenCalledOnce();
  alerts.raise({ kind: "unrequested-capture-stop", sectionId: "section-two" });
  expect(alerts.alerts.value).toEqual([alert]);
});

it("keeps alerts visible when Problem playback cannot start", () => {
  const alerts = createCaptureAlerts({
    loopProblem() {
      throw new Error("AudioContext unavailable");
    },
  });
  alerts.raise({ kind: "capture-finalization-failed", sectionId: "section-one" });
  expect(alerts.alerts.value).toHaveLength(1);
  expect(alerts.soundUnavailable.value).toBe(true);
});
