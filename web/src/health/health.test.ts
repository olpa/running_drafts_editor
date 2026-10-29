import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { axe } from "../test-support/axe.ts";
import { createHealthGateway, type HealthGateway } from "./gateway.ts";
import { createHealth, healthKey } from "./health.ts";
import HealthPanel from "./HealthPanel.vue";

describe("health gateway", () => {
  it("checks liveness at the configured backend", async () => {
    let requestedUrl = "";
    const gateway = createHealthGateway("http://localhost:18081/", (async (
      input: string | URL | Request,
    ) => {
      requestedUrl = String(input);
      return new Response('{"status":"ok"}', { status: 200, statusText: "OK" });
    }) as typeof fetch);

    expect(await gateway.check("live")).toEqual({
      endpoint: "live",
      status: 200,
      statusText: "OK",
      body: '{"status":"ok"}',
    });
    expect(requestedUrl).toBe("http://localhost:18081/health/live");
  });

  it("keeps readiness distinct and reports an unhealthy response", async () => {
    let requestedUrl = "";
    const gateway = createHealthGateway("https://api.example.test", (async (
      input: string | URL | Request,
    ) => {
      requestedUrl = String(input);
      return new Response("database unavailable", {
        status: 503,
        statusText: "Service Unavailable",
      });
    }) as typeof fetch);

    await expect(gateway.check("ready")).rejects.toThrow(/HTTP 503 Service Unavailable/);
    expect(requestedUrl).toBe("https://api.example.test/health/ready");
  });
});

describe("HealthPanel", () => {
  function mountPanel(gateway: HealthGateway) {
    return mount(HealthPanel, {
      global: { provide: { [healthKey as symbol]: createHealth(gateway) } },
    });
  }

  it("shows the busy, success, and error states of each check", async () => {
    let release!: () => void;
    const gateway: HealthGateway = {
      check: (endpoint) =>
        endpoint === "live"
          ? new Promise((resolve) => {
              release = () => resolve({ endpoint, status: 200, statusText: "OK", body: "ok" });
            })
          : Promise.reject(new Error("HTTP 503")),
    };
    const wrapper = mountPanel(gateway);

    await wrapper.get("[data-testid=live-check]").trigger("click");
    expect(wrapper.get("[data-testid=live-result]").attributes("data-state")).toBe("busy");
    expect(wrapper.get("[data-testid=live-check]").attributes("disabled")).toBeDefined();
    release();
    await flushPromises();
    expect(wrapper.get("[data-testid=live-result]").attributes("data-state")).toBe("success");
    expect(wrapper.get("[data-testid=live-summary]").text()).toBe("Backend is live");
    expect(wrapper.get("[data-testid=live-detail]").text()).toBe("HTTP 200 OK · ok");

    await wrapper.get("[data-testid=ready-check]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-testid=ready-result]").attributes("data-state")).toBe("error");
    expect(wrapper.get("[data-testid=ready-summary]").text()).toBe("Check failed");
    expect(wrapper.get("[data-testid=ready-detail]").text()).toBe("HTTP 503");
  });

  it("has no detectable accessibility violations", async () => {
    const wrapper = mountPanel({ check: () => Promise.reject(new Error("unused")) });
    expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();
  });
});
