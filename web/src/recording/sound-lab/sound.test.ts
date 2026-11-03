import { afterEach, describe, expect, it, vi } from "vitest";
import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { createCaptureStopAlarm } from "../../alarm/capture-stop-alarm.ts";
import { candidate, candidates, palettes, roles } from "./candidates.ts";
import { createLabSound, normalize, synthesize, type SoundSettings } from "./sound.ts";

function settings(): SoundSettings {
  return {
    choices: { ...palettes[0]!.choices },
    levels: { ...palettes[0]!.levels },
    master: 0.6,
    pitch: 0,
    duration: 1,
    alarmGap: 0,
    unavailable: false,
    repeatProblem: false,
  };
}
class Buffer {
  readonly numberOfChannels = 1;
  private samples: Float32Array;
  constructor(
    readonly length: number,
    readonly sampleRate: number,
  ) {
    this.samples = new Float32Array(length);
  }
  getChannelData() {
    return this.samples;
  }
  copyToChannel(data: Float32Array) {
    this.samples.set(data);
  }
}
interface Gain {
  gain: { value: number };
  connect: ReturnType<typeof vi.fn>;
  disconnect: ReturnType<typeof vi.fn>;
}
interface Source {
  buffer: Buffer | null;
  loop: boolean;
  onended: (() => void) | null;
  connect: (gain: Gain) => Gain;
  disconnect: ReturnType<typeof vi.fn>;
  start: ReturnType<typeof vi.fn>;
  stop: ReturnType<typeof vi.fn>;
}
class Audio extends EventTarget {
  state = "running";
  currentTime = 0;
  sampleRate = 8000;
  destination = {};
  sources: Source[] = [];
  gains: Gain[] = [];
  async resume() {
    this.state = "running";
  }
  async close() {
    this.state = "closed";
  }
  createBuffer(_channels: number, length: number, sampleRate: number) {
    return new Buffer(length, sampleRate);
  }
  async decodeAudioData() {
    const buffer = new Buffer(100, this.sampleRate);
    buffer.copyToChannel(Float32Array.from({ length: 100 }, (_, i) => Math.sin(i) * 0.9));
    return buffer;
  }
  createGain(): Gain {
    const gain = { gain: { value: 1 }, connect: vi.fn(), disconnect: vi.fn() };
    this.gains.push(gain);
    return gain;
  }
  createBufferSource(): Source {
    const source = {
      buffer: null as Buffer | null,
      loop: false,
      onended: null as (() => void) | null,
      connect: (gain: ReturnType<Audio["createGain"]>) => gain,
      disconnect: vi.fn(),
      start: vi.fn(),
      stop: vi.fn(),
    };
    this.sources.push(source);
    return source;
  }
}
afterEach(() => vi.unstubAllGlobals());

describe("laboratory candidate safety", () => {
  it("keeps every candidate locally reproducible with complete provenance and valid palette references", () => {
    expect(new Set(candidates.map((c) => c.id)).size).toBe(candidates.length);
    for (const role of roles)
      expect(candidates.some((c) => c.role === role && c.baseline)).toBe(true);
    for (const palette of palettes)
      for (const role of roles) expect(candidate(palette.choices[role])).toBeDefined();
    for (const value of candidates) {
      expect(value.author).toBeTruthy();
      expect(value.license).toBeTruthy();
      expect(value.source).toBeTruthy();
      expect(value.attribution).toBeTruthy();
      if (value.asset) {
        const path = resolve(
          "src/recording/sound-lab/assets",
          `${value.id.replace("kenney-", "")}.ogg`,
        );
        expect(existsSync(path)).toBe(true);
        expect(readFileSync(path).subarray(0, 4).toString()).toBe("OggS");
      } else {
        for (const pitch of [-12, 0, 12])
          for (const duration of [0.5, 1, 2]) {
            const samples = synthesize(value, 8000, pitch, duration);
            expect(samples.length).toBeGreaterThan(0);
            expect(samples.every((v) => Number.isFinite(v) && Math.abs(v) <= 0.080001)).toBe(true);
          }
      }
    }
    expect(normalize(new Float32Array(100)).every((v) => v === 0)).toBe(true);
  });
  it("preserves the baseline continuous alarm length and samples despite synthesis tuning", () => {
    const baseline = candidate("baseline-interrupted");
    const normal = synthesize(baseline, 8000);
    expect(normal.length).toBe(9440);
    expect(synthesize(baseline, 8000, 12, 2)).toEqual(normal);
    expect(normal[400]).toBeCloseTo(
      Math.sin(2 * Math.PI * (220 * 0.05 - (60 * 0.05 ** 2) / 0.3)) * 0.08 * (1 - 0.05 / 0.15),
      5,
    );
  });
});

