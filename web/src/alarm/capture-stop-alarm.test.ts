import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { describe, expect, it, vi } from "vitest";
import { axe } from "../test-support/axe.ts";
import { createCaptureStopAlarm, captureStopAlarmKey } from "./capture-stop-alarm.ts";
import CaptureStopAlarm from "./CaptureStopAlarm.vue";

function harness(fail = false) {
  const cleanup = vi.fn();
  let unavailable: () => void = () => {};
  const loop = vi.fn((onUnavailable: () => void) => {
    unavailable = onUnavailable;
    if (fail) throw new Error("AudioContext unavailable");
    return cleanup;
  });
  const play = vi.fn();
  const log = vi.fn();
  const alarm = createCaptureStopAlarm(
    { prepare() {}, play, loopInterrupted: loop, dispose() {} },
    log,
  );
  return { alarm, cleanup, loop, play, log, unavailable: () => unavailable() };
}

describe("Capture stop alarm", () => {
  it("starts one loop, ignores repeated stops, and never depends on visibility", () => {
    const h = harness();
    h.alarm.activate();
    h.alarm.activate();
    document.dispatchEvent(new Event("visibilitychange"));
    window.dispatchEvent(new Event("blur"));
    expect(h.loop).toHaveBeenCalledOnce();
    expect(h.log).toHaveBeenCalledOnce();
    expect(h.cleanup).not.toHaveBeenCalled();
    expect(h.alarm.active.value).toBe(true);
    h.alarm.acknowledge();
    expect(h.alarm.active.value).toBe(false);
    expect(h.cleanup).toHaveBeenCalledOnce();
    expect(h.play).not.toHaveBeenCalled();
    h.alarm.activate();
    expect(h.loop).toHaveBeenCalledTimes(2);
    h.alarm.dispose();
    expect(h.cleanup).toHaveBeenCalledTimes(2);
    // Disposal releases resources, without domain acknowledgement.
    expect(h.alarm.active.value).toBe(true);
    h.alarm.activate();
    expect(h.loop).toHaveBeenCalledTimes(2);
  });
  it("keeps the alarm usable after audio failure and ignores late failure callbacks", () => {
    const h = harness();
    h.alarm.activate();
    h.unavailable();
    expect(h.alarm.soundUnavailable.value).toBe(true);
    h.alarm.acknowledge();
    h.unavailable();
    expect(h.alarm.soundUnavailable.value).toBe(false);
    const broken = harness(true);
    broken.alarm.activate();
    expect(broken.alarm.active.value).toBe(true);
    expect(broken.alarm.soundUnavailable.value).toBe(true);
    broken.alarm.acknowledge();
    expect(broken.alarm.active.value).toBe(false);
  });
  it("renders the fixed alert without autofocus or a sound-enable button", async () => {
    const h = harness(true);
    const wrapper = mount(CaptureStopAlarm, {
      global: { provide: { [captureStopAlarmKey as symbol]: h.alarm } },
    });
    h.alarm.activate();
    await nextTick();
    expect(wrapper.get("[role=alert]").text()).toContain(
      "Capture stopped. Up to 30 seconds of recent audio may not be saved.",
    );
    expect(wrapper.text()).toContain("Alarm sound is unavailable.");
    expect(wrapper.findAll("button")).toHaveLength(1);
    expect(wrapper.get("button").text()).toBe("Acknowledge");
    expect(wrapper.find("[autofocus]").exists()).toBe(false);
    expect(document.activeElement).not.toBe(wrapper.get("button").element);
    expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();
    await wrapper.get("button").trigger("click");
    expect(wrapper.find("[role=alert]").exists()).toBe(false);
    wrapper.unmount();
  });
});
