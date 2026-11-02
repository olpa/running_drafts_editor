export type Cue = "start" | "stop" | "health" | "delayed" | "interrupted";

/** WebAudio feedback; `onPlayed` records semantic cues rather than sound samples. */
export class CuePlayer {
  private context: AudioContext | null = null;
  private count = 0;

  constructor(private readonly onPlayed: (cue: Cue, count: number) => void = () => undefined) {}

  /** Prepare during a user gesture. Audio failure must not prevent capture. */
  prepare(): void {
    try {
      this.context ??= new AudioContext();
      void this.context.resume().catch(() => {});
    } catch {
      // The visible equivalents remain available when audio is unavailable.
    }
  }

  play(cue: Cue): void {
    const context = this.context;
    if (context) {
      const now = context.currentTime;
      if (cue === "delayed" || cue === "interrupted") {
        this.tone(now, 0.15, 220, 160);
        let position = now + 0.3;
        // U (upload delayed) and X (capture interrupted), after an attention tone.
        for (const symbol of cue === "delayed" ? "..-" : "-..-") {
          const duration = symbol === "." ? 0.08 : 0.24;
          this.tone(position, duration, 660, 660);
          position += duration + 0.08;
        }
      } else {
        const frequencies =
          cue === "stop" ? [880, 440] : cue === "health" ? [660, 880] : [440, 880];
        this.tone(now, cue === "health" ? 0.1 : 0.15, frequencies[0]!, frequencies[1]!);
      }
    }
    this.count += 1;
    this.onPlayed(cue, this.count);
  }

  /** A WebAudio loop keeps playing without JavaScript timer or visibility checks. */
  loopInterrupted(onUnavailable: () => void): () => void {
    const context = this.context;
    if (!context || context.state !== "running") {
      onUnavailable();
      return () => {};
    }
    const duration = 1.18;
    const buffer = context.createBuffer(
      1,
      Math.ceil(context.sampleRate * duration),
      context.sampleRate,
    );
    const samples = buffer.getChannelData(0);
    const tone = (at: number, length: number, from: number, to: number) => {
      const first = Math.round(at * context.sampleRate);
      const count = Math.round(length * context.sampleRate);
      for (let i = 0; i < count && first + i < samples.length; i++) {
        const t = i / context.sampleRate;
        const phase = 2 * Math.PI * (from * t + ((to - from) * t * t) / (2 * length));
        samples[first + i] = Math.sin(phase) * 0.08 * Math.max(0, 1 - t / length);
      }
    };
    tone(0, 0.15, 220, 160);
    let position = 0.3;
    for (const symbol of "-..-") {
      const length = symbol === "." ? 0.08 : 0.24;
      tone(position, length, 660, 660);
      position += length + 0.08;
    }
    const source = context.createBufferSource();
    source.buffer = buffer;
    source.loop = true;
    source.connect(context.destination);
    let stopped = false;
    const stateChanged = () => {
      if (!stopped && context.state !== "running") onUnavailable();
    };
    context.addEventListener("statechange", stateChanged);
    try {
      source.start();
    } catch (error) {
      context.removeEventListener("statechange", stateChanged);
      source.disconnect();
      throw error;
    }
    this.count++;
    this.onPlayed("interrupted", this.count);
    return () => {
      if (stopped) return;
      stopped = true;
      context.removeEventListener("statechange", stateChanged);
      source.stop();
      source.disconnect();
    };
  }

  dispose(): void {
    const context = this.context;
    this.context = null;
    if (context) void context.close().catch(() => {});
  }

  private tone(at: number, duration: number, from: number, to: number): void {
    const context = this.context!;
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.frequency.setValueAtTime(from, at);
    oscillator.frequency.linearRampToValueAtTime(to, at + duration);
    gain.gain.setValueAtTime(0.08, at);
    gain.gain.linearRampToValueAtTime(0, at + duration);
    oscillator.connect(gain).connect(context.destination);
    oscillator.start(at);
    oscillator.stop(at + duration);
  }
}

/** What the recording module needs from sound feedback. */
export interface Cues {
  prepare(): void;
  play(cue: Cue): void;
  loopInterrupted(onUnavailable: () => void): () => void;
  dispose(): void;
}
