import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { candidates } from "./candidates.ts";
import {
  assignCandidate,
  feedbackRoles,
  modifiedSoundAssignments,
  restoreProduction,
  selectedSoundAssignments,
} from "./sound-store.ts";

afterEach(() => {
  for (const role of feedbackRoles) restoreProduction(role);
});

describe("page-lifetime sound configuration", () => {
  it("assigns any retained candidate independently to all five CuePlayer meanings", () => {
    for (const role of feedbackRoles) assignCandidate(role, "wood-tap");
    expect(modifiedSoundAssignments()).toHaveLength(5);
    expect(
      Object.values(selectedSoundAssignments()).every((value) => value.id === "wood-tap"),
    ).toBe(true);
  });

  it("retains complete provenance and every bundled candidate asset", () => {
    expect(new Set(candidates.map((value) => value.id)).size).toBe(candidates.length);
    for (const value of candidates) {
      expect(value.author).toBeTruthy();
      expect(value.source).toBeTruthy();
      expect(value.license).toBeTruthy();
      expect(value.attribution).toBeTruthy();
      if (!value.asset) continue;
      const path = resolve(
        "src/recording/sound-lab/assets",
        `${value.id.replace("kenney-", "")}.ogg`,
      );
      expect(existsSync(path)).toBe(true);
      expect(readFileSync(path).subarray(0, 4).toString()).toBe("OggS");
    }
  });
});
