import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(process.cwd(), "src/style.css"), "utf8");

function token(name: string): string {
  const match = new RegExp(`--${name}:\\s*(#[0-9a-f]{6})\\s*;`, "iu").exec(css);
  if (!match) throw new Error(`Missing colour token --${name}`);
  return match[1]!;
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((start) => {
    const channel = parseInt(hex.slice(start, start + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

function contrast(foreground: string, background: string): number {
  const [light, dark] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (light! + 0.05) / (dark! + 0.05);
}

describe("design tokens meet WCAG 2.2 AA", () => {
  // Text pairs used by ProfileSummary and the shared button styles need 4.5:1.
  it.each([
    ["text", "panel"],
    ["muted", "panel"],
    ["accent-dark", "panel"],
    ["text", "bg"],
    ["muted", "bg"],
  ])("--%s on --%s has contrast of at least 4.5:1", (foreground, background) => {
    expect(contrast(token(foreground), token(background))).toBeGreaterThanOrEqual(4.5);
  });

  it("primary button text on the accent colours has contrast of at least 4.5:1", () => {
    expect(contrast("#ffffff", token("accent"))).toBeGreaterThanOrEqual(4.5);
    expect(contrast("#ffffff", token("accent-dark"))).toBeGreaterThanOrEqual(4.5);
  });

  // Non-text contrast: the focus indicator must stand out from what it surrounds.
  it.each(["panel", "bg"])("--focus-ring on --%s has contrast of at least 3:1", (background) => {
    expect(contrast(token("focus-ring"), token(background))).toBeGreaterThanOrEqual(3);
  });

  it("styles keyboard focus for buttons and links", () => {
    expect(css).toMatch(
      /button:focus-visible[^{]*\{[^}]*outline:\s*3px solid var\(--focus-ring\)/u,
    );
    expect(css).toMatch(/a:focus-visible/u);
  });
});
