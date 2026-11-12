import type { Meta, StoryObj } from "@storybook/vue3-vite";
import CaptureScenarioWorkbench from "./CaptureScenarioWorkbench.vue";

const interaction =
  "Recording controls are inactive. Stop warning sound and Acknowledge remain available when shown; Replay restores the arranged state.";

const meta = {
  title: "Capture/Allowances",
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

export const StorageWarning: Story = {
  args: {
    scenarioKind: "allowance-storage-warning",
    initial: "The latest usage snapshot leaves one ordinary frame before the storage warning.",
    play: "Secures one healthy frame, then captures the frame that crosses the reported warning.",
    inspect: "The storage warning remains visible while Capture continues normally.",
    interaction,
  },
};

export const DurationWarning: Story = {
  args: {
    scenarioKind: "allowance-duration-warning",
    initial: "The reported duration warning is one healthy frame cycle away.",
    play: "Secures one healthy frame, then advances Capture across the duration warning.",
    inspect: "The remaining recording time and temporary warning-sound status are visible.",
    interaction,
  },
};

export const OverlappingWarnings: Story = {
  args: {
    scenarioKind: "allowance-overlap",
    initial: "Storage and duration will reach their reported warning thresholds together.",
    play: "Secures one healthy frame, then crosses both warning thresholds.",
    inspect: "Both remaining-capacity messages stay visible in one production warning surface.",
    interaction,
  },
};

export const BackendStorageRejection: Story = {
  args: {
    scenarioKind: "allowance-storage-rejection",
    initial: "One healthy frame and one warning frame precede a backend storage rejection.",
    play: "Advances Capture until the following whole frame is rejected by the backend.",
    inspect:
      "The Section keeps exact pending audio with safe Problem Details and the global stop alert.",
    interaction,
  },
};

export const BackendDurationRejection: Story = {
  args: {
    scenarioKind: "allowance-duration-rejection",
    initial: "One healthy frame and one warning frame precede a backend duration rejection.",
    play: "Advances Capture until the following whole frame is rejected by the backend.",
    inspect:
      "The Section keeps exact pending audio with safe Problem Details and the global stop alert.",
    interaction,
  },
};

export const DurationRemainingAtZero: Story = {
  args: {
    scenarioKind: "allowance-duration-zero",
    initial: "The reported duration limit can be reached before any backend frame rejection.",
    play: "Shows healthy Capture, crosses the warning, then advances to zero reported time remaining.",
    inspect: "The warning shows zero remaining while Capture is still active.",
    interaction,
  },
};
