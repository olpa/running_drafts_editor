import assert from "node:assert/strict";
import test from "node:test";
import { Window, type HTMLButtonElement as HappyButtonElement,
  type HTMLElement as HappyHTMLElement } from "happy-dom";
import type { AccountId } from "../src/session.ts";

const browser = new Window({ url: "https://editor.test/" });
(browser as unknown as { __BACKEND_BASE_URL__: string }).__BACKEND_BASE_URL__ =
  "https://api.example.test";
Object.assign(globalThis, {
  window: browser, document: browser.document, location: browser.location,
  history: browser.history, HTMLElement: browser.HTMLElement,
  HTMLButtonElement: browser.HTMLButtonElement, customElements: browser.customElements,
  CustomEvent: browser.CustomEvent, Event: browser.Event,
});

await import("../src/components/seed-app.ts");

async function settle(): Promise<void> {
  for (let index = 0; index < 20; index++) await Promise.resolve();
}

test("signed-in user reaches Projects through visible navigation and can return to recording", async () => {
  const paths: string[] = [];
  const accountId = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;
  const request = async (input: string | URL | Request): Promise<Response> => {
    const path = new URL(String(input)).pathname;
    paths.push(path);
    if (path === "/api/v1/auth/providers") {
      return Response.json({ providers: [{ id: "fixture", displayName: "Fixture" }] });
    }
    if (path === "/api/v1/me") {
      return Response.json({
        account: { id: accountId }, provider: { id: "fixture", displayName: "Fixture" },
        profile: { email: null, emailVerified: null, displayName: "Fixture User",
          givenName: null, familyName: null, pictureUrl: null, locale: null },
        sessionExpiresAt: new Date(Date.now() + 8 * 60 * 60 * 1_000).toISOString(),
      });
    }
    if (path === "/api/v1/projects") return Response.json({ projects: [] });
    throw new Error(`Unexpected request: ${path}`);
  };
  globalThis.fetch = request as typeof fetch;
  browser.fetch = request as unknown as typeof browser.fetch;
  browser.document.body.replaceChildren();

  const app = browser.document.createElement("seed-app") as HappyHTMLElement;
  browser.document.body.append(app);
  await settle();

  const recording = app.querySelector<HappyHTMLElement>("[data-testid=recording-workspace]")!;
  const projects = app.querySelector<HappyHTMLElement>("[data-testid=projects-workspace]")!;
  const navigation = app.querySelector<HappyHTMLElement>("[data-testid=private-navigation]")!;
  assert.equal(navigation.hidden, false);
  assert.equal(recording.hidden, false);
  assert.equal(projects.hidden, true);

  app.querySelector<HappyButtonElement>("[data-testid=projects-view]")!.click();
  await settle();
  assert.equal(recording.hidden, true);
  assert.equal(projects.hidden, false);
  assert.ok(paths.includes("/api/v1/projects"));

  app.querySelector<HappyButtonElement>("[data-testid=record-view]")!.click();
  assert.equal(recording.hidden, false);
  assert.equal(projects.hidden, true);
});
