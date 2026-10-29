import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { AccountId } from "../session/gateway.ts";
import { DEFAULT_CONFIG, configKey } from "../shared/config.ts";
import { axe } from "../test-support/axe.ts";
import type { MicrophoneCapture, TransportFrame } from "./capture.ts";
import type { RecordingGateway } from "./gateway.ts";
import RecordingPanel from "./RecordingPanel.vue";
import { createRecording, recordingKey, type Recording } from "./recording.ts";

const ACCOUNT_A = "5728cdd2-909d-4207-8303-c04b270c308f" as AccountId;
const ACCOUNT_B = "37a919e8-4091-4c7f-809c-e1c856f3de2c" as AccountId;

class FakeCapture implements MicrophoneCapture {
  private onFrame: ((frame: TransportFrame) => void) | null = null;
  disposed = false;
  async start(onFrame: (frame: TransportFrame) => void): Promise<void> {
    this.onFrame = onFrame;
  }
  async stop(): Promise<void> {}
  dispose(): void {
    this.disposed = true;
  }
  emit(frame: TransportFrame): void {
    this.onFrame?.(frame);
  }
}

function harness(authorize = true) {
  const captures: FakeCapture[] = [];
  const played: string[] = [];
  const gateway: RecordingGateway = {
    createProject: async () => ({ id: "p", title: "Canonical", createdAt: "x", updatedAt: "x" }),
    getProject: async (id) => ({ id, title: "Existing", createdAt: "x", updatedAt: "x" }),
    createSection: async () => ({ sectionId: "s", audioAssetId: "a", position: 0 }),
    putFrame: () => new Promise(() => undefined),
    complete: async () => ({ id: "a" }) as never,
    interrupt: async () => ({ id: "a" }) as never,
  };
  const recording = createRecording({
    config: DEFAULT_CONFIG,
    environment: {
      gateway,
      now: () => performance.now(),
      openCapture: async () => {
        const capture = new FakeCapture();
        captures.push(capture);
        return capture;
      },
    },
    cues: { prepare() {}, play: (cue) => played.push(cue), dispose() {} },
    authorizeStart: () => authorize,
  });
  return { recording, captures, played };
}

function mountPanel(recording: Recording) {
  return mount(RecordingPanel, {
    global: {
      provide: { [recordingKey as symbol]: recording, [configKey as symbol]: DEFAULT_CONFIG },
    },
  });
}

describe("Recording module", () => {
  it("is idle until an Account signs in, and idle again when it leaves without pending work", () => {
    const { recording } = harness();
    expect(recording.hasRecorder.value).toBe(false);
    recording.setSessionAccount(ACCOUNT_A);
    expect(recording.hasRecorder.value).toBe(true);
    expect(recording.ownsCurrentSession.value).toBe(true);
    recording.setSessionAccount(null);
    expect(recording.hasRecorder.value).toBe(false);
    expect(recording.ownsCurrentSession.value).toBe(false);
  });

  it("hands an idle recorder to the next Account", () => {
    const { recording } = harness();
    recording.setSessionAccount(ACCOUNT_A);
    recording.setSessionAccount(ACCOUNT_B);
    expect(recording.hasRecorder.value).toBe(true);
    expect(recording.ownsCurrentSession.value).toBe(true);
    expect(recording.ownershipWarning.value).toBe("");
  });

  it("does not start for an Account that does not own the recorder or when not authorized", async () => {
    const refused = harness(false);
    refused.recording.setSessionAccount(ACCOUNT_A);
    refused.recording.toggle();
    await flushPromises();
    expect(refused.captures).toHaveLength(0);

    const { recording, captures } = harness();
    recording.setSessionAccount(ACCOUNT_A);
    recording.toggle();
    await flushPromises();
    expect(captures).toHaveLength(1);
    expect(recording.snapshot.value.capturing).toBe(true);
  });

  it("keeps unsent audio for its own Account and warns a different Account", async () => {
    const { recording, captures } = harness();
    recording.setSessionAccount(ACCOUNT_A);
    recording.toggle();
    await flushPromises();
    captures[0]!.emit({
      seq: 0,
      startMs: 0,
      endMs: 10_000,
      mediaType: "audio/webm;codecs=opus",
      data: new Blob(["bytes"]),
    });

    recording.setSessionAccount(null);
    expect(recording.hasRetainedWork.value).toBe(true);
    expect(recording.ownershipWarning.value).toMatch(/Pending recording work is retained/u);

    recording.setSessionAccount(ACCOUNT_B);
    expect(recording.ownsCurrentSession.value).toBe(false);
    expect(recording.ownershipWarning.value).toMatch(/different Account/u);

    recording.setSessionAccount(ACCOUNT_A);
    expect(recording.ownershipWarning.value).toBe("");
    expect(recording.ownsCurrentSession.value).toBe(true);
  });

  it("releases capture when permanently disposed", async () => {
    const { recording, captures } = harness();
    recording.setSessionAccount(ACCOUNT_A);
    recording.toggle();
    await flushPromises();
    recording.dispose();
    expect(captures[0]!.disposed).toBe(true);
    expect(recording.hasRecorder.value).toBe(false);
  });
});

describe("RecordingPanel", () => {
  it("asks the user to sign in while idle", () => {
    const { recording } = harness();
    const wrapper = mountPanel(recording);
    expect(wrapper.get("[data-testid=start]").attributes("disabled")).toBeDefined();
    expect(wrapper.get("[data-testid=capture-message]").text()).toBe("Sign in to start capture.");
    expect(wrapper.get("[data-testid=project-title]").text()).toBe("New Project");
  });

  it("starts, shows capture state, and stops from the same control", async () => {
    const { recording, played } = harness();
    recording.setSessionAccount(ACCOUNT_A);
    const wrapper = mountPanel(recording);
    const start = wrapper.get("[data-testid=start]");
    expect(start.text()).toBe("Start");
    expect(start.attributes("data-mode")).toBe("start");

    await start.trigger("click");
    await flushPromises();
    expect(start.attributes("data-mode")).toBe("stop");
    expect(start.text()).toBe("Stop current recording to start new");
    expect(wrapper.get("[data-testid=capture-indicator]").attributes("data-capturing")).toBe(
      "true",
    );
    expect(wrapper.get("[data-testid=capture-message]").text()).toBe(
      "Capture is active. Speak now.",
    );
    expect(wrapper.find("[data-testid=capture-progress]").exists()).toBe(true);
    expect(wrapper.attributes("data-cue")).toBe("start");
    expect(played).toContain("start");
    expect(wrapper.get("[data-testid=project-title]").text()).toBe("Canonical");
  });

  it("has no detectable accessibility violations", async () => {
    const { recording } = harness();
    recording.setSessionAccount(ACCOUNT_A);
    const wrapper = mountPanel(recording);
    expect(await axe(wrapper.element as HTMLElement)).toHaveNoViolations();
  });
});
