export const roles = ["health", "delayed", "stop", "interrupted"] as const;
export type Role = (typeof roles)[number];
export const roleNames: Record<Role, string> = {
  health: "Healthy heartbeat",
  delayed: "Problem",
  stop: "Capture completed",
  interrupted: "Unrequested-stop alarm",
};

type Voice = "sine" | "triangle" | "bell" | "wood" | "noise";
export interface Note {
  at: number;
  length: number;
  from: number;
  to: number;
  voice: Voice;
}
export interface Candidate {
  id: string;
  name: string;
  role: Role;
  family: string;
  notes?: readonly Note[];
  duration?: number;
  asset?: string;
  baseline?: boolean;
  author: string;
  source: string;
  license: string;
  attribution: string;
}
const note = (
  at: number,
  length: number,
  from: number,
  to = from,
  voice: Voice = "sine",
): Note => ({
  at,
  length,
  from,
  to,
  voice,
});
const morse = (pattern: string): Note[] => {
  let at = 0.3;
  return [
    note(0, 0.15, 220, 160),
    ...[...pattern].map((symbol) => {
      const length = symbol === "." ? 0.08 : 0.24;
      const result = note(at, length, 660);
      at += length + 0.08;
      return result;
    }),
  ];
};
function synth(
  role: Role,
  id: string,
  name: string,
  family: string,
  notes: Note[],
  duration?: number,
): Candidate {
  return {
    role,
    id,
    name,
    family,
    notes,
    ...(duration === undefined ? {} : { duration }),
    author: "Running Drafts contributors",
    source: "Local definition in candidates.ts",
    license: "Original project synthesis; no third-party assets",
    attribution: "No third-party material",
  };
}
function baseline(role: Role, notes: Note[], duration: number): Candidate {
  return {
    ...synth(
      role,
      `baseline-${role}`,
      `Production ${roleNames[role].toLowerCase()}`,
      "Production baseline",
      notes,
      duration,
    ),
    baseline: true,
  };
}
function asset(role: Role, file: string, name: string, family: string): Candidate {
  return {
    role,
    id: `kenney-${file}`,
    name,
    family,
    asset: new URL(`./assets/${file}.ogg`, import.meta.url).href,
    author: "Kenney",
    source: `https://kenney.nl/assets/interface-sounds#${file}.ogg`,
    license: "CC0 1.0 Universal",
    attribution: "None required; optional credit: Kenney (www.kenney.nl)",
  };
}

/** Add one definition here and optionally refer to its stable ID in a palette below. */
export const candidates: readonly Candidate[] = [
  baseline("health", [note(0, 0.1, 660, 880)], 0.1),
  synth("health", "felt-pulse", "Felt pulse", "Soft triangle", [
    note(0, 0.12, 420, 420, "triangle"),
  ]),
  synth("health", "wood-tap", "Wood tap", "Percussion", [note(0, 0.07, 630, 260, "wood")]),
  synth("health", "glass-drop", "Glass drop", "Bell", [note(0, 0.23, 1050, 1050, "bell")]),
  synth("health", "breath-tick", "Breath tick", "Filtered noise", [
    note(0, 0.065, 1100, 700, "noise"),
  ]),
  synth("health", "soft-pair", "Soft pair", "Paired sine", [
    note(0, 0.07, 520),
    note(0.1, 0.08, 650),
  ]),
  synth("health", "low-bloom", "Low bloom", "Low sine", [note(0, 0.18, 260, 360)]),
  asset("health", "tick_001", "Kenney tick", "Mechanical tick"),
  asset("health", "pluck_001", "Kenney pluck", "Plucked string"),
  asset("health", "glass_001", "Kenney glass", "Glass percussion"),

  baseline("delayed", morse("..-"), 1.02),
  synth("delayed", "hollow-knock", "Hollow knock", "Wooden pair", [
    note(0, 0.16, 330, 200, "wood"),
    note(0.25, 0.16, 330, 200, "wood"),
  ]),
  synth("delayed", "minor-second", "Minor second", "Dissonant bell", [
    note(0, 0.35, 520, 520, "bell"),
    note(0, 0.35, 550, 550, "bell"),
  ]),
  synth("delayed", "downward-call", "Downward call", "Falling triangle", [
    note(0, 0.35, 820, 380, "triangle"),
  ]),
  synth("delayed", "dry-rattle", "Dry rattle", "Noise pulses", [
    note(0, 0.09, 1300, 700, "noise"),
    note(0.14, 0.09, 1300, 700, "noise"),
    note(0.28, 0.09, 1300, 700, "noise"),
  ]),
  synth("delayed", "double-bell", "Double bell", "Bell pair", [
    note(0, 0.22, 740, 740, "bell"),
    note(0.3, 0.3, 620, 620, "bell"),
  ]),
  synth("delayed", "low-wobble", "Low wobble", "Sine triplet", [
    note(0, 0.12, 350, 440),
    note(0.14, 0.12, 440, 350),
    note(0.28, 0.12, 350, 440),
  ]),
  asset("delayed", "error_001", "Kenney error", "Electronic error"),
  asset("delayed", "scratch_001", "Kenney scratch", "Friction"),
  asset("delayed", "question_001", "Kenney question", "Question gesture"),

  baseline("stop", [note(0, 0.15, 880, 440)], 0.15),
  synth("stop", "settled-chord", "Settled chord", "Warm chord", [
    note(0, 0.45, 440),
    note(0, 0.45, 550),
    note(0, 0.45, 660),
  ]),
  synth("stop", "closing-bell", "Closing bell", "Bell cadence", [
    note(0, 0.25, 880, 880, "bell"),
    note(0.22, 0.4, 660, 660, "bell"),
  ]),
  synth("stop", "wood-cadence", "Wood cadence", "Percussion cadence", [
    note(0, 0.12, 660, 400, "wood"),
    note(0.18, 0.23, 440, 220, "wood"),
  ]),
  synth("stop", "gentle-rise", "Gentle rise", "Triangle cadence", [
    note(0, 0.16, 440, 440, "triangle"),
    note(0.16, 0.3, 660, 660, "triangle"),
  ]),
  synth("stop", "triangle-finish", "Triangle finish", "Four-note triangle cadence", [
    note(0, 0.14, 440, 440, "triangle"),
    note(0.22, 0.14, 550, 550, "triangle"),
    note(0.44, 0.14, 660, 660, "triangle"),
    note(0.7, 0.65, 880, 880, "triangle"),
  ]),
  asset("stop", "confirmation_001", "Kenney confirmation", "Confirmation"),
  asset("stop", "maximize_001", "Kenney maximize", "Expanding gesture"),
  asset("stop", "open_001", "Kenney open", "Opening gesture"),

  { ...baseline("interrupted", morse("-..-"), 1.18), name: "Morse Stop" },
  synth(
    "interrupted",
    "alternating-call",
    "Alternating call",
    "Two-tone call",
    [note(0, 0.25, 660, 660, "triangle"), note(0.3, 0.25, 880, 880, "triangle")],
    0.6,
  ),
  synth(
    "interrupted",
    "urgent-bell",
    "Urgent bell",
    "Repeated bell",
    [note(0, 0.3, 940, 940, "bell"), note(0.35, 0.3, 940, 940, "bell")],
    0.7,
  ),
  synth(
    "interrupted",
    "siren-sweep",
    "Siren sweep",
    "Continuous sweep",
    [note(0, 0.5, 450, 950), note(0.5, 0.5, 950, 450)],
    1,
  ),
  synth(
    "interrupted",
    "rough-beacon",
    "Rough beacon",
    "Noise and tone",
    [note(0, 0.3, 800, 400, "noise"), note(0.35, 0.3, 620, 620, "triangle")],
    0.7,
  ),
  asset("interrupted", "bong_001", "Kenney bong", "Resonant strike"),
  asset("interrupted", "glitch_001", "Kenney glitch", "Digital interruption"),
  asset("interrupted", "error_004", "Kenney error beacon", "Electronic warning"),
];

