import type { Meta, StoryObj } from "@storybook/vue3-vite";
import SoundConfiguration from "./SoundConfiguration.vue";

const meta = {
  title: "Capture/Sound configuration",
  component: SoundConfiguration,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof SoundConfiguration>;

export default meta;
type Story = StoryObj<typeof meta>;
export const ConfigureSounds: Story = {};
