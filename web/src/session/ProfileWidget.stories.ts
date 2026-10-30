import type { Meta, StoryObj } from "@storybook/vue3-vite";
import ProfileWidgetStory from "./ProfileWidgetStory.vue";

const HOUR_MS = 60 * 60 * 1000;

const meta = {
  component: ProfileWidgetStory,
  tags: ["autodocs"],
  args: {
    initialState: "signedIn",
    displayName: "Maya Chen",
    verifiedEmail: "maya.chen@example.com",
    accountId: "5728cdd2-909d-4207-8303-c04b270c308f",
    providerKey: "google",
    sessionExpiresAt: Date.now() + 8 * HOUR_MS,
  },
  argTypes: {
    initialState: { control: false },
    displayName: { control: "text", name: "Display name" },
    verifiedEmail: { control: "text", name: "Verified email" },
    accountId: { control: "text", name: "Account ID" },
    providerKey: { control: "text", name: "Provider key" },
    sessionExpiresAt: { control: "date", name: "Session expiry" },
  },
} satisfies Meta<typeof ProfileWidgetStory>;

export default meta;
type Story = StoryObj<typeof meta>;

export const SignedIn: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "Change the profile or session Controls, then activate Log out. Inspect the production signed-out widget and the numbered gateway and Session activity in the bounded log. Activate Sign in with Google to inspect the recorded fake navigation.",
      },
    },
  },
};

export const SignedOut: Story = {
  args: { initialState: "signedOut" },
  parameters: {
    controls: { disable: true },
    docs: {
      description: {
        story:
          "Inspect the production signed-out profile counterpart directly. Activate Sign in with Google and inspect the fake login URL and navigation in the bounded activity log.",
      },
    },
  },
};
