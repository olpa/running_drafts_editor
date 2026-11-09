import type { Meta, StoryObj } from "@storybook/vue3-vite";
import CaptureScenarioWorkbench from "./CaptureScenarioWorkbench.vue";
const meta = {
  title: "Capture/Delivery delay and recovery",
  component: CaptureScenarioWorkbench,
  parameters: { layout: "fullscreen" },
  argTypes: { scenarioKind: { control: false } },
} satisfies Meta<typeof CaptureScenarioWorkbench>;
export default meta;
type Story = StoryObj<typeof meta>;
const interaction =
  "Production controls are inactive. Acknowledge may stop Problem sound; Replay restores the arranged state.";
export const SingleOverdueFrameRecovers: Story = {
  args: {
    scenarioKind: "single",
    initial: "The first Transport frame will miss its securing deadline.",
    play: "Raises its identified alert, secures it, then stops and seals cleanly.",
    inspect: "The overdue alert resolves and Capture completes.",
    interaction,
  },
};
export const BacklogStopAfterTwoOverdueFrames: Story = {
  args: {
    scenarioKind: "two",
    initial: "Two Transport frames will remain unsecured through the backlog limit.",
    play: "Advances through both deadlines, stops Capture, and recovers retained delivery.",
    inspect: "Recovered frame state with the Unrequested Capture stop alert still present.",
    interaction,
  },
};
export const RecoveryBurst: Story = {
  args: {
    scenarioKind: "burst",
    initial: "Two delayed frames are arranged to secure close together.",
    play: "Runs Capture through the clustered acknowledgements, then stops and seals.",
    inspect: "Independent overlapping Healthy cues and clean completion.",
    interaction,
  },
};
export const FinalFrameDelayed: Story = {
  args: {
    scenarioKind: "final",
    initial: "User Stop will produce a delayed final partial frame.",
    play: "Stops, lets the final frame become overdue, recovers it, and seals.",
    inspect: "The alert resolves without a Healthy heartbeat.",
    interaction,
  },
};
