import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { axe } from "../test-support/axe.ts";
import type { AccountId, CurrentSession } from "./gateway.ts";
import ProfileSummary from "./ProfileSummary.vue";
import { resolveProvider } from "./providers.ts";
import { sessionKey, type Session, type SessionState } from "./session.ts";
import { shallowRef } from "vue";

const ACCOUNT_ID = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;

function sessionData(overrides: Partial<CurrentSession["profile"]> = {}): CurrentSession {
  return {
    account: { id: ACCOUNT_ID },
    provider: resolveProvider("google"),
    profile: {
      email: "runner@example.test",
      emailVerified: true,
      displayName: "Runner Example",
      givenName: null,
      familyName: null,
      pictureUrl: null,
      locale: null,
      ...overrides,
    },
    sessionExpiresAt: "2026-10-21T15:30:00Z",
  };
}

/** A stand-in for the application session, as Storybook will provide it later. */
function stubSession(state: SessionState): { session: Session; logout: ReturnType<typeof vi.fn> } {
  const logout = vi.fn(async () => undefined);
  const current = shallowRef(state);
  const session = {
    state: current,
    providers: [],
    logout,
  } as unknown as Session;
  return { session, logout };
}

function mountSummary(state: SessionState) {
  const { session, logout } = stubSession(state);
  const wrapper = mount(ProfileSummary, {
    attachTo: document.body,
    global: { provide: { [sessionKey as symbol]: session } },
  });
  return { wrapper, logout };
}

describe("ProfileSummary", () => {
  it("shows the identity, full Account ID, provider, and local expiry", () => {
    const { wrapper } = mountSummary({ status: "signedIn", session: sessionData() });
    const text = wrapper.text();

    expect(text).toContain("Runner Example");
    expect(text).toContain("runner@example.test");
    expect(wrapper.get("dl").text()).toContain("Account ID");
    expect(wrapper.get("code").text()).toBe(ACCOUNT_ID);
    expect(text).toContain("Google");

    const time = wrapper.get("time");
    expect(time.attributes("datetime")).toBe("2026-10-21T15:30:00Z");
    expect(time.text()).toBe(
      new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(
        new Date("2026-10-21T15:30:00Z"),
      ),
    );
  });

  it("shows an unverified or missing email as absent", () => {
    for (const profile of [
      { emailVerified: false },
      { emailVerified: null },
      { email: null, emailVerified: true },
    ]) {
      const { wrapper } = mountSummary({ status: "signedIn", session: sessionData(profile) });
      expect(wrapper.text()).not.toContain("Email");
      expect(wrapper.text()).not.toContain("runner@example.test");
    }
  });

  it("shows an unknown provider key as received", () => {
    const session = { ...sessionData(), provider: resolveProvider("microsoft") };
    const { wrapper } = mountSummary({ status: "signedIn", session });
    expect(wrapper.text()).toContain("microsoft");
  });

  it("renders nothing when signed out", () => {
    const { wrapper } = mountSummary({
      status: "signedOut",
      message: "Signed out.",
      logoutUnconfirmed: false,
      loginAllowed: true,
      referenceId: null,
    });
    expect(wrapper.html()).toBe("<!--v-if-->");
  });

  it("logs out through a button, not a link", async () => {
    const { wrapper, logout } = mountSummary({ status: "signedIn", session: sessionData() });
    expect(wrapper.find("a").exists()).toBe(false);
    const button = wrapper.get("button");
    expect(button.text()).toBe("Log out");
    expect(button.attributes("type")).toBe("button");
    expect(button.classes()).toContain("link-button");
    await button.trigger("click");
    expect(logout).toHaveBeenCalledTimes(1);
  });

  it("is a labelled region whose Log out button is reachable by keyboard", () => {
    const { wrapper } = mountSummary({ status: "signedIn", session: sessionData() });
    const region = wrapper.get("section");
    const heading = wrapper.get("h2");
    expect(region.attributes("aria-labelledby")).toBe(heading.attributes("id"));

    // A native button is in the tab order and answers Enter and Space by itself.
    const button = wrapper.get("button").element as HTMLButtonElement;
    expect(button.tabIndex).toBe(0);
    button.focus();
    expect(document.activeElement).toBe(button);
  });

  it("has no detectable accessibility violations", async () => {
    const { wrapper } = mountSummary({ status: "signedIn", session: sessionData() });
    expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();
  });
});
