import { configureAxe } from "vitest-axe";

/** Axe for isolated components: page-level landmark rules do not apply to a fragment, and jsdom
 * cannot compute contrast (style.test.ts checks the design tokens instead). */
export const axe = configureAxe({
  rules: { region: { enabled: false }, "color-contrast": { enabled: false } },
});
