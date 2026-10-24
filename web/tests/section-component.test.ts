import assert from "node:assert/strict";
import test from "node:test";
import { Window } from "happy-dom";
import type { SectionSnapshot } from "../src/recording.ts";

const browser = new Window({ url: "https://editor.test/" });
Object.assign(globalThis, { window: browser, document: browser.document });

const { SectionComponent } = await import("../src/components/section-component.ts");

function snapshot(overrides: Partial<SectionSnapshot> = {}): SectionSnapshot {
  return {
    sectionId: "section-one", audioAssetId: "asset-one", position: 0,
    capturing: false, finalizingCapture: false, elapsedMs: 10_000, securedMs: 0,
    pendingMs: 0, pendingFrames: 0, hasUnsecuredAudio: false,
    completion: null, completionProblem: "", canRetryCompletion: false,
    problem: "", canUploadPending: false, closeProblem: "", canRetryClose: false,
    ...overrides,
  };
}

test("renders a semantic Section and updates its completion lifecycle in place", () => {
  const component = new SectionComponent(snapshot(), {
    uploadPendingAudio: () => undefined, retryClose: () => undefined,
  });
  const root = component.element;
  const liveStatus = root.querySelector('[role="status"]');

  assert.equal(root.localName, "section");
  assert.equal(root.dataset.testid, "section");
  assert.equal(root.hasAttribute("data-completion"), false);
  assert.equal(root.querySelector("h3")?.textContent, "Section 1");

  component.update(snapshot({ completion: "pending", pendingMs: 2_000, pendingFrames: 1,
    hasUnsecuredAudio: true }));
  assert.equal(component.element, root);
  assert.equal(root.querySelector('[role="status"]'), liveStatus);
  assert.equal(root.dataset.completion, "pending");
  assert.match(root.textContent!, /2 seconds of audio pending/u);
  assert.match(root.textContent!, /Completing recording…/u);

  component.update(snapshot({ completion: "succeeded" }));
  assert.equal(root.dataset.completion, "succeeded");
  assert.equal(root.querySelector('[role="status"]'), liveStatus);
  assert.match(root.textContent!, /Recording complete/u);
  assert.equal(liveStatus?.classList.contains("success"), true);
});

test("separates a retryable completion problem from its action", () => {
  const retried: string[] = [];
  const component = new SectionComponent(snapshot({ completion: "failed",
    completionProblem: "Could not complete recording.", canRetryCompletion: true }), {
    uploadPendingAudio: () => undefined, retryClose: (sectionId) => retried.push(sectionId),
  });

  const retry = [...component.element.querySelectorAll("button")]
    .find((button) => button.textContent === "Try again")!;
  assert.equal(component.element.dataset.completion, "failed");
  assert.equal(component.element.querySelector('[role="status"]')?.textContent,
    "Could not complete recording.");
  assert.equal(retry.hidden, false);
  assert.equal(retry.getAttribute("aria-label"), "Try completing Section 1 again");
  retry.click();
  assert.deepEqual(retried, ["section-one"]);
});

test("renders a terminal completion mismatch without offering retry", () => {
  const component = new SectionComponent(snapshot({ completion: "failed",
    completionProblem: "Recording was sealed as interrupted and cannot be completed." }), {
    uploadPendingAudio: () => undefined, retryClose: () => undefined,
  });

  assert.match(component.element.textContent!, /sealed as interrupted/u);
  assert.equal([...component.element.querySelectorAll("button")]
    .some((button) => !button.hidden), false);
});
