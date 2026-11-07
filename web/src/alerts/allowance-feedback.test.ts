import { describe, expect, it } from "vitest";
import { ManualScheduler } from "../test-support/manual-scheduler.ts";
import { createAllowanceFeedback } from "./allowance-feedback.ts";

describe("AllowanceFeedback", () => {
  it("plays independent bounded warnings and supports one silence action", async () => {
    const scheduler = new ManualScheduler();
    let starts = 0;
    let stops = 0;
    const feedback = createAllowanceFeedback(
      {
        loopProblem() {
          throw new Error("The Problem semantic role must not be used.");
        },
        loopAllowanceWarning() {
          starts++;
          return () => stops++;
        },
      },
      scheduler,
    );

    feedback.play("storage");
    feedback.play("duration");
    feedback.play("storage");
    expect(feedback.active.value).toBe(true);
    expect(starts).toBe(2);
    await scheduler.advanceBy(2_999);
    expect(stops).toBe(0);
    feedback.silence();
    expect(feedback.active.value).toBe(false);
    expect(stops).toBe(2);
    await scheduler.advanceBy(10_000);
    expect(stops).toBe(2);
  });
});
