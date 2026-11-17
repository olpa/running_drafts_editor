import type { Meta, StoryObj } from "@storybook/vue3-vite";
import AudioPlayerStory from "./AudioPlayerStory.vue";

const meta = {
  title: "Projects/Audio player",
  component: AudioPlayerStory,
  args: {
    authError: false,
    missing: false,
    overload: false,
    serverFailure: false,
    networkFailure: false,
    invalidMedia: false,
    slowInitialLoad: false,
    prolongedBuffering: false,
  },
  argTypes: {
    authError: { control: "boolean", name: "Auth error" },
    missing: { control: "boolean", name: "Missing audio" },
    overload: { control: "boolean", name: "Server overload" },
    serverFailure: { control: "boolean", name: "Server failure" },
    networkFailure: { control: "boolean", name: "Network failure" },
    invalidMedia: { control: "boolean", name: "Invalid media" },
    slowInitialLoad: { control: "boolean", name: "Slow initial load" },
    prolongedBuffering: { control: "boolean", name: "Prolonged buffering" },
  },
  parameters: {
    docs: {
      description: {
        component:
          "Review the production Section audio player against a real same-origin range mock. The mock repeats a short English voice recording inside a valid indexed one-hour WebM and caps every 206 response to about five seconds of bytes. The story accelerates the production 15-second loading and buffering thresholds to one second and bounds simulated HTTP delay at two seconds. Controls update the mock server without replacing the active production player; enable one failure at a time, then activate or continue playback to inspect the transition and bounded application activity log. Inspect native media requests in the browser Network panel.",
      },
    },
  },
} satisfies Meta<typeof AudioPlayerStory>;

export default meta;
type Story = StoryObj<typeof meta>;

export const ReviewWorkbench: Story = {};

// The packet pattern read by .storybook/audio-story-server.ts was generated with:
// ffmpeg -i e2e/fixtures/check-english.wav -c:a libopus -b:a 24k \
//   -application voip -ac 1 -ar 48000 -f webm e2e/fixtures/check-english.webm
