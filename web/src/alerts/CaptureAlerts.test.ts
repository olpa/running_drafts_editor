import { mount } from "@vue/test-utils";
import { nextTick, shallowRef } from "vue";
import { expect, it } from "vitest";
import {
  applicationCaptureKey,
  type ApplicationCapture,
} from "../recording/application-capture.ts";
import type { RecordingSnapshot } from "../recording/project.ts";
import { axe } from "../test-support/axe.ts";
import CaptureAlertsView from "./CaptureAlerts.vue";
import { captureAlertId, captureAlertsKey, createCaptureAlerts } from "./capture-alerts.ts";

it("shows active Capture problems and acknowledges all of them", async () => {
  let unavailable = () => {};
  const alerts = createCaptureAlerts({
    loopProblem(onUnavailable) {
      unavailable = onUnavailable;
      return () => {};
    },
  });
  const snapshot = shallowRef<RecordingSnapshot>({
    project: null,
    starting: false,
    canStart: false,
    capturing: true,
    elapsedMs: 15_000,
    hasUnsecuredAudio: true,
    hasPendingWork: true,
    startProblem: "",
    capturedBytes: 1,
    finalizingCapture: false,
    microphoneLevel: null,
    sections: [
      {
        sectionId: "section-one",
        audioAssetId: "asset-one",
        position: 0,
        capturing: true,
        finalizingCapture: false,
        elapsedMs: 15_000,
        securedMs: 5_000,
        pendingMs: 10_000,
        pendingFrames: 2,
        overdueFrames: [
          { frameId: 4, elapsedSeconds: 7 },
          { frameId: 5, elapsedSeconds: 3 },
        ],
        hasUnsecuredAudio: true,
        completion: null,
        completionProblem: "",
        canRetryCompletion: false,
        problem: "",
        diagnosticDetails: [],
        canUploadPending: false,
        closeProblem: "",
        canRetryClose: false,
        awaitingSeal: false,
        capturedBytes: 1,
        microphoneLevel: null,
      },
    ],
  });
  const recording = { snapshot } as ApplicationCapture;
  const wrapper = mount(CaptureAlertsView, {
    global: {
      provide: {
        [captureAlertsKey as symbol]: alerts,
        [applicationCaptureKey as symbol]: recording,
      },
    },
  });

  alerts.raise({
    kind: "transport-frame-overdue",
    sectionId: "section-one",
    frameId: 4,
    elapsedSeconds: 2,
  });
  alerts.raise({
    kind: "transport-frame-overdue",
    sectionId: "section-one",
    frameId: 5,
    elapsedSeconds: 2,
  });
  alerts.raise({ kind: "unrequested-capture-stop", sectionId: "section-one" });
  unavailable();
  await nextTick();

  expect(wrapper.get("[role=alert]").text()).toContain(
    "Saving audio is delayed. Capture continues.",
  );
  expect(wrapper.findAll("[data-testid=delay-message]")).toHaveLength(1);
  expect(wrapper.get("[data-testid=delay-details]").text()).toBe(
    "2 pieces pending · oldest delay 7 seconds",
  );
  expect(wrapper.get("[data-testid=delay-details]").attributes("aria-live")).toBe("off");
  expect(wrapper.get("[role=alert]").text()).toContain(
    "Capture stopped. Check the recording Section for retained audio and recovery details.",
  );
  expect(wrapper.text()).toContain("Problem sound is unavailable.");
  expect(wrapper.get("button").text()).toBe("Acknowledge");
  expect(wrapper.find("[autofocus]").exists()).toBe(false);
  expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();

  alerts.resolve(
    captureAlertId({ kind: "transport-frame-overdue", sectionId: "section-one", frameId: 4 }),
  );
  alerts.resolve(
    captureAlertId({ kind: "transport-frame-overdue", sectionId: "section-one", frameId: 5 }),
  );
  snapshot.value = {
    ...snapshot.value,
    capturing: false,
    hasUnsecuredAudio: false,
    hasPendingWork: false,
    sections: snapshot.value.sections.map((section) => ({
      ...section,
      capturing: false,
      pendingMs: 0,
      pendingFrames: 0,
      overdueFrames: [],
      hasUnsecuredAudio: false,
    })),
  };
  await nextTick();
  expect(wrapper.get("[role=alert]").text()).toContain(
    "Capture stopped unexpectedly. No audio remains pending.",
  );

  await wrapper.get("button").trigger("click");
  expect(alerts.alerts.value).toEqual([]);
  expect(wrapper.find("[role=alert]").exists()).toBe(false);
});
