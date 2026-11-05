import { expect, it, vi } from "vitest";
import { captureAlertId, createCaptureAlerts } from "../alerts/capture-alerts.ts";
import { createCaptureFeedback } from "./capture-feedback.ts";

it("routes Section outcomes to independent cues and identified Capture alerts", () => {
  const play = vi.fn();
  const stopProblem = vi.fn();
  const alerts = createCaptureAlerts({ loopProblem: () => stopProblem });
  const feedback = createCaptureFeedback({ play }, alerts);

  feedback("section-one", { type: "captureStarted" });
  feedback("section-one", { type: "transportFrameOverdue", frameId: 2, elapsedSeconds: 2 });
  feedback("section-one", { type: "audioAssetSealingFailed", error: new Error("offline") });
  feedback("section-two", { type: "unrequestedCaptureStopped" });

  expect(play).toHaveBeenCalledWith("capture-started");
  expect(alerts.alerts.value.map(captureAlertId)).toEqual([
    '["section-one","transport-frame-overdue",2]',
    '["section-one","audio-asset-sealing-failed",null]',
    '["section-two","unrequested-capture-stop",null]',
  ]);

  feedback("section-one", {
    type: "transportFrameSecured",
    frameId: 2,
    captureActive: false,
    finalFrame: true,
  });
  expect(alerts.alerts.value.map(captureAlertId)).toEqual([
    '["section-one","audio-asset-sealing-failed",null]',
    '["section-two","unrequested-capture-stop",null]',
  ]);
  expect(play).not.toHaveBeenCalledWith("healthy-heartbeat");

  feedback("section-one", { type: "audioAssetSealed" });
  expect(play).toHaveBeenCalledWith("capture-completed");
  expect(alerts.alerts.value.map(captureAlertId)).toEqual([
    '["section-two","unrequested-capture-stop",null]',
  ]);
  expect(stopProblem).not.toHaveBeenCalled();
});

it.each(["captureFinalizationFailed", "unrequestedCaptureStopped"] as const)(
  "suppresses Completion while the same Section retains %s",
  (type) => {
    const play = vi.fn();
    const alerts = createCaptureAlerts({ loopProblem: () => vi.fn() });
    const feedback = createCaptureFeedback({ play }, alerts);

    if (type === "captureFinalizationFailed") {
      feedback("section-one", { type, reason: "user", error: new Error("failed") });
    } else {
      feedback("section-one", { type });
    }
    feedback("section-one", { type: "audioAssetSealingFailed", error: new Error("offline") });
    alerts.acknowledgeAll();
    feedback("section-one", { type: "audioAssetSealed" });

    expect(play).not.toHaveBeenCalledWith("capture-completed");
    expect(alerts.alerts.value).toHaveLength(0);
  },
);