export interface Palette {
  id: string;
  name: string;
  choices: Record<Role, string>;
  levels: Record<Role, number>;
  alarmGap: number;
  repeatProblem?: boolean;
}
export const productionCandidate: Palette = {
  id: "production-candidate",
  name: "Production candidate",
  choices: {
    health: "gentle-rise",
    delayed: "baseline-interrupted",
    stop: "triangle-finish",
    interrupted: "baseline-interrupted",
  },
  levels: { health: 1, delayed: 1, stop: 1, interrupted: 1 },
  alarmGap: 0,
  repeatProblem: true,
};
// Same fixed rising gesture as CuePlayer.play("start"); master gain is laboratory-only.
export const fixedStart: Candidate = {
  ...baseline("health", [note(0, 0.15, 440, 880)], 0.15),
  id: "production-start",
  name: "Fixed production Capture-started cue",
};
const levels: Record<Role, number> = { health: 0.55, delayed: 0.85, stop: 0.75, interrupted: 1 };
export const palettes: readonly Palette[] = [
  {
    id: "production",
    name: "Production baseline",
    choices: {
      health: "baseline-health",
      delayed: "baseline-delayed",
      stop: "baseline-stop",
      interrupted: "baseline-interrupted",
    },
    levels: { health: 1, delayed: 1, stop: 1, interrupted: 1 },
    alarmGap: 0,
  },
  productionCandidate,
  {
    id: "soft",
    name: "Soft and tactile",
    choices: {
      health: "felt-pulse",
      delayed: "hollow-knock",
      stop: "wood-cadence",
      interrupted: "alternating-call",
    },
    levels,
    alarmGap: 0.35,
  },
  {
    id: "glass",
    name: "Glass and bells",
    choices: {
      health: "glass-drop",
      delayed: "double-bell",
      stop: "closing-bell",
      interrupted: "urgent-bell",
    },
    levels,
    alarmGap: 0.5,
  },
  {
    id: "minimal",
    name: "Minimal signals",
    choices: {
      health: "breath-tick",
      delayed: "downward-call",
      stop: "settled-chord",
      interrupted: "siren-sweep",
    },
    levels,
    alarmGap: 0,
  },
  {
    id: "kenney",
    name: "Kenney interface",
    choices: {
      health: "kenney-pluck_001",
      delayed: "kenney-error_001",
      stop: "kenney-confirmation_001",
      interrupted: "kenney-error_004",
    },
    levels,
    alarmGap: 0.5,
  },
];
export function candidate(id: string): Candidate {
  const found = candidates.find((value) => value.id === id);
  if (!found) throw new Error(`Unknown sound candidate: ${id}`);
  return found;
}
