import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import ProfileAllowanceStory from "./ProfileAllowanceStory.vue";

describe("Profile allowance stories", () => {
  it("renders the complete production Profile with a fixed usage snapshot", async () => {
    const wrapper = mount(ProfileAllowanceStory, { props: { state: "pro" } });
    await flushPromises();

    expect(wrapper.text()).toContain("Profile");
    expect(wrapper.text()).toContain("Maya Chen");
    expect(wrapper.text()).toContain("Plan");
    expect(wrapper.text()).toContain("Pro");
    expect(wrapper.text()).not.toContain("Upgrade");
    expect(wrapper.find('[data-testid="allowance-status"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("shows a silent visual when the loaded snapshot is already at warning", async () => {
    const wrapper = mount(ProfileAllowanceStory, { props: { state: "warning" } });
    await flushPromises();

    expect(wrapper.get('[data-testid="allowance-status"]').text()).toContain(
      "Audio allowance is nearly full.",
    );
    expect(
      wrapper.get("#usage-summary-title").element.nextElementSibling?.getAttribute("data-testid"),
    ).toBe("allowance-status");
    expect(wrapper.text()).not.toContain("Stop warning sound");
    expect(wrapper.get('[data-testid="story-log"]').text()).not.toContain("unexpected cue");
    wrapper.unmount();
  });
});
