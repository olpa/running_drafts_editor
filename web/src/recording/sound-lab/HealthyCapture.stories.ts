import type { Meta, StoryObj } from "@storybook/vue3-vite";
import CaptureScenarioWorkbench from "./CaptureScenarioWorkbench.vue";

const meta = {
  title: "Capture/Healthy and completion",
  component: CaptureScenarioWorkbench,
  parameters: { layout: "fullscreen" },
  argTypes: { scenarioKind: { control: false }, reviewMode: { control: false } },
  render: (args, context) => ({
    components: { CaptureScenarioWorkbench },
    setup: () => ({ args, reviewMode: context.globals.captureReviewMode }),
    template: `<CaptureScenarioWorkbench :key="reviewMode" v-bind="args" :review-mode="reviewMode" />`,
  }),
} satisfies Meta<typeof CaptureScenarioWorkbench>;
export default meta;
type Story = StoryObj<typeof meta>;
const interaction =
  "Production controls are inactive. Acknowledge may stop Problem sound; Replay restores the arranged state.";
export const HealthyCaptureAndCompletion: Story = {
  args: {
    scenarioKind: "healthy",
    initial: "A signed-in Account is ready to start a new Section.",
    play: "Starts Capture, secures three regular frames, stops, and seals the Audio Asset.",
    inspect: "Healthy progress cues followed by the completed Section state.",
    interaction,
  },
};
export const ShortCaptureAndCompletion: Story = {
  args: {
    scenarioKind: "short",
    initial: "A new Section is ready for a short Capture.",
    play: "Starts and stops before a regular frame boundary, then secures the final partial frame and seals.",
    inspect: "Completion without a Healthy heartbeat for the final partial frame.",
    interaction,
  },
};
export const OlderSectionCompletesDuringNewerCapture: Story = {
  args: {
    scenarioKind: "older",
    initial: "One Section can finish while a newer Section owns Capture.",
    play: "Stops the first Section, starts another, and seals the older Audio Asset during newer Capture.",
    inspect: "The older completion does not masquerade as progress for the active Section.",
    interaction,
  },
};
