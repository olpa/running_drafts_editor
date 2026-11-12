import type { Preview } from "@storybook/vue3-vite";
import "../src/style.css";

const preview: Preview = {
  globalTypes: {
    captureReviewMode: {
      description: "Capture story presentation",
      toolbar: {
        title: "Capture view",
        icon: "circlehollow",
        items: [
          { value: "experience", title: "Experience" },
          { value: "workbench", title: "Workbench" },
        ],
        dynamicTitle: true,
      },
    },
  },
  initialGlobals: {
    captureReviewMode: "experience",
  },
  parameters: {
    controls: { expanded: true },
  },
};

export default preview;
