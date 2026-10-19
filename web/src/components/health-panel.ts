import template from "./health-panel.html?raw";
import { HealthClient, type HealthEndpoint } from "../health.js";

const LABELS: Record<HealthEndpoint, { checking: string; success: string }> = {
  live: { checking: "Checking liveness…", success: "Backend is live" },
  ready: { checking: "Checking readiness…", success: "Backend and database are ready" },
};

/** Runs and presents the two development-environment health checks. */
export class HealthPanel extends HTMLElement {
  private readonly client = new HealthClient();

  connectedCallback(): void {
    if (this.childElementCount) return;
    this.innerHTML = template;
    this.button("live").addEventListener("click", () => void this.check("live"));
    this.button("ready").addEventListener("click", () => void this.check("ready"));
  }

  private async check(endpoint: HealthEndpoint): Promise<void> {
    const button = this.button(endpoint);
    const result = this.element(`${endpoint}-result`);
    button.disabled = true;
    result.dataset.state = "busy";
    this.element(`${endpoint}-summary`).textContent = LABELS[endpoint].checking;
    this.element(`${endpoint}-detail`).textContent = `Contacting /health/${endpoint}`;

    try {
      const response = await this.client.check(endpoint);
      result.dataset.state = "success";
      this.element(`${endpoint}-summary`).textContent = LABELS[endpoint].success;
      const status = `HTTP ${response.status}${response.statusText ? ` ${response.statusText}` : ""}`;
      this.element(`${endpoint}-detail`).textContent = response.body ? `${status} · ${response.body}` : status;
    } catch (error) {
      result.dataset.state = "error";
      this.element(`${endpoint}-summary`).textContent = "Check failed";
      this.element(`${endpoint}-detail`).textContent = error instanceof Error ? error.message : String(error);
    } finally {
      button.disabled = false;
    }
  }

  private button(endpoint: HealthEndpoint): HTMLButtonElement {
    return this.element<HTMLButtonElement>(`${endpoint}-check`);
  }

  private element<T extends HTMLElement = HTMLElement>(testId: string): T {
    const element = this.querySelector<T>(`[data-testid="${testId}"]`);
    if (!element) throw new Error(`Missing health-panel element: ${testId}`);
    return element;
  }
}

customElements.define("health-panel", HealthPanel);
