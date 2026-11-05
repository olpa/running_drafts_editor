import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { expect, it } from "vitest";
import { axe } from "../test-support/axe.ts";
import CaptureAlertsView from "./CaptureAlerts.vue";
import { captureAlertsKey, createCaptureAlerts } from "./capture-alerts.ts";

it("shows active Capture problems and acknowledges all of them", async () => {
  let unavailable = () => {};
  const alerts = createCaptureAlerts({
    loopProblem(onUnavailable) {
      unavailable = onUnavailable;
      return () => {};
    },
  });
  const wrapper = mount(CaptureAlertsView, {
    global: { provide: { [captureAlertsKey as symbol]: alerts } },
  });

  alerts.raise({
    kind: "transport-frame-overdue",
    sectionId: "section-one",
    frameId: 4,
    elapsedSeconds: 2,
  });
  alerts.raise({ kind: "unrequested-capture-stop", sectionId: "section-one" });
  unavailable();
  await nextTick();

  expect(wrapper.get("[role=alert]").text()).toContain("Saving audio is delayed by 2 seconds.");
  expect(wrapper.get("[role=alert]").text()).toContain(
    "Capture stopped. Up to 15 seconds of recent audio may not be saved.",
  );
  expect(wrapper.text()).toContain("Problem sound is unavailable.");
  expect(wrapper.get("button").text()).toBe("Acknowledge");
  expect(wrapper.find("[autofocus]").exists()).toBe(false);
  expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();

  await wrapper.get("button").trigger("click");
  expect(alerts.alerts.value).toEqual([]);
  expect(wrapper.find("[role=alert]").exists()).toBe(false);
});
