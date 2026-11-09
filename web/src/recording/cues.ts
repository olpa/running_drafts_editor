import type { ProblemSound } from "../alerts/capture-alerts.ts";
import {
  normalizeSound,
  productionSounds,
  synthesizeSound,
  type SoundDefinition,
  type SoundRole,
} from "./sound-definitions.ts";

export type Cue = Exclude<SoundRole, "problem">;
export type FeedbackRole = SoundRole | "allowance-warning";
export type SoundAssignments = Readonly<Record<FeedbackRole, SoundDefinition>>;

export const productionSoundAssignments: SoundAssignments = {
  ...productionSounds,
  "allowance-warning": productionSounds.problem,
};

/** WebAudio feedback; `onEvent` records semantic roles rather than sound samples. */
export class CuePlayer implements Cues {
  private context: AudioContext | null = null;
  private readonly sources = new Set<AudioBufferSourceNode>();
  private count = 0;
  private readonly decoded = new Map<string, AudioBuffer>();

  constructor(
    private readonly onEvent: (
      role: FeedbackRole,
      phase: "started" | "stopped",
      count: number,
    ) => void = () => undefined,
    private readonly definitions: SoundAssignments = productionSoundAssignments,
  ) {}

  /** Prepare during a user gesture. Audio failure must not prevent Capture. */
  async prepare(): Promise<void> {
    try {
      this.context ??= new AudioContext();
      await this.context.resume();
      await Promise.all(
        [...new Set(Object.values(this.definitions))]
          .filter((definition) => definition.asset)
          .map((definition) => this.loadAsset(definition)),
      );
    } catch {
      // The visible equivalents remain available when audio is unavailable.
    }
  }

  play(cue: Cue): void {
    if (this.context) this.start(this.definitions[cue], false, cue);
    this.played(cue);
  }

  /** A WebAudio loop keeps playing without JavaScript timers or visibility checks. */
  loopProblem(onUnavailable: () => void): () => void {
    return this.loop("problem", onUnavailable);
  }

  /** The allowance role deliberately reuses the Problem waveform, not its semantics. */
  loopAllowanceWarning(onUnavailable: () => void): () => void {
    return this.loop("allowance-warning", onUnavailable);
  }

  private loop(role: "problem" | "allowance-warning", onUnavailable: () => void): () => void {
    const context = this.context;
    if (!context || context.state !== "running") {
      onUnavailable();
      return () => {};
    }
    const stop = this.start(this.definitions[role], true, role);
    let stopped = false;
    const stateChanged = () => {
      if (!stopped && context.state !== "running") onUnavailable();
    };
    context.addEventListener("statechange", stateChanged);
    this.played(role);
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

  private played(role: FeedbackRole): void {
    this.count++;
    this.onEvent(role, "started", this.count);
  }

  private start(value: SoundDefinition, loop: boolean, role: FeedbackRole): () => void {
    const context = this.context!;
    let buffer = this.decoded.get(value.id);
    if (!buffer) {
      const samples = synthesizeSound(value, context.sampleRate);
      buffer = context.createBuffer(1, samples.length, context.sampleRate);
      buffer.copyToChannel(Float32Array.from(samples), 0);
    }
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
      this.onEvent(role, "stopped", this.count);
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

  private async loadAsset(value: SoundDefinition): Promise<void> {
    if (!value.asset || this.decoded.has(value.id)) return;
    const response = await fetch(value.asset);
    if (!response.ok) throw new Error(`Could not load bundled sound ${value.id}.`);
    const decoded = await this.context!.decodeAudioData(await response.arrayBuffer());
    const samples = new Float32Array(decoded.length);
    for (let channel = 0; channel < decoded.numberOfChannels; channel++) {
      const input = decoded.getChannelData(channel);
      for (let index = 0; index < samples.length; index++) {
        samples[index] = samples[index]! + input[index]! / decoded.numberOfChannels;
      }
    }
    const normalized = normalizeSound(samples);
    const buffer = this.context!.createBuffer(1, normalized.length, decoded.sampleRate);
    buffer.copyToChannel(Float32Array.from(normalized), 0);
    this.decoded.set(value.id, buffer);
  }
}

/** What production Capture feedback needs from sound playback. */
export interface Cues extends ProblemSound {
  prepare(): void | Promise<void>;
  play(cue: Cue): void;
  loopAllowanceWarning?(onUnavailable: () => void): () => void;
  dispose(): void;
}
