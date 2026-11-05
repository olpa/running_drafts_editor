import type { Meta, StoryObj } from "@storybook/vue3-vite";
import SoundLab from "./SoundLab.vue";
import { scenarios, type ScenarioName } from "./scenario.ts";
const meta = {
  title: "Capture/Sound laboratory",
  component: SoundLab,
  args: {
    scenarioName: "palette",
    clockMode: "automatic",
    playbackSpeed: 1,
    intervalSeconds: 5,
    securingDeadlineMs: 2000,
    simulatePlaybackFailure: false,
  },
  argTypes: {
    scenarioName: { control: false },
    clockMode: {
      control: "radio",
      options: ["automatic", "real-time", "manual"],
      name: "Review clock",
    },
    playbackSpeed: {
      control: { type: "range", min: 1, max: 10, step: 1 },
      name: "Automatic playback speed",
      if: { arg: "clockMode", eq: "automatic" },
    },
    intervalSeconds: {
      control: { type: "range", min: 1, max: 20, step: 1 },
      name: "Simulated frame interval (seconds; production: 5)",
    },
    securingDeadlineMs: {
      control: { type: "range", min: 500, max: 2000, step: 100 },
      name: "Securing deadline (ms)",
    },
    simulatePlaybackFailure: { control: "boolean", name: "Simulate unavailable audio" },
  },
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "Click Play scenario to enable sound and listen to the arranged sequence. Automatic review advances the injected clock and performs real production actions; manual review retains the Advance controls. Environmental adapters never reach a live service. Real-time playback uses 1× speed; Accelerated Healthy Dictation defaults to 5×. Problem audio keeps real time. The real-time frame interval is a local exception to ADR 0002's three-second wait limit. Record findings in the issue requesting the current experiment; Storybook stores none.",
      },
    },
  },
} satisfies Meta<typeof SoundLab>;
export default meta;
type Story = StoryObj<typeof meta>;
function scenario(
  name: ScenarioName,
  clockMode: "automatic" | "manual" | "real-time" = "automatic",
): Story {
  return {
    args: { scenarioName: name, clockMode },
    parameters: { docs: { description: { story: scenarios[name] } } },
  };
}
export const SoundPalette: Story = scenario("palette", "manual");
export const AllFramesSecured: Story = scenario("healthy", "real-time");
export const AcceleratedHealthyDictation: Story = {
  ...scenario("accelerated"),
  args: { scenarioName: "accelerated", playbackSpeed: 5 },
};
export const SingleOverdueFrame: Story = scenario("single");
export const TwoOverdueFrames: Story = scenario("two");
export const RecoveryBurst: Story = scenario("burst");
export const UserStopAndSuccessfulSeal: Story = scenario("stop");
export const FinalFrameDelayed: Story = scenario("final");
export const UnrequestedDurationStop: Story = scenario("duration");
export const CaptureFailure: Story = scenario("failure");
export const SealingDuringCaptureAlert: Story = scenario("coexist");
export const OlderSectionCompletes: Story = scenario("older");
export const RepeatedStopEvents: Story = scenario("repeated");
export const AudioPlaybackUnavailable: Story = {
  ...scenario("unavailable"),
  args: { ...scenario("unavailable").args, simulatePlaybackFailure: true },
};
