import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import ProfileWidgetStory from "./ProfileWidgetStory.vue";

describe("ProfileWidget story", () => {
  it("provides production usage for the signed-in profile", async () => {
    const wrapper = mount(ProfileWidgetStory, {
      props: {
        initialState: "signedIn",
        displayName: "Maya Chen",
        verifiedEmail: "maya.chen@example.com",
        accountId: "5728cdd2-909d-4207-8303-c04b270c308f",
        providerKey: "google",
        sessionExpiresAt: Date.UTC(2099, 0, 1),
      },
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="usage-summary"]').text()).toContain("Free");
    expect(wrapper.text()).not.toContain("Storage grants");
    expect(wrapper.text()).not.toContain("Duration grants");
    wrapper.unmount();
  });
});
