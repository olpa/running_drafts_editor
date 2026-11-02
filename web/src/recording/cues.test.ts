import { afterEach, describe, expect, it, vi } from "vitest";
import { CuePlayer } from "./cues.ts";
import { playSectionFeedback } from "./feedback.ts";
import type { SectionEvent } from "./section-events.ts";

class Audio extends EventTarget {
  state = "running";
  currentTime = 0;
  sampleRate = 1000;
  destination = {};
  sources: {
    loop: boolean;
    buffer: unknown;
    connect: ReturnType<typeof vi.fn>;
    disconnect: ReturnType<typeof vi.fn>;
    start: ReturnType<typeof vi.fn>;
    stop: ReturnType<typeof vi.fn>;
  }[] = [];
  oscillators: unknown[] = [];
  async resume() {}
  async close() {
    this.state = "closed";
    this.dispatchEvent(new Event("statechange"));
  }
  createBuffer(_channels: number, length: number) {
    const data = new Float32Array(length);
    return { getChannelData: () => data };
  }
  createBufferSource() {
    const source = {
      loop: false,
      buffer: null as unknown,
      connect: vi.fn(),
      disconnect: vi.fn(),
      start: vi.fn(),
      stop: vi.fn(),
    };
    this.sources.push(source);
    return source;
  }
  createOscillator() {
    const source = {
      frequency: { setValueAtTime() {}, linearRampToValueAtTime() {} },
      connect() {
        return { connect() {} };
      },
      start() {},
      stop() {},
    };
    this.oscillators.push(source);
    return source;
  }
  createGain() {
    return { gain: { setValueAtTime() {}, linearRampToValueAtTime() {} }, connect() {} };
  }
}
afterEach(() => {
  vi.unstubAllGlobals();
});
describe("sound feedback", () => {
  it("maps semantic events to the baseline vocabulary independently", () => {
    const play = vi.fn();
    const cues = { prepare() {}, play, loopInterrupted: () => () => {}, dispose() {} };
    const events: SectionEvent[] = [
      { type: "captureStarted" },
      { type: "transportFrameSecured", frameId: 0, captureActive: true, finalFrame: false },
      { type: "transportFrameSecured", frameId: 1, captureActive: false, finalFrame: true },
      { type: "transportFrameOverdue", frameId: 2 },
      { type: "captureFinalizationFailed", reason: "user", error: new Error() },
      { type: "audioAssetSealingFailed", error: new Error() },
      { type: "unrequestedCaptureStopped" },
      { type: "audioAssetSealed" },
    ];
    for (const event of events) playSectionFeedback(event, cues);
    expect(play.mock.calls.flat()).toEqual([
      "start",
      "health",
      "delayed",
      "delayed",
      "delayed",
      "stop",
    ]);
    play.mockImplementation(() => {
      throw new Error("audio failed");
    });
    expect(() => playSectionFeedback({ type: "captureStarted" }, cues)).not.toThrow();
  });
  it("loops the baseline sound in WebAudio, overlaps independent cues, and stops immediately", () => {
    const context = new Audio();
    vi.stubGlobal("AudioContext", function () {
      return context;
    });
    const player = new CuePlayer();
    player.prepare();
    const unavailable = vi.fn();
    const stop = player.loopInterrupted(unavailable);
    const source = context.sources[0]!;
    expect(source.loop).toBe(true);
    expect(source.start).toHaveBeenCalledOnce();
    expect(source.buffer).not.toBeNull();
    document.dispatchEvent(new Event("visibilitychange"));
    player.play("stop");
    player.play("health");
    expect(context.oscillators).toHaveLength(2);
    expect(source.stop).not.toHaveBeenCalled();
    expect(unavailable).not.toHaveBeenCalled();
    stop();
    stop();
    expect(source.stop).toHaveBeenCalledOnce();
    expect(source.disconnect).toHaveBeenCalledOnce();
    player.dispose();
  });
  it("reports unavailable and interrupted audio without affecting visible behavior", () => {
    const player = new CuePlayer();
    const unavailable = vi.fn();
    player.loopInterrupted(unavailable)();
    expect(unavailable).toHaveBeenCalledOnce();
    const context = new Audio();
    vi.stubGlobal("AudioContext", function () {
      return context;
    });
    player.prepare();
    const stop = player.loopInterrupted(unavailable);
    context.state = "suspended";
    context.dispatchEvent(new Event("statechange"));
    expect(unavailable).toHaveBeenCalledTimes(2);
    stop();
    context.dispatchEvent(new Event("statechange"));
    expect(unavailable).toHaveBeenCalledTimes(2);
    player.dispose();
  });
});
