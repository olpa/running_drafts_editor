export type Cue = "start" | "stop" | "health" | "delayed" | "interrupted";

/** WebAudio feedback; test hooks record semantic cues rather than sound samples. */
export class CuePlayer {
  private context: AudioContext | null = null;
  private count = 0;

  constructor(private readonly host: HTMLElement) {}

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
        const frequencies = cue === "stop" ? [880, 440] : cue === "health" ? [660, 880] : [440, 880];
        this.tone(now, cue === "health" ? 0.1 : 0.15, frequencies[0]!, frequencies[1]!);
      }
    }
    this.count += 1;
    this.host.dataset.cue = cue;
    this.host.dataset.cueCount = String(this.count);
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
