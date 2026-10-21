import template from "./seed-app-template.ts";
import "./recording-panel.ts";
import { loadConfig } from "../config.ts";
import { RadarLog } from "../radar-log.ts";
import type { RecordingDependencies, RecordingEvent } from "../recording.js";
import { authenticationCallbackFromSearch, SessionClient, SessionController,
  sessionAllowsRecording, type SessionState, withoutAuthenticationCallback } from "../session.ts";
import type { RecordingPanel } from "./recording-panel.js";

/** Composes the session boundary with recording without disconnecting retained audio. */
export class SeedApp extends HTMLElement {
  private readonly config = loadConfig();
  private readonly sessionClient = new SessionClient();
  private readonly session = new SessionController(this.sessionClient);
  private log!: RadarLog;
  private transcript!: HTMLOListElement;
  private logView!: HTMLPreElement;
  private authentication!: HTMLElement;
  private workspace!: HTMLElement;
  private privateResults!: HTMLElement;
  private recordingPanel!: RecordingPanel;
  private recordingDependencies: RecordingDependencies | null = null;

  /** Configure browser boundaries before connection, primarily for composition tests. */
  setRecordingDependencies(dependencies: RecordingDependencies): void {
    if (this.isConnected || this.transcript) throw new Error("Recording dependencies must be set before connection.");
    this.recordingDependencies = dependencies;
  }

  /** Re-read the authoritative Account after returning from authentication. */
  refreshSession(): Promise<void> {
    return this.session.bootstrap();
  }

  connectedCallback(): void {
    if (this.transcript) return;
    this.innerHTML = template;
    this.transcript = this.element("transcript");
    this.logView = this.element("log");
    this.authentication = this.element("authentication");
    this.workspace = this.element("recording-workspace");
    this.privateResults = this.element("private-results");
    this.recordingPanel = document.createElement("recording-panel") as RecordingPanel;
    if (this.recordingDependencies) this.recordingPanel.setDependencies(this.recordingDependencies);
    this.element("recording-panel-host").append(this.recordingPanel);
    this.resetLog();
    this.addEventListener("authentication-expired", () => this.session.authenticationExpired());
    this.addEventListener("recording-start-request", (event) => this.authorizeRecordingStart(event));
    this.recordingPanel.addEventListener("recording-event", (event) => this.handleRecordingEvent(event));
    this.session.subscribe((state) => this.renderSession(state));
    const callback = authenticationCallbackFromSearch(window.location.search);
    if (new URLSearchParams(window.location.search).has("authError")
      || new URLSearchParams(window.location.search).has("referenceId")) removeAuthenticationCallbackFromUrl();
    void this.session.bootstrap(callback);
  }

  disconnectedCallback(): void {
    this.recordingPanel?.finishModule();
  }

  private authorizeRecordingStart(event: Event): void {
    const state = this.session.state;
    if (state.status !== "signedIn") {
      event.preventDefault();
      return;
    }
    if (!sessionAllowsRecording(state.session.sessionExpiresAt)) {
      event.preventDefault();
      void this.session.logout("Sign in again before recording. Your previous session ended safely.");
    }
  }

  private handleRecordingEvent(event: Event): void {
    const detail = (event as CustomEvent<RecordingEvent>).detail;
    if (detail.type === "changed") this.updateWorkspaceVisibility();
    if (detail.type === "log" && this.recordingPanel.ownsCurrentSession) {
      this.log.write(detail.message);
      this.renderLog();
    }
  }

  private renderSession(state: Readonly<SessionState>): void {
    this.authentication.replaceChildren();
    if (state.status === "loading") {
      this.authentication.append(message("Checking your session…"));
      return;
    }
    if (state.status === "signedIn") {
      this.recordingPanel.setSessionAccount(state.session.account.id);
      const identity = state.session.profile.displayName ?? state.session.profile.email
        ?? `Account ${state.session.account.id.slice(0, 8)}`;
      const summary = message(`${identity} · ${state.session.provider.displayName}`);
      const expiry = sessionExpiry(state.session.sessionExpiresAt);
      const logout = button("Log out", () => void this.session.logout());
      this.authentication.append(summary, expiry, logout);
    } else {
      this.recordingPanel.setSessionAccount(null);
      this.clearPrivateResults();
      const text = `${state.message}${state.referenceId ? ` Reference: ${state.referenceId}` : ""}`;
      this.authentication.append(message(text));
      if (state.status === "failure") {
        this.authentication.append(button("Try again", () => void this.session.bootstrap()));
      } else {
        if (state.loginAllowed) {
          const actions = document.createElement("div");
          actions.className = "provider-actions";
          for (const provider of state.providers) {
            actions.append(button(`Continue with ${provider.displayName}`, () => {
              const returnTo = `${window.location.pathname}${window.location.search}`;
              window.location.assign(this.sessionClient.loginUrl(provider.id, returnTo));
            }, "primary"));
          }
          this.authentication.append(actions, policyLinks());
        }
        if (state.logoutUnconfirmed) {
          this.authentication.append(button("Retry logout", () => void this.session.logout()));
        }
      }
    }
    this.updateWorkspaceVisibility();
  }

  private updateWorkspaceVisibility(): void {
    const signedIn = this.session.state.status === "signedIn";
    this.workspace.hidden = !signedIn && !this.recordingPanel.hasRetainedWork;
    this.privateResults.hidden = !signedIn;
  }

  private clearPrivateResults(): void {
    this.transcript.replaceChildren();
    this.resetLog();
  }

  private element<T extends HTMLElement>(testId: string): T {
    const element = this.querySelector<T>(`[data-testid="${testId}"]`);
    if (!element) throw new Error(`Missing seed-app element: ${testId}`);
    return element;
  }

  private resetLog(): void {
    this.log = new RadarLog(this.config.logColumns, this.config.logRows, this.config.logGapChars);
    this.logView.style.setProperty("--log-columns", String(this.config.logColumns));
    this.renderLog();
  }

  private renderLog(): void {
    const { lines, cursor } = this.log.view();
    const text = lines.join("\n");
    const mark = document.createElement("span");
    mark.className = "cursor";
    mark.textContent = text.charAt(cursor);
    this.logView.replaceChildren(text.slice(0, cursor), mark, text.slice(cursor + 1));
  }
}

function message(text: string): HTMLParagraphElement {
  const element = document.createElement("p"); element.textContent = text; return element;
}
function button(label: string, action: () => void, className = ""): HTMLButtonElement {
  const element = document.createElement("button"); element.type = "button"; element.textContent = label;
  element.className = className; element.addEventListener("click", action); return element;
}
function policyLinks(): HTMLParagraphElement {
  const paragraph = document.createElement("p"); paragraph.className = "policy-links";
  const privacy = document.createElement("a"); privacy.href = "/privacy"; privacy.textContent = "Privacy";
  const terms = document.createElement("a"); terms.href = "/terms"; terms.textContent = "Terms";
  paragraph.append(privacy, " · ", terms); return paragraph;
}
function sessionExpiry(sessionExpiresAt: string): HTMLParagraphElement {
  const paragraph = document.createElement("p");
  const time = document.createElement("time");
  time.dateTime = sessionExpiresAt;
  time.textContent = new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(sessionExpiresAt));
  paragraph.append("Session expires ", time);
  return paragraph;
}
function removeAuthenticationCallbackFromUrl(): void {
  window.history.replaceState(null, "", withoutAuthenticationCallback(window.location.href));
}

customElements.define("seed-app", SeedApp);
