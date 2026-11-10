import type { Meta, StoryObj } from "@storybook/vue3-vite";
import ProfileAllowanceStory from "./ProfileAllowanceStory.vue";

const meta = {
  title: "Account/Profile/Allowances",
  component: ProfileAllowanceStory,
  parameters: { layout: "fullscreen" },
  argTypes: { state: { control: false } },
} satisfies Meta<typeof ProfileAllowanceStory>;
export default meta;
type Story = StoryObj<typeof meta>;

export const FreeUsage: Story = {
  args: { state: "free" },
  parameters: {
    docs: {
      description: { story: "Inspect the reported Free Plan usage and effective thresholds." },
    },
  },
};

export const ProUsage: Story = {
  args: { state: "pro" },
  parameters: {
    docs: {
      description: {
        story: "Inspect the reported Pro Plan usage and effective thresholds without Upgrade copy.",
      },
    },
  },
};

export const FreeWithGrants: Story = {
  args: { state: "grants" },
  parameters: {
    docs: {
      description: {
        story:
          "Inspect aggregate active storage and duration grants and their effective thresholds.",
      },
    },
  },
};

export const LoadedAtStorageWarning: Story = {
  args: { state: "warning" },
  parameters: {
    docs: {
      description: {
        story:
          "Inspect a page loaded at the reported storage warning threshold. It is visual and silent.",
      },
    },
  },
};
