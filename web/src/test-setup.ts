import { afterEach, expect } from "vitest";
import * as axeMatchers from "vitest-axe/matchers";

expect.extend(axeMatchers);

globalThis.scrollTo = () => undefined;

afterEach(() => {
  document.body.innerHTML = "";
});
