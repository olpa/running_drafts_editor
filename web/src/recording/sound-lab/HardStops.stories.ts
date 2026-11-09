import type { Meta, StoryObj } from "@storybook/vue3-vite";
import CaptureScenarioWorkbench from "./CaptureScenarioWorkbench.vue";
const meta = {
  title: "Capture/Hard stops",
  component: CaptureScenarioWorkbench,
  parameters: { layout: "fullscreen" },
  argTypes: { scenarioKind: { control: false } },
} satisfies Meta<typeof CaptureScenarioWorkbench>;
export default meta;
type Story = StoryObj<typeof meta>;
export const BackendDurationRejection: Story = {
  args: {
    scenarioKind: "duration",
    initial: "The backend will reject a Transport frame at the duration allowance.",
    play: "Starts Capture and advances to the authoritative backend rejection.",
    inspect: "Capture is stopped with exact pending audio, Section error, and global alert.",
    interaction:
      "Production controls are inactive. Acknowledge may stop Problem sound; Replay restores the arranged state.",
  },
};
