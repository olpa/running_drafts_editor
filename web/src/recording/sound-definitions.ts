export type SoundRole = "capture-started" | "healthy-heartbeat" | "problem" | "capture-completed";

export type Voice = "sine" | "triangle" | "bell" | "wood" | "noise";

export interface SoundNote {
  readonly at: number;
  readonly length: number;
  readonly from: number;
  readonly to: number;
  readonly voice: Voice;
}

export interface SoundDefinition {
  readonly id: string;
  readonly name: string;
  readonly role: SoundRole;
  readonly notes?: readonly SoundNote[];
  /** Optional bundled source used by review tooling through the production player. */
  readonly asset?: string;
  readonly duration?: number;
  /** Preserve the original production envelope and amplitude exactly. */
  readonly fixed?: boolean;
}

export const soundNote = (
  at: number,
  length: number,
  from: number,
  to = from,
  voice: Voice = "sine",
): SoundNote => ({ at, length, from, to, voice });

function morse(pattern: string): SoundNote[] {
  let at = 0.3;
  return [
    soundNote(0, 0.15, 220, 160),
    ...[...pattern].map((symbol) => {
      const length = symbol === "." ? 0.08 : 0.24;
      const result = soundNote(at, length, 660);
      at += length + 0.08;
      return result;
    }),
  ];
}

export const productionSounds: Readonly<Record<SoundRole, SoundDefinition>> = {
  "capture-started": {
    id: "capture-started",
    name: "Capture started",
    role: "capture-started",
    notes: [soundNote(0, 0.15, 440, 880)],
    duration: 0.15,
    fixed: true,
  },
  "healthy-heartbeat": {
    id: "triangle-cadence",
    name: "Triangle cadence",
    role: "healthy-heartbeat",
    notes: [soundNote(0, 0.16, 440, 440, "triangle"), soundNote(0.16, 0.3, 660, 660, "triangle")],
  },
  problem: {
    id: "morse-stop",
    name: "Morse Stop",
    role: "problem",
    notes: morse("-..-"),
    duration: 1.18,
    fixed: true,
  },
  "capture-completed": {
    id: "four-note-triangle-cadence",
    name: "Four-note triangle cadence",
    role: "capture-completed",
    notes: [
      soundNote(0, 0.14, 440, 440, "triangle"),
      soundNote(0.22, 0.14, 550, 550, "triangle"),
      soundNote(0.44, 0.14, 660, 660, "triangle"),
      soundNote(0.7, 0.65, 880, 880, "triangle"),
    ],
  },
};

const bounded = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

/** Deterministic production synthesis shared with the Storybook sound adapter. */
export function synthesizeSound(
  value: SoundDefinition,
  sampleRate: number,
  pitch = 0,
  stretch = 1,
): Float32Array {
  if (!value.notes?.length) throw new Error(`Sound ${value.id} has no synthesis definition.`);
  const ratio = value.fixed ? 1 : 2 ** (bounded(pitch, -12, 12) / 12);
  const scale = value.fixed ? 1 : bounded(stretch, 0.5, 2);
  const duration =
    (value.duration ?? Math.max(...value.notes.map((note) => note.at + note.length))) * scale;
  const samples = new Float32Array(Math.ceil(duration * sampleRate));
  for (const note of value.notes) {
    const first = Math.round(note.at * scale * sampleRate);
    const length = note.length * scale;
    for (let i = 0; i < length * sampleRate && first + i < samples.length; i++) {
      const time = i / sampleRate;
      const cycles =
        ratio * (note.from * time + ((note.to - note.from) * time * time) / (2 * length));
      const envelope = value.fixed
        ? 1 - time / length
        : Math.min(1, time / 0.004) * (1 - time / length) ** 2;
      samples[first + i] = (samples[first + i] ?? 0) + voice(note, cycles, time) * envelope * 0.08;
    }
  }
  return value.fixed ? samples : normalizeSound(samples);
}

function voice(note: SoundNote, cycles: number, time: number): number {
  const phase = 2 * Math.PI * cycles;
  switch (note.voice) {
    case "sine":
      return Math.sin(phase);
    case "triangle":
      return (2 / Math.PI) * Math.asin(Math.sin(phase));
    case "bell":
      return Math.sin(phase) + 0.35 * Math.sin(phase * 2.76) * Math.exp(-14 * time);
    case "wood":
      return (
        Math.sin(phase) * Math.exp(-30 * time) + 0.3 * Math.sin(phase * 1.73) * Math.exp(-55 * time)
      );
    case "noise":
      return (Math.sin(phase * 1.79) + Math.sin(phase * 2.93) + Math.sin(phase * 4.71)) / 3;
  }
}

/** RMS normalization with a sample-peak cap; silence remains silence. */
export function normalizeSound(samples: Float32Array): Float32Array {
  let energy = 0;
  let peak = 0;
  for (const sample of samples) {
    energy += sample * sample;
    peak = Math.max(peak, Math.abs(sample));
  }
  const rms = Math.sqrt(energy / samples.length);
  const gain = rms > 0.000001 ? Math.min(0.028 / rms, 0.08 / peak) : 0;
  return samples.map((sample) => sample * gain);
}
