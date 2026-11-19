import type { Meta, StoryObj } from "@storybook/vue3-vite";
import LiveMicrophoneCapture from "./LiveMicrophoneCapture.vue";

const meta = {
  title: "Capture/Microphone level",
  component: LiveMicrophoneCapture,
  parameters: {
    layout: "fullscreen",
    captureReviewMode: "workbench",
  },
} satisfies Meta<typeof LiveMicrophoneCapture>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LiveMicrophone: Story = {};
