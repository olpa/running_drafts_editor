import { afterEach, describe, expect, it, vi } from "vitest";
import { CuePlayer, productionSoundAssignments } from "./cues.ts";
import { productionSounds, synthesizeSound } from "./sound-definitions.ts";

class Buffer {
  readonly samples: Float32Array;
  constructor(readonly length: number) {
    this.samples = new Float32Array(length);
  }
  copyToChannel(samples: Float32Array) {
    this.samples.set(samples);
  }
}

class Source extends EventTarget {
  loop = false;
  buffer: Buffer | null = null;
  connect = vi.fn();
  disconnect = vi.fn();
  start = vi.fn();
  stop = vi.fn();
}

class Gain {
  readonly gain = { value: 0 };
  connect = vi.fn();
  disconnect = vi.fn();
}

class Compressor {
  readonly threshold = { value: 0 };
  readonly knee = { value: 0 };
  readonly ratio = { value: 0 };
  readonly attack = { value: 0 };
  readonly release = { value: 0 };
  connect = vi.fn();
  disconnect = vi.fn();
}

class Audio extends EventTarget {
  state = "running";
  sampleRate = 8000;
  destination = {};
  sources: Source[] = [];
  gains: Gain[] = [];
  compressors: Compressor[] = [];
  async resume() {}
  async close() {
    this.state = "closed";
    this.dispatchEvent(new Event("statechange"));
  }
  createBuffer(_channels: number, length: number) {
    return new Buffer(length);
  }
  createBufferSource() {
    const source = new Source();
    this.sources.push(source);
    return source;
  }
  createGain() {
    const gain = new Gain();
    this.gains.push(gain);
    return gain;
  }
  createDynamicsCompressor() {
    const compressor = new Compressor();
    this.compressors.push(compressor);
    return compressor;
  }
}

afterEach(() => vi.unstubAllGlobals());

describe("production sound feedback", () => {
  it("uses the approved named definitions at bounded production levels", () => {
    expect(productionSounds["healthy-heartbeat"].id).toBe("triangle-cadence");
    expect(productionSounds.problem.id).toBe("morse-stop");
    expect(productionSounds["capture-completed"].id).toBe("four-note-triangle-cadence");
    const problem = synthesizeSound(productionSounds.problem, 8000);
    expect(problem).toHaveLength(9440);
    expect(synthesizeSound(productionSounds.problem, 8000, 12, 2)).toEqual(problem);
    for (const sound of Object.values(productionSounds)) {
      const samples = synthesizeSound(sound, 8000);
      expect(
        samples.every((sample) => Number.isFinite(sample) && Math.abs(sample) <= 0.080001),
      ).toBe(true);
    }
  });

  it("plays independent one-shot cues and one gap-free Problem loop", async () => {
    const context = new Audio();
    vi.stubGlobal("AudioContext", function () {
      return context;
    });
    const played: string[] = [];
    const player = new CuePlayer((role) => played.push(role));
    await player.prepare();
    player.play("capture-started");
    player.play("healthy-heartbeat");
    player.play("capture-completed");
    const unavailable = vi.fn();
    const stop = player.loopProblem(unavailable);
    const stopAllowance = player.loopAllowanceWarning(unavailable);

    expect(played).toEqual([
      "capture-started",
      "healthy-heartbeat",
      "capture-completed",
      "problem",
      "allowance-warning",
    ]);
    expect(context.sources).toHaveLength(5);
    expect(context.sources.slice(0, 3).every((source) => !source.loop)).toBe(true);
    expect(context.sources[3]!.loop).toBe(true);
    expect(context.sources[3]!.buffer?.length).toBe(9440);
    expect(context.sources[4]!.loop).toBe(true);
    expect(context.sources[4]!.buffer?.length).toBe(9440);
    expect(unavailable).not.toHaveBeenCalled();

    stop();
    stop();
    stopAllowance();
    expect(context.sources[3]!.stop).toHaveBeenCalledOnce();
    player.dispose();
  });

  it("changes active and future cues through one master gain", async () => {
    const context = new Audio();
    vi.stubGlobal("AudioContext", function () {
      return context;
    });
    const player = new CuePlayer();
    player.setVolume(100);
    await player.prepare();
    expect(context.gains[0]!.gain.value).toBeCloseTo(7.943, 3);
    expect(context.gains[0]!.connect.mock.calls[0]![0]).toBe(context.compressors[0]);
    expect(context.compressors[0]!.connect.mock.calls[0]![0]).toBe(context.destination);
    expect(context.compressors[0]).toMatchObject({
      threshold: { value: -3 },
      knee: { value: 0 },
      ratio: { value: 20 },
      attack: { value: 0.003 },
      release: { value: 0.1 },
    });
    player.setVolume(50);
    player.play("capture-started");
    const stop = player.loopProblem(() => {});

    expect(context.gains).toHaveLength(1);
    expect(context.gains[0]!.gain.value).toBe(1);
    expect(
      context.sources.every((source) => source.connect.mock.calls[0]![0] === context.gains[0]),
    ).toBe(true);

    player.setVolume(100);
    expect(context.gains[0]!.gain.value).toBeCloseTo(7.943, 3);
    player.setVolume(0);
    expect(context.gains[0]!.gain.value).toBe(0);
    expect(() => player.setVolume(101)).toThrow(/between 0 and 100/u);

    stop();
    player.dispose();
  });

  it("accepts an independent definition for every semantic meaning", async () => {
    const context = new Audio();
    vi.stubGlobal("AudioContext", function () {
      return context;
    });
    const player = new CuePlayer(() => {}, {
      ...productionSoundAssignments,
      "allowance-warning": productionSounds["capture-started"],
    });
    await player.prepare();
    player.loopProblem(() => {});
    player.loopAllowanceWarning(() => {});
    expect(context.sources[0]!.buffer?.length).toBe(9440);
    expect(context.sources[1]!.buffer?.length).toBe(1200);
    player.dispose();
  });

  it("reports unavailable Problem audio and ignores late state changes after stop", async () => {
    const unavailable = vi.fn();
    new CuePlayer().loopProblem(unavailable)();
    expect(unavailable).toHaveBeenCalledOnce();

    const context = new Audio();
    vi.stubGlobal("AudioContext", function () {
      return context;
    });
    const player = new CuePlayer();
    await player.prepare();
    const stop = player.loopProblem(unavailable);
    context.state = "suspended";
    context.dispatchEvent(new Event("statechange"));
    expect(unavailable).toHaveBeenCalledTimes(2);
    stop();
    context.dispatchEvent(new Event("statechange"));
    expect(unavailable).toHaveBeenCalledTimes(2);
    player.dispose();
  });
});
