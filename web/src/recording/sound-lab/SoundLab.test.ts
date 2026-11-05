import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { settle } from "../../test-support/manual-scheduler.ts";
import SoundLab from "./SoundLab.vue";

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
function mountLab(props = {}) {
  const wrapper = mount(SoundLab, { props });
  wrappers.push(wrapper);
  return wrapper;
}
async function click(wrapper: ReturnType<typeof mount>, text: string) {
  const button = wrapper.findAll("button").find((button) => button.text() === text);
  if (!button) throw new Error(`Missing button: ${text}`);
  await button.trigger("click");
  await settle();
  await flushPromises();
}
describe("sound laboratory workbench", () => {
  it("plays a complete arranged scenario, pauses, resumes, and replays without Advance actions", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] });
    const wrapper = mountLab({
      scenarioName: "stop",
      clockMode: "automatic",
      playbackSpeed: 5,
      intervalSeconds: 2,
    });
    await flushPromises();
    await click(wrapper, "Play scenario");
    expect(wrapper.get('[data-testid="capture-indicator"]').attributes("data-capturing")).toBe(
      "true",
    );
    expect(wrapper.findAll("button").some((button) => button.text().startsWith("Advance"))).toBe(
      false,
    );
    await vi.advanceTimersByTimeAsync(80);
    expect(wrapper.get(".scenario-playback").text()).toContain("0.4 simulated seconds");
    await click(wrapper, "Pause scenario");
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.get(".scenario-playback").text()).toContain("0.4 simulated seconds");
    await click(wrapper, "Resume scenario");
    await vi.advanceTimersByTimeAsync(700);
    expect(wrapper.get(".scenario-playback").text()).toContain("Scenario finished");
    expect(wrapper.text()).toContain("Recording complete");
    await click(wrapper, "Replay scenario");
    expect(wrapper.get(".scenario-playback").text()).toContain("0.0 simulated seconds");
    expect(wrapper.findAll(".activity li").some((entry) => entry.text().includes("q2"))).toBe(true);
  });
  it("starts automatic clock advancement when the production Start button is used", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] });
    const wrapper = mountLab({ scenarioName: "single", clockMode: "automatic", playbackSpeed: 5 });
    await flushPromises();
    await wrapper.get('[data-testid="start"]').trigger("click");
    await flushPromises();
    await vi.advanceTimersByTimeAsync(200);
    expect(wrapper.get(".scenario-playback").text()).toContain("Scenario playing");
    expect(wrapper.get(".scenario-playback").text()).toContain("1.0 simulated seconds");
  });
  it("starts with Production and resolves an overdue alert by its frame identity", async () => {
    const wrapper = mountLab({ scenarioName: "single" });
    await flushPromises();
    expect(wrapper.find<HTMLInputElement>('input[type="range"]').element.value).toBe("1");
    expect(wrapper.findAll("select").map((select) => select.element.value)).toEqual([
      "production",
      "triangle-cadence",
      "morse-stop",
      "four-note-triangle-cadence",
    ]);
    expect(
      wrapper
        .findAll<HTMLInputElement>('.roles input[type="range"]')
        .map((input) => input.element.value),
    ).toEqual(["1", "1", "1"]);
    expect(
      wrapper
        .findAll("button")
        .find((button) => button.text() === "Secure overdue frame")
        ?.attributes("disabled"),
    ).toBeDefined();
    await wrapper.get('[data-testid="start"]').trigger("click");
    await flushPromises();
    await click(wrapper, "Advance one frame interval");
    await click(wrapper, "Advance deadline");
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(true);
    expect(
      wrapper
        .findAll("button")
        .find((button) => button.text() === "Secure overdue frame")
        ?.attributes("disabled"),
    ).toBeUndefined();
    expect(
      wrapper
        .findAll("button")
        .filter((button) => button.text().startsWith("Advance"))
        .every((button) => button.attributes("disabled") !== undefined),
    ).toBe(true);
    expect(
      wrapper.findAll("button").some((button) => button.text() === "Advance three frame intervals"),
    ).toBe(false);
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(true);
    await click(wrapper, "Secure overdue frame");
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(false);
  });
  it("keeps the single overdue alert active for listening before arranged recovery", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] });
    const wrapper = mountLab({
      scenarioName: "single",
      clockMode: "automatic",
      intervalSeconds: 10,
    });
    await flushPromises();
    await click(wrapper, "Play scenario");
    await vi.advanceTimersByTimeAsync(12_000);
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(true);
    expect(wrapper.get(".scenario-playback").text()).toContain("12.0 simulated seconds");
    await vi.advanceTimersByTimeAsync(5_200);
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(false);
    await vi.advanceTimersByTimeAsync(4_100);
    expect(wrapper.get(".scenario-playback").text()).toContain("Scenario finished");
    expect(wrapper.text()).toContain("Recording complete");
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain("Capture stopped. Up to 15 seconds");
  });
  it("recovers a five-second Single Overdue Frame before backlog protection stops Capture", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] });
    const wrapper = mountLab({
      scenarioName: "single",
      clockMode: "automatic",
      intervalSeconds: 5,
    });
    await flushPromises();
    await click(wrapper, "Play scenario");
    await vi.advanceTimersByTimeAsync(7_000);
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(true);
    await vi.advanceTimersByTimeAsync(3_000);
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(false);
    await vi.advanceTimersByTimeAsync(2_200);
    expect(wrapper.text()).toContain("Recording complete");
    const history = wrapper.findAll(".activity li").map((entry) => entry.text());
    expect(history.some((entry) => entry.includes("unrequestedCaptureStopped"))).toBe(false);
    expect(
      history.filter((entry) => entry.includes("sound: four-note-triangle-cadence")),
    ).toHaveLength(1);
    const recovery = history.find((entry) => entry.includes("sound: triangle-cadence"));
    const completion = history.find((entry) => entry.includes("sound: four-note-triangle-cadence"));
    const at = (entry: string | undefined) =>
      Number(entry?.match(/ · ([\d.]+)s · /)?.[1] ?? Number.NaN);
    expect(at(completion) - at(recovery)).toBeGreaterThanOrEqual(2);
    await vi.advanceTimersByTimeAsync(1_800);
    expect(wrapper.get(".scenario-playback").text()).toContain("Scenario finished");
  });
  it("auditions Problem through production Capture-alert wiring", async () => {
    const wrapper = mountLab();
    await flushPromises();
    const roleButtons = wrapper.findAll(".roles article button");
    await roleButtons[1]!.trigger("click");
    expect(wrapper.find(".problem-preview").exists()).toBe(true);
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(true);
    expect(
      wrapper.findAll("button").filter((button) => button.text() === "Acknowledge"),
    ).toHaveLength(1);
    await roleButtons[0]!.trigger("click");
    await roleButtons[2]!.trigger("click");
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(true);
    await click(wrapper, "Acknowledge");
    expect(wrapper.find(".problem-preview").exists()).toBe(false);
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(false);
  });
  it("reports invalid timing visibly before constructing a scenario", async () => {
    const wrapper = mountLab({ securingDeadlineMs: 400 });
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("between 500 and 2000 ms");
    expect(wrapper.find('[data-testid="start"]').exists()).toBe(false);
  });
  it("stages a recovery burst through adapter timing while Capture remains active", async () => {
    const wrapper = mountLab({ scenarioName: "burst" });
    await flushPromises();
    await wrapper.findAll("select")[1]!.setValue("closing-bell");
    await wrapper.get('[data-testid="start"]').trigger("click");
    await flushPromises();
    await click(wrapper, "Advance three frame intervals");
    const entries = wrapper.findAll(".activity li").map((entry) => entry.text());
    expect(entries.filter((entry) => entry.includes("sound: closing-bell"))).toHaveLength(3);
    expect(wrapper.findAll("select")[3]!.element.value).toBe("four-note-triangle-cadence");
    expect(entries.some((entry) => entry.includes("event: transportFrameOverdue f0"))).toBe(true);
    expect(
      entries.some(
        (entry) => entry.includes("9.97s") && entry.includes("event: transportFrameSecured f0"),
      ),
    ).toBe(true);
    expect(
      entries.some(
        (entry) => entry.includes("10.01s") && entry.includes("event: transportFrameSecured f1"),
      ),
    ).toBe(true);
    expect(wrapper.get('[data-testid="capture-indicator"]').attributes("data-capturing")).toBe(
      "true",
    );
  });
  it("rebuilds timing while retaining the selected palette, overrides, and numbered history", async () => {
    const fetch = vi.fn(() => {
      throw new Error("Unexpected live request");
    });
    vi.stubGlobal("fetch", fetch);
    const wrapper = mountLab();
    await settle();
    await flushPromises();
    const selects = wrapper.findAll("select");
    await selects[0]!.setValue("soft");
    await selects[1]!.setValue("wood-tap");
    await wrapper.setProps({ intervalSeconds: 2, securingDeadlineMs: 500 });
    await settle();
    await flushPromises();
    expect(wrapper.findAll("select")[0]!.element.value).toBe("soft");
    expect(wrapper.findAll("select")[1]!.element.value).toBe("wood-tap");
    expect(wrapper.text()).toContain("Experimental frame interval: 2s (production: 5s)");
    const history = wrapper.findAll(".activity li");
    expect(history[0]!.text()).toContain("q1");
    expect(history.at(-1)!.text()).toContain("q2");
    expect(history.map((entry) => Number(entry.attributes("value")))).toEqual(
      history.map((_, i) => i + 1),
    );
    expect(fetch).not.toHaveBeenCalled();
    await click(wrapper, "Reset to selected palette defaults");
    expect(wrapper.findAll("select")[1]!.element.value).toBe("felt-pulse");
  });
  it("runs the production UI over simulated adapters with explicit backend confirmation", async () => {
    const fetch = vi.fn(() => {
      throw new Error("Unexpected live request");
    });
    vi.stubGlobal("fetch", fetch);
    const wrapper = mountLab({ scenarioName: "stop" });
    await settle();
    await flushPromises();
    await wrapper.get('[data-testid="start"]').trigger("click");
    await settle();
    await flushPromises();
    expect(wrapper.get('[data-testid="capture-indicator"]').attributes("data-capturing")).toBe(
      "true",
    );
    await click(wrapper, "Advance 100 ms");
    await wrapper.get('[data-testid="start"]').trigger("click");
    await settle();
    await flushPromises();
    await click(wrapper, "Advance 100 ms");
    expect(wrapper.text()).toContain("Completing recording…");
    await click(wrapper, "Confirm simulated sealing");
    expect(wrapper.text()).toContain("Recording complete");
    expect(fetch).not.toHaveBeenCalled();
  });
  it("keeps a Capture alert and history across scenario rebuilds, with sound unavailable", async () => {
    const wrapper = mountLab({ scenarioName: "unavailable", simulatePlaybackFailure: true });
    await settle();
    await flushPromises();
    await click(wrapper, "Audition Problem");
    expect(wrapper.get('[data-testid="capture-alerts"]').text()).toContain(
      "Problem sound is unavailable.",
    );
    await wrapper.setProps({ intervalSeconds: 1 });
    await settle();
    await flushPromises();
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(true);
    await click(wrapper, "Acknowledge");
    expect(wrapper.find('[data-testid="capture-alerts"]').exists()).toBe(false);
    expect(wrapper.findAll(".activity li").some((entry) => entry.text().includes("q1"))).toBe(true);
  });
});
