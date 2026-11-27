import { statSync } from "node:fs";
import { resolve } from "node:path";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

// The backend is mocked by the browser test harness (#76); #78 adds the
// same-origin /api proxy to the real backend.
export default defineConfig({
  plugins: [
    vue(),
    {
      name: "running-drafts-policy-links",
      config(config) {
        const directory = config.publicDir;
        const publicRoot = resolve(config.root ?? ".", directory || "public");
        const pages = [
          { label: "Privacy", href: "/rd/legal/privacy-policy.html" },
          { label: "Terms", href: "/rd/legal/terms-and-conditions.html" },
          { label: "Impressum", href: "/legal/impressum.html" },
        ];
        const links =
          directory === false
            ? []
            : pages.filter(({ href }) =>
                statSync(resolve(publicRoot, "." + href), { throwIfNoEntry: false })?.isFile(),
              );
        return { define: { __RUNNING_DRAFTS_POLICY_LINKS__: JSON.stringify(links) } };
      },
    },
  ],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test-setup.ts"],
  },
});