describe("laboratory audio adapter", () => {
  function harness() {
    const audio = new Audio();
    vi.stubGlobal("AudioContext", function () {
      return audio;
    });
    const fetch = vi.fn(async (url: string) => {
      if (!url) throw new Error("Missing bundled asset URL");
      return new Response(new Uint8Array([1]));
    });
    vi.stubGlobal("fetch", fetch);
    const config = settings();
    const failure = vi.fn();
    const log = vi.fn();
    const sound = createLabSound(config, log, failure);
    return { audio, fetch, config, failure, sound };
  }
  it("repeats one Problem loop until a healthy cue or acknowledgement, independently of completion", async () => {
    const h = harness();
    h.config.repeatProblem = true;
    h.config.choices.delayed = "baseline-interrupted";
    await h.sound.enable();
    h.sound.cues.play("delayed");
    h.sound.cues.play("delayed");
    expect(h.audio.sources).toHaveLength(1);
    const loop = h.audio.sources[0]!;
    expect(loop.loop).toBe(true);
    expect(h.sound.problemActive.value).toBe(true);
    h.sound.cues.play("stop");
    expect(loop.stop).not.toHaveBeenCalled();
    expect(h.sound.problemActive.value).toBe(true);
    h.sound.cues.play("health");
    expect(loop.stop).toHaveBeenCalledOnce();
    expect(h.sound.problemActive.value).toBe(false);
    h.sound.cues.play("delayed");
    const later = h.audio.sources.at(-1)!;
    h.sound.acknowledgeProblem();
    expect(later.stop).toHaveBeenCalledOnce();
    expect(h.sound.problemActive.value).toBe(false);
    h.sound.cues.dispose();
  });
  it("gives the stop alarm sound priority and keeps it active through healthy feedback", async () => {
    const h = harness();
    h.config.repeatProblem = true;
    await h.sound.enable();
    h.sound.cues.play("delayed");
    const problem = h.audio.sources[0]!;
    const alarm = createCaptureStopAlarm(h.sound.cues);
    alarm.activate();
    const stopLoop = h.audio.sources[1]!;
    expect(problem.stop).toHaveBeenCalledOnce();
    h.sound.cues.play("delayed");
    expect(h.audio.sources).toHaveLength(2);
    h.sound.cues.play("health");
    expect(h.sound.problemActive.value).toBe(false);
    expect(alarm.active.value).toBe(true);
    expect(stopLoop.stop).not.toHaveBeenCalled();
    h.sound.cues.play("delayed");
    h.sound.acknowledgeProblem();
    alarm.acknowledge();
    expect(stopLoop.stop).toHaveBeenCalledOnce();
    expect(h.sound.problemActive.value).toBe(false);
    expect(alarm.active.value).toBe(false);
    h.sound.cues.dispose();
  });
  it("shares identical Problem and stop-alarm playback without restarting or doubling it", async () => {
    const h = harness();
    h.config.repeatProblem = true;
    h.config.choices.delayed = "baseline-interrupted";
    await h.sound.enable();
    h.sound.cues.play("delayed");
    const loop = h.audio.sources[0]!;
    const alarm = createCaptureStopAlarm(h.sound.cues);
    alarm.activate();
    expect(h.audio.sources).toHaveLength(1);
    expect(loop.stop).not.toHaveBeenCalled();
    h.sound.acknowledgeProblem();
    alarm.acknowledge();
    expect(loop.stop).toHaveBeenCalledOnce();
    h.sound.cues.dispose();
  });
  it("retunes an active Problem loop and keeps one-shot feedback available for the baseline", async () => {
    const h = harness();
    h.config.repeatProblem = true;
    await h.sound.enable();
    h.sound.cues.play("delayed");
    const first = h.audio.sources[0]!;
    h.config.choices.delayed = "baseline-interrupted";
    h.sound.refreshAlarm();
    expect(first.stop).toHaveBeenCalledOnce();
    const replacement = h.audio.sources[1]!;
    expect(replacement.loop).toBe(true);
    expect(replacement.buffer?.length).toBe(9440);
    h.config.repeatProblem = false;
    h.sound.refreshAlarm();
    expect(replacement.stop).toHaveBeenCalledOnce();
    expect(h.sound.problemActive.value).toBe(false);
    h.sound.cues.play("delayed");
    expect(h.audio.sources.at(-1)?.loop).toBe(false);
    h.sound.cues.dispose();
  });
  it("requires explicit enabling and loads only bundled assets before playback", async () => {
    const h = harness();
    h.sound.cues.prepare();
    h.sound.cues.play("health");
    expect(h.fetch).not.toHaveBeenCalled();
    expect(h.audio.sources).toHaveLength(0);
    expect(h.failure).toHaveBeenCalledWith(expect.stringContaining("Enable laboratory sound"));
    await h.sound.enable();
    expect(h.fetch).toHaveBeenCalledTimes(12);
    for (const [url] of h.fetch.mock.calls) expect(String(url)).not.toMatch(/^https?:/);
    h.sound.cues.play("health");
    h.config.choices.health = "wood-tap";
    h.config.master = 0.4;
    h.sound.cues.play("health");
    expect(h.audio.sources).toHaveLength(2);
    expect(h.audio.gains.at(-1)?.gain.value).toBe(0.4);
    expect(h.audio.sources[0]?.stop).not.toHaveBeenCalled();
    h.sound.cues.dispose();
    expect(h.audio.sources.every((source) => source.stop.mock.calls.length === 1)).toBe(true);
  });
  it("replaces active alarm audio without clearing the production alarm and disposes both lifetimes", async () => {
    const h = harness();
    await h.sound.enable();
    const alarm = createCaptureStopAlarm(h.sound.cues);
    alarm.activate();
    const first = h.audio.sources[0]!;
    expect(first.loop).toBe(true);
    expect(first.buffer?.length).toBe(9440);
    h.config.choices.interrupted = "alternating-call";
    h.config.alarmGap = 0.5;
    h.sound.refreshAlarm();
    expect(first.stop).toHaveBeenCalledOnce();
    expect(alarm.active.value).toBe(true);
    const replacement = h.audio.sources[1]!;
    expect(replacement.buffer?.length).toBe(8800);
    h.audio.state = "suspended";
    h.audio.dispatchEvent(new Event("statechange"));
    expect(alarm.soundUnavailable.value).toBe(true);
    alarm.acknowledge();
    expect(replacement.stop).toHaveBeenCalledOnce();
    h.sound.cues.dispose();
    h.sound.refreshAlarm();
    expect(h.audio.sources).toHaveLength(2);
  });
  it("surfaces a bundled asset failure without falling through to other services", async () => {
    const h = harness();
    h.fetch.mockRejectedValueOnce(new Error("Local asset missing"));
    await h.sound.enable();
    expect(h.fetch).toHaveBeenCalledOnce();
    expect(h.failure).toHaveBeenCalledWith(expect.stringContaining("Local asset missing"));
    const unavailable = vi.fn();
    h.sound.cues.loopInterrupted(unavailable);
    expect(unavailable).toHaveBeenCalledOnce();
    h.sound.cues.dispose();
  });
});
