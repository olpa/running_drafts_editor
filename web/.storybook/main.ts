import type { StorybookConfig } from "@storybook/vue3-vite";
import { audioStoryServer } from "./audio-story-server.ts";

const config: StorybookConfig = {
  stories: ["../src/**/*.stories.ts"],
  addons: ["@storybook/addon-a11y", "@storybook/addon-docs"],
  framework: {
    name: "@storybook/vue3-vite",
    options: { docgen: "vue-component-meta" },
  },
  viteFinal(config) {
    config.plugins ??= [];
    config.plugins.push(audioStoryServer());
    return config;
  },
};

export default config;
