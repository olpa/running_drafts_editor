export type Cue = "start" | "stop";

const CUE_DURATION_S = 0.15;
const CUE_FREQUENCIES_HZ: Record<Cue, [number, number]> = {
  start: [440, 880],
  stop: [880, 440],
};

/**
 * Plays short generated beeps and exposes the last cue as `data-cue` and
 * `data-cue-count` on the host element, so tests can observe cues.
 */
export class CuePlayer {
  private context: AudioContext | null = null;
  private count = 0;

  constructor(private readonly host: HTMLElement) {}

  /** Call from a user gesture so that later cues may play. */
  prepare(): void {
    this.context ??= new AudioContext();
    void this.context.resume();
  }

  play(cue: Cue): void {
    this.prepare();
    const context = this.context!;
    const [from, to] = CUE_FREQUENCIES_HZ[cue];
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    const now = context.currentTime;
    oscillator.frequency.setValueAtTime(from, now);
    oscillator.frequency.linearRampToValueAtTime(to, now + CUE_DURATION_S);
    gain.gain.setValueAtTime(0.2, now);
    gain.gain.linearRampToValueAtTime(0, now + CUE_DURATION_S);
    oscillator.connect(gain).connect(context.destination);
    oscillator.start(now);
    oscillator.stop(now + CUE_DURATION_S);

    this.count += 1;
    this.host.dataset.cue = cue;
    this.host.dataset.cueCount = String(this.count);
  }
}
