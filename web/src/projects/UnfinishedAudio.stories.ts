import type { Meta, StoryObj } from "@storybook/vue3-vite";
import { defineComponent, h, provide, type PropType } from "vue";
import sampleAudioUrl from "../../e2e/fixtures/check-english.webm?url";
import { ApiError } from "../shared/http.ts";
import type { AudioAssetSummary } from "../shared/project-types.ts";
import ProjectSectionCard from "./ProjectSectionCard.vue";
import { createProjects, projectsKey } from "./projects.ts";

type Scenario =
  | "recent"
  | "eligible"
  | "sealed"
  | "zero"
  | "recheckFailed"
  | "confirmationFailed"
  | "signIn"
  | "unavailable";

const recent = "2026-11-25T12:00:15Z";
const eligible = "2026-11-25T12:01:00Z";

function seal(asset: AudioAssetSummary): AudioAssetSummary {
  return { ...asset, isSealed: true, sealReason: "user_resolved" };
}

const Review = defineComponent({
  props: {
    scenario: { type: String as PropType<Scenario>, required: true },
  },
  setup(props) {
    const scenario = props.scenario;
    const zero = scenario === "zero";
    const sample: AudioAssetSummary = {
      id: "unfinished-asset",
      mediaType: "audio/webm;codecs=opus",
      isSealed: false,
      sealReason: null,
      createdAt: "2026-11-25T12:00:00Z",
      updatedAt: "2026-11-25T12:00:00Z",
      frameCount: zero ? 0 : 1,
      observedDurationMs: zero ? 0 : 5381,
      storedBytes: zero ? 0 : 17486,
    };
    const initial = scenario === "sealed" ? seal(sample) : sample;
    let current = initial;
    let readAttempts = 0;
    let confirmationAttempts = 0;
    const unused = async () => {
      throw new Error("Unexpected request in the unfinished-audio story.");
    };
    provide(
      projectsKey,
      createProjects(
        {
          listProjects: unused,
          getProjectDetail: unused,
          getAudioAsset: async (id) => {
            if (id !== current.id) return unused();
            if (scenario === "signIn") throw new ApiError("GET", id, 401);
            if (scenario === "unavailable") throw new ApiError("GET", id, 404);
            if (scenario === "recheckFailed" && readAttempts++ === 0) {
              throw new ApiError("GET", id, 503);
            }
            return { audioAsset: current, serverTime: eligible };
          },
          confirmEnded: async (id, count) => {
            if (id !== current.id || count !== current.frameCount || current.isSealed) {
              return unused();
            }
            if (scenario === "confirmationFailed" && confirmationAttempts++ === 0) {
              throw new ApiError("POST", id, 503);
            }
            current = seal(current);
            return current;
          },
          audioDownloadUrl: () => sampleAudioUrl,
          resolvePlaybackUrl: async (id) => {
            if (id !== current.id || zero) return unused();
            return sampleAudioUrl;
          },
          probePlayback: unused,
        },
        undefined,
        { signInUrl: () => "#preview-sign-in" },
      ),
    );
    return () =>
      h("ol", { style: { listStyle: "none", padding: 0, maxWidth: "42rem" } }, [
        h(ProjectSectionCard, {
          section: { id: "section", position: 0, audioAsset: initial },
          serverTime:
            scenario === "eligible" || scenario === "confirmationFailed" ? eligible : recent,
        }),
      ]);
  },
});

const meta = {
  title: "Projects/Unfinished audio",
  component: Review,
  render: (args) => ({
    setup: () => () => h(Review, { ...args, key: args.scenario }),
  }),
  argTypes: { scenario: { control: false, table: { disable: true } } },
} satisfies Meta<typeof Review>;

export default meta;
type Story = StoryObj<typeof meta>;

export const RecentActivity: Story = {
  args: { scenario: "recent" },
  parameters: {
    docs: {
      description: {
        story:
          "Play the sample if you like, then choose Check again and Confirm recording has ended. The confirmation box disappears. Open the Section actions to download.",
      },
    },
  },
};

export const ReadyToConfirm: Story = { args: { scenario: "eligible" } };
export const Sealed: Story = { args: { scenario: "sealed" } };
export const RecentZeroFrameAsset: Story = { args: { scenario: "zero" } };
const showRecheckResult: NonNullable<Story["play"]> = async ({ canvas, userEvent }) => {
  await userEvent.click(canvas.getByRole("button", { name: "Check again" }));
};

export const RecheckFails: Story = {
  args: { scenario: "recheckFailed" },
  play: showRecheckResult,
};
export const ConfirmationFails: Story = {
  args: { scenario: "confirmationFailed" },
  play: async ({ canvas, userEvent }) => {
    await userEvent.click(canvas.getByRole("button", { name: "Confirm recording has ended" }));
  },
};
export const SignInRequired: Story = {
  args: { scenario: "signIn" },
  play: showRecheckResult,
};
export const AudioUnavailable: Story = {
  args: { scenario: "unavailable" },
  play: showRecheckResult,
};
