import { mount } from "@vue/test-utils";
import { shallowRef } from "vue";
import { describe, expect, it, vi } from "vitest";
import { axe } from "../test-support/axe.ts";
import type { AccountId, CurrentSession } from "./gateway.ts";
import ProfileWidget from "./ProfileWidget.vue";
import { resolveProvider } from "./providers.ts";
import { sessionKey, type Session, type SessionState } from "./session.ts";

const ACCOUNT_ID = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;

function sessionData(): CurrentSession {
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
    },
    sessionExpiresAt: "2026-10-21T15:30:00Z",
  };
}

function mountWidget(initialState: SessionState) {
  const state = shallowRef(initialState);
  const login = vi.fn();
  const session = {
    state,
    providers: [resolveProvider("google")],
    login,
  } as unknown as Session;
  const wrapper = mount(ProfileWidget, {
    attachTo: document.body,
    global: { provide: { [sessionKey as symbol]: session } },
  });
  return { wrapper, state, login };
}

function signedOut(
  message: string,
  options: {
    loginAllowed?: boolean;
    logoutUnconfirmed?: boolean;
    referenceId?: string | null;
  } = {},
): Extract<SessionState, { status: "signedOut" }> {
  return {
    status: "signedOut",
    message,
    loginAllowed: options.loginAllowed ?? true,
    logoutUnconfirmed: options.logoutUnconfirmed ?? false,
    referenceId: options.referenceId ?? null,
  };
}

describe("ProfileWidget", () => {
  it("renders the production profile summary while signed in", () => {
    const { wrapper } = mountWidget({ status: "signedIn", session: sessionData() });
    expect(wrapper.get("h2").text()).toBe("Profile");
    expect(wrapper.text()).toContain("Runner Example");
    expect(wrapper.text()).toContain(ACCOUNT_ID);
    expect(wrapper.get("button").text()).toBe("Log out");
  });

  it.each(["Signed out.", "Sign in to continue."])(
    "shows the stable profile prompt for the routine message %s",
    (routineMessage) => {
      const { wrapper } = mountWidget(signedOut(routineMessage));
      expect(wrapper.get("h2").text()).toBe("Profile");
      expect(wrapper.text()).toContain("Sign in to view your profile.");
      expect(wrapper.text()).not.toContain(routineMessage);
    },
  );

  it("signs in through a catalog provider button", async () => {
    const { wrapper, login } = mountWidget(signedOut("Signed out."));
    const button = wrapper.get("button");
    expect(button.text()).toBe("Sign in with Google");
    expect(button.attributes("type")).toBe("button");
    expect(button.classes()).toContain("primary");

    const element = button.element as HTMLButtonElement;
    expect(element.tabIndex).toBe(0);
    element.focus();
    expect(document.activeElement).toBe(element);
    await button.trigger("click");
    expect(login).toHaveBeenCalledOnce();
    expect(login).toHaveBeenCalledWith("google");
  });

  it("shows exceptional signed-out information with login when allowed", () => {
    const { wrapper } = mountWidget(
      signedOut("Your session expired. Sign in again to continue.", {
        referenceId: "not-visible",
      }),
    );
    expect(wrapper.text()).toContain("Your session expired. Sign in again to continue.");
    expect(wrapper.text()).not.toContain("not-visible");
    expect(wrapper.get("button").text()).toBe("Sign in with Google");
  });

  it("shows pending and unconfirmed logout information without recovery actions", () => {
    const states = [
      signedOut("Signing out…", { loginAllowed: false }),
      signedOut("Private data was hidden, but server logout was not confirmed.", {
        loginAllowed: false,
        logoutUnconfirmed: true,
      }),
    ];
    for (const state of states) {
      const { wrapper } = mountWidget(state);
      expect(wrapper.text()).toContain(state.message);
      expect(wrapper.find("button").exists()).toBe(false);
    }
  });

  it("shows loading and failure information without recovery actions or reference IDs", () => {
    const loading = mountWidget({ status: "loading" }).wrapper;
    expect(loading.text()).toContain("Checking your session…");
    expect(loading.find("button").exists()).toBe(false);

    const failure = mountWidget({
      status: "failure",
      message: "Running Drafts could not check your session.",
      referenceId: "not-visible",
    }).wrapper;
    expect(failure.text()).toContain("Running Drafts could not check your session.");
    expect(failure.text()).not.toContain("not-visible");
    expect(failure.find("button").exists()).toBe(false);
  });

  it("announces state changes without moving focus automatically", async () => {
    const { wrapper, state } = mountWidget({ status: "signedIn", session: sessionData() });
    expect(wrapper.get(".profile-widget-state").attributes("aria-live")).toBe("polite");
    const outside = document.createElement("button");
    document.body.append(outside);
    outside.focus();

    state.value = signedOut("Signed out.");
    await wrapper.vm.$nextTick();

    expect(wrapper.text()).toContain("Sign in to view your profile.");
    expect(document.activeElement).toBe(outside);
  });

  it("has no detectable accessibility violations while signed out", async () => {
    const { wrapper } = mountWidget(signedOut("Signed out."));
    expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();
  });
});
