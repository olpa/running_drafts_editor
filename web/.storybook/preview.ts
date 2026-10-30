import type { Preview } from "@storybook/vue3-vite";
import "../src/style.css";

const preview: Preview = {
  parameters: {
    controls: { expanded: true },
  },
};

export default preview;
