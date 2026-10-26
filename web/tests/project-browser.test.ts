import assert from "node:assert/strict";
import test from "node:test";
import { Window, type HTMLAnchorElement as HappyAnchorElement,
  type HTMLButtonElement as HappyButtonElement,
  type HTMLElement as HappyHTMLElement } from "happy-dom";

const browser = new Window({ url: "https://editor.test/" });
(browser as unknown as { __BACKEND_BASE_URL__: string }).__BACKEND_BASE_URL__ =
  "https://api.example.test";
Object.assign(globalThis, {
  window: browser, document: browser.document, location: browser.location,
  history: browser.history, HTMLElement: browser.HTMLElement,
  HTMLButtonElement: browser.HTMLButtonElement, customElements: browser.customElements,
  CustomEvent: browser.CustomEvent, Event: browser.Event,
});

await import("../src/components/project-browser.ts");

interface ProjectBrowserElement extends HappyHTMLElement {
  showList(): Promise<void>;
}

async function settle(): Promise<void> {
  for (let index = 0; index < 20; index++) await Promise.resolve();
}

test("user finds a Project in backend order and opens its downloadable Section", async () => {
  const requests: { path: string; credentials: RequestCredentials | undefined }[] = [];
  const createdId = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
  const sectionId = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
  const assetId = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
  const responses = new Map<string, object>([
    ["/api/v1/projects", { projects: [
      { id: "dddddddd-dddd-4ddd-8ddd-dddddddddddd", title: "Newer Project",
        createdAt: "2026-10-26T10:01:00Z", updatedAt: "2026-10-26T10:02:00Z" },
      { id: createdId, title: "Canonical field notes",
        createdAt: "2026-10-26T10:00:00Z", updatedAt: "2026-10-26T10:01:00Z" },
    ] }],
    [`/api/v1/projects/${createdId}`, {
      id: createdId, title: "Canonical field notes",
      createdAt: "2026-10-26T10:00:00Z", updatedAt: "2026-10-26T10:01:00Z",
      document: { sections: [{
        id: sectionId, position: 0, audioAsset: {
          id: assetId, mediaType: "audio/webm;codecs=opus", isSealed: true,
          sealReason: "completed", sealedAt: "2026-10-26T10:01:00Z", frameCount: 3,
          observedDurationMs: 21_500, storedBytes: 123_456,
        },
      }] },
    }],
  ]);
  const request = async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
    const path = new URL(String(input)).pathname;
    requests.push({ path, credentials: init?.credentials });
    const payload = responses.get(path);
    if (!payload) throw new Error(`Unexpected request: ${path}`);
    return Response.json(payload);
  };
  globalThis.fetch = request as typeof fetch;
  browser.fetch = request as unknown as typeof browser.fetch;
  browser.document.body.replaceChildren();

  const projects = browser.document.createElement("project-browser") as ProjectBrowserElement;
  browser.document.body.append(projects);
  await projects.showList();

  const entries = [...projects.querySelectorAll<HappyHTMLElement>("[data-testid=project-list-item]")];
  assert.deepEqual(entries.map((entry) => entry.dataset.projectId), [
    "dddddddd-dddd-4ddd-8ddd-dddddddddddd", createdId,
  ]);
  const created = entries.find((entry) => entry.dataset.projectId === createdId)!;
  assert.match(created.textContent ?? "", /Canonical field notes/u);
  created.querySelector<HappyButtonElement>("button")!.click();
  await settle();

  const detail = projects.querySelector<HappyHTMLElement>("[data-testid=project-detail]")!;
  assert.equal(detail.dataset.projectId, createdId);
  assert.equal(detail.querySelector("h2")?.textContent, "Canonical field notes");
  const section = detail.querySelector<HappyHTMLElement>("[data-testid=project-section]")!;
  assert.equal(section.dataset.sectionId, sectionId);
  assert.equal(section.dataset.audioAssetId, assetId);
  assert.equal(section.dataset.sealed, "true");
  assert.equal(section.dataset.sealReason, "completed");
  const download = section.querySelector<HappyAnchorElement>("[data-testid=audio-download]")!;
  assert.equal(download.href, `https://api.example.test/api/v1/audio-assets/${assetId}/download`);
  assert.match(download.textContent ?? "", /Download original audio/u);
  assert.deepEqual(requests, [
    { path: "/api/v1/projects", credentials: "include" },
    { path: `/api/v1/projects/${createdId}`, credentials: "include" },
  ]);
});

test("failed Project loading offers a visible retry", async () => {
  let attempts = 0;
  const request = async (): Promise<Response> => {
    attempts++;
    return attempts === 1
      ? Response.json({ title: "Unavailable", code: "unavailable" }, { status: 503 })
      : Response.json({ projects: [] });
  };
  globalThis.fetch = request as typeof fetch;
  browser.fetch = request as unknown as typeof browser.fetch;
  browser.document.body.replaceChildren();
  const projects = browser.document.createElement("project-browser") as ProjectBrowserElement;
  browser.document.body.append(projects);

  await projects.showList();
  const retry = [...projects.querySelectorAll<HappyButtonElement>("button")]
    .find((candidate) => candidate.textContent === "Retry Projects");
  assert.ok(retry);
  retry.click();
  await settle();

  assert.equal(attempts, 2);
  assert.match(projects.textContent ?? "", /No Projects yet/u);
});
