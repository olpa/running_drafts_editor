import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

// The backend is mocked by the browser test harness (#76); #78 adds the
// same-origin /api proxy to the real backend.
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test-setup.ts"],
  },
});
