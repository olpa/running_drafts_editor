import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { candidate, candidates, palettes, productionPalette, roles } from "./candidates.ts";
import { createLabSound, normalize, synthesize, type SoundSettings } from "./sound.ts";

function settings(): SoundSettings {
  return {
    choices: { ...productionPalette.choices },
    levels: { ...productionPalette.levels },
    master: 0.6,
    pitch: 0,
    duration: 1,
    problemGap: 0,
    unavailable: false,
  };
}

class Buffer {
  readonly numberOfChannels = 1;
  private readonly samples: Float32Array;
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
  async resume() {}
  async close() {
    this.state = "closed";
  }
  createBuffer(_channels: number, length: number, sampleRate: number) {
    return new Buffer(length, sampleRate);
  }
  async decodeAudioData() {
    const buffer = new Buffer(100, this.sampleRate);
    buffer.copyToChannel(Float32Array.from({ length: 100 }, (_, index) => Math.sin(index) * 0.9));
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
      connect: (gain: Gain) => gain,
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
  it("keeps every candidate locally reproducible with complete provenance and valid palettes", () => {
    expect(new Set(candidates.map((value) => value.id)).size).toBe(candidates.length);
    for (const role of roles) expect(candidates.some((value) => value.role === role)).toBe(true);
    for (const palette of palettes) {
      for (const role of roles) expect(candidate(palette.choices[role])).toBeDefined();
    }
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
        for (const pitch of [-12, 0, 12]) {
          for (const duration of [0.5, 1, 2]) {
            const samples = synthesize(value, 8000, pitch, duration);
            expect(samples.length).toBeGreaterThan(0);
            expect(
              samples.every((sample) => Number.isFinite(sample) && Math.abs(sample) <= 0.080001),
            ).toBe(true);
          }
        }
      }
    }
    expect(normalize(new Float32Array(100)).every((sample) => sample === 0)).toBe(true);
    expect(synthesize(candidate("morse-stop"), 8000)).toHaveLength(9440);
  });
});

describe("laboratory sound adapter", () => {
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
    return { audio, config, failure, fetch, sound };
  }

  it("requires enabling, loads only local assets, and plays selected one-shot cues", async () => {
    const h = harness();
    h.sound.cues.play("healthy-heartbeat");
    expect(h.audio.sources).toHaveLength(0);
    expect(h.failure).toHaveBeenCalledWith(expect.stringContaining("Enable laboratory sound"));
    await h.sound.enable();
    expect(h.fetch).toHaveBeenCalledTimes(12);
    for (const [url] of h.fetch.mock.calls) expect(String(url)).not.toMatch(/^https?:/);

    h.sound.cues.play("healthy-heartbeat");
    h.config.choices["healthy-heartbeat"] = "wood-tap";
    h.config.master = 0.4;
    h.sound.cues.play("healthy-heartbeat");
    expect(h.audio.sources).toHaveLength(2);
    expect(h.audio.gains.at(-1)?.gain.value).toBe(0.4);
  });

  it("plays one selected Problem loop with the bounded Storybook gap", async () => {
    const h = harness();
    await h.sound.enable();
    h.config.problemGap = 0.5;
    const unavailable = vi.fn();
    const stop = h.sound.cues.loopProblem(unavailable);
    const source = h.audio.sources[0]!;
    expect(source.loop).toBe(true);
    expect(source.buffer?.length).toBe(13_440);
    expect(unavailable).not.toHaveBeenCalled();
    stop();
    expect(source.stop).toHaveBeenCalledOnce();
  });

  it("reports simulated Problem failure and disposes active sources", async () => {
    const h = harness();
    await h.sound.enable();
    h.config.unavailable = true;
    const unavailable = vi.fn();
    h.sound.cues.loopProblem(unavailable);
    expect(unavailable).toHaveBeenCalledOnce();
    expect(h.failure).toHaveBeenCalledWith(expect.stringContaining("simulated"));
    h.config.unavailable = false;
    h.sound.cues.play("capture-completed");
    const source = h.audio.sources.at(-1)!;
    h.sound.cues.dispose();
    expect(source.stop).toHaveBeenCalledOnce();
  });
});
