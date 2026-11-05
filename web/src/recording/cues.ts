import type { ProblemSound } from "../alerts/capture-alerts.ts";
import {
  productionSounds,
  synthesizeSound,
  type SoundDefinition,
  type SoundRole,
} from "./sound-definitions.ts";

export type Cue = Exclude<SoundRole, "problem">;

/** WebAudio feedback; `onPlayed` records semantic roles rather than sound samples. */
export class CuePlayer implements Cues {
  private context: AudioContext | null = null;
  private readonly sources = new Set<AudioBufferSourceNode>();
  private count = 0;

  constructor(
    private readonly onPlayed: (role: SoundRole, count: number) => void = () => undefined,
  ) {}

  /** Prepare during a user gesture. Audio failure must not prevent Capture. */
  prepare(): void {
    try {
      this.context ??= new AudioContext();
      void this.context.resume().catch(() => {});
    } catch {
      // The visible equivalents remain available when audio is unavailable.
    }
  }

  play(cue: Cue): void {
    if (this.context) this.start(productionSounds[cue], false);
    this.played(cue);
  }

  /** A WebAudio loop keeps playing without JavaScript timers or visibility checks. */
  loopProblem(onUnavailable: () => void): () => void {
    const context = this.context;
    if (!context || context.state !== "running") {
      onUnavailable();
      return () => {};
    }
    const stop = this.start(productionSounds.problem, true);
    let stopped = false;
    const stateChanged = () => {
      if (!stopped && context.state !== "running") onUnavailable();
    };
    context.addEventListener("statechange", stateChanged);
    this.played("problem");
    return () => {
      if (stopped) return;
      stopped = true;
      context.removeEventListener("statechange", stateChanged);
      stop();
    };
  }

  dispose(): void {
    const context = this.context;
    this.context = null;
    for (const source of this.sources) {
      source.stop();
      source.disconnect();
    }
    this.sources.clear();
    if (context) void context.close().catch(() => {});
  }

  private played(role: SoundRole): void {
    this.count++;
    this.onPlayed(role, this.count);
  }

  private start(value: SoundDefinition, loop: boolean): () => void {
    const context = this.context!;
    const samples = synthesizeSound(value, context.sampleRate);
    const buffer = context.createBuffer(1, samples.length, context.sampleRate);
    buffer.copyToChannel(Float32Array.from(samples), 0);
    const source = context.createBufferSource();
    source.buffer = buffer;
    source.loop = loop;
    source.connect(context.destination);
    this.sources.add(source);
    let cleaned = false;
    const clean = () => {
      if (cleaned) return;
      cleaned = true;
      this.sources.delete(source);
      source.disconnect();
    };
    source.addEventListener("ended", clean, { once: true });
    try {
      source.start();
    } catch (error) {
      clean();
      throw error;
    }
    let stopped = false;
    return () => {
      if (stopped) return;
      stopped = true;
      source.stop();
      clean();
    };
  }
}

/** What production Capture feedback needs from sound playback. */
export interface Cues extends ProblemSound {
  prepare(): void;
  play(cue: Cue): void;
  dispose(): void;
}
