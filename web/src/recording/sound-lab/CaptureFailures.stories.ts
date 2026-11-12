import type { Meta, StoryObj } from "@storybook/vue3-vite";
import CaptureScenarioWorkbench from "./CaptureScenarioWorkbench.vue";
const meta = {
  title: "Capture/Failures",
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
export const RecorderFailure: Story = {
  args: {
    scenarioKind: "failure",
    initial: "The recorder will fail after Capture has produced audio.",
    play: "Stops Capture unexpectedly and recovers retained delivery through production modules.",
    inspect: "A sealed Section whose persistent Capture alert suppresses Completion.",
    interaction,
  },
};
export const CaptureFinalizationFailure: Story = {
  args: {
    scenarioKind: "finalization",
    initial: "User Stop will fail while collecting final captured audio.",
    play: "Starts Capture and arranges failure during finalization.",
    inspect: "The finalization error and Capture alert remain visible.",
    interaction,
  },
};
export const AudioAssetSealingFailure: Story = {
  args: {
    scenarioKind: "sealing",
    initial: "The backend will fail the first sealing attempt.",
    play: "Runs a short Capture through Stop and the failed sealing request.",
    inspect: "The retryable sealing error and Capture alert remain visible.",
    interaction,
  },
};
export const CaptureAlertsFromTwoSections: Story = {
  args: {
    scenarioKind: "repeated",
    initial: "Two successive Sections will suffer recorder failures.",
    play: "Starts each Capture and leaves both identified alerts active.",
    inspect: "Two Section alerts share one application Problem loop.",
    interaction,
  },
};
