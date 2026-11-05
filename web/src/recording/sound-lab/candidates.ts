import {
  productionSounds,
  soundNote,
  type SoundDefinition,
  type SoundNote,
} from "../sound-definitions.ts";

export const roles = ["healthy-heartbeat", "problem", "capture-completed"] as const;
export type Role = (typeof roles)[number];
export const roleNames: Record<Role, string> = {
  "healthy-heartbeat": "Healthy heartbeat",
  problem: "Problem",
  "capture-completed": "Capture completed",
};

export type Note = SoundNote;
export interface Candidate {
  id: string;
  name: string;
  role: Role;
  family: string;
  notes?: readonly Note[];
  duration?: number;
  asset?: string;
  fixed?: boolean;
  author: string;
  source: string;
  license: string;
  attribution: string;
}
const note = soundNote;
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
function fixed(
  role: Role,
  id: string,
  name: string,
  family: string,
  notes: Note[],
  duration: number,
): Candidate {
  return {
    ...synth(role, id, name, family, notes, duration),
    fixed: true,
  };
}
function promoted(value: SoundDefinition, role: Role, family: string): Candidate {
  return {
    ...value,
    role,
    family,
    author: "Running Drafts contributors",
    source: "Promoted production definition in sound-definitions.ts",
    license: "Original project synthesis; no third-party assets",
    attribution: "No third-party material",
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
  fixed(
    "healthy-heartbeat",
    "short-rising-tone",
    "Short rising tone",
    "Rising sine",
    [note(0, 0.1, 660, 880)],
    0.1,
  ),
  synth("healthy-heartbeat", "felt-pulse", "Felt pulse", "Soft triangle", [
    note(0, 0.12, 420, 420, "triangle"),
  ]),
  synth("healthy-heartbeat", "wood-tap", "Wood tap", "Percussion", [
    note(0, 0.07, 630, 260, "wood"),
  ]),
  synth("healthy-heartbeat", "glass-drop", "Glass drop", "Bell", [
    note(0, 0.23, 1050, 1050, "bell"),
  ]),
  synth("healthy-heartbeat", "breath-tick", "Breath tick", "Filtered noise", [
    note(0, 0.065, 1100, 700, "noise"),
  ]),
  synth("healthy-heartbeat", "soft-pair", "Soft pair", "Paired sine", [
    note(0, 0.07, 520),
    note(0.1, 0.08, 650),
  ]),
  synth("healthy-heartbeat", "low-bloom", "Low bloom", "Low sine", [note(0, 0.18, 260, 360)]),
  asset("healthy-heartbeat", "tick_001", "Kenney tick", "Mechanical tick"),
  asset("healthy-heartbeat", "pluck_001", "Kenney pluck", "Plucked string"),
  asset("healthy-heartbeat", "glass_001", "Kenney glass", "Glass percussion"),

  fixed("problem", "morse-u", "Morse U", "Morse signal", morse("..-"), 1.02),
  synth("problem", "hollow-knock", "Hollow knock", "Wooden pair", [
    note(0, 0.16, 330, 200, "wood"),
    note(0.25, 0.16, 330, 200, "wood"),
  ]),
  synth("problem", "minor-second", "Minor second", "Dissonant bell", [
    note(0, 0.35, 520, 520, "bell"),
    note(0, 0.35, 550, 550, "bell"),
  ]),
  synth("problem", "downward-call", "Downward call", "Falling triangle", [
    note(0, 0.35, 820, 380, "triangle"),
  ]),
  synth("problem", "dry-rattle", "Dry rattle", "Noise pulses", [
    note(0, 0.09, 1300, 700, "noise"),
    note(0.14, 0.09, 1300, 700, "noise"),
    note(0.28, 0.09, 1300, 700, "noise"),
  ]),
  synth("problem", "double-bell", "Double bell", "Bell pair", [
    note(0, 0.22, 740, 740, "bell"),
    note(0.3, 0.3, 620, 620, "bell"),
  ]),
  synth("problem", "low-wobble", "Low wobble", "Sine triplet", [
    note(0, 0.12, 350, 440),
    note(0.14, 0.12, 440, 350),
    note(0.28, 0.12, 350, 440),
  ]),
  asset("problem", "error_001", "Kenney error", "Electronic error"),
  asset("problem", "scratch_001", "Kenney scratch", "Friction"),
  asset("problem", "question_001", "Kenney question", "Question gesture"),

  fixed(
    "capture-completed",
    "short-falling-tone",
    "Short falling tone",
    "Falling sine",
    [note(0, 0.15, 880, 440)],
    0.15,
  ),
  synth("capture-completed", "settled-chord", "Settled chord", "Warm chord", [
    note(0, 0.45, 440),
    note(0, 0.45, 550),
    note(0, 0.45, 660),
  ]),
  synth("capture-completed", "closing-bell", "Closing bell", "Bell cadence", [
    note(0, 0.25, 880, 880, "bell"),
    note(0.22, 0.4, 660, 660, "bell"),
  ]),
  synth("capture-completed", "wood-cadence", "Wood cadence", "Percussion cadence", [
    note(0, 0.12, 660, 400, "wood"),
    note(0.18, 0.23, 440, 220, "wood"),
  ]),
  promoted(productionSounds["healthy-heartbeat"], "healthy-heartbeat", "Triangle cadence"),
  promoted(
    productionSounds["capture-completed"],
    "capture-completed",
    "Four-note triangle cadence",
  ),
  asset("capture-completed", "confirmation_001", "Kenney confirmation", "Confirmation"),
  asset("capture-completed", "maximize_001", "Kenney maximize", "Expanding gesture"),
  asset("capture-completed", "open_001", "Kenney open", "Opening gesture"),

  promoted(productionSounds.problem, "problem", "Morse signal"),
  synth(
    "problem",
    "alternating-call",
    "Alternating call",
    "Two-tone call",
    [note(0, 0.25, 660, 660, "triangle"), note(0.3, 0.25, 880, 880, "triangle")],
    0.6,
  ),
  synth(
    "problem",
    "urgent-bell",
    "Urgent bell",
    "Repeated bell",
    [note(0, 0.3, 940, 940, "bell"), note(0.35, 0.3, 940, 940, "bell")],
    0.7,
  ),
  synth(
    "problem",
    "siren-sweep",
    "Siren sweep",
    "Continuous sweep",
    [note(0, 0.5, 450, 950), note(0.5, 0.5, 950, 450)],
    1,
  ),
  synth(
    "problem",
    "rough-beacon",
    "Rough beacon",
    "Noise and tone",
    [note(0, 0.3, 800, 400, "noise"), note(0.35, 0.3, 620, 620, "triangle")],
    0.7,
  ),
  asset("problem", "bong_001", "Kenney bong", "Resonant strike"),
  asset("problem", "glitch_001", "Kenney glitch", "Digital interruption"),
  asset("problem", "error_004", "Kenney error beacon", "Electronic warning"),
];

export interface Palette {
  id: string;
  name: string;
  choices: Record<Role, string>;
  levels: Record<Role, number>;
  problemGap: number;
}
export const productionPalette: Palette = {
  id: "production",
  name: "Production",
  choices: {
    "healthy-heartbeat": "triangle-cadence",
    problem: "morse-stop",
    "capture-completed": "four-note-triangle-cadence",
  },
  levels: { "healthy-heartbeat": 1, problem: 1, "capture-completed": 1 },
  problemGap: 0,
};
export const fixedStart = productionSounds["capture-started"];
const levels: Record<Role, number> = {
  "healthy-heartbeat": 0.55,
  problem: 1,
  "capture-completed": 0.75,
};
export const palettes: readonly Palette[] = [
  productionPalette,
  {
    id: "soft",
    name: "Soft and tactile",
    choices: {
      "healthy-heartbeat": "felt-pulse",
      problem: "alternating-call",
      "capture-completed": "wood-cadence",
    },
    levels,
    problemGap: 0.35,
  },
  {
    id: "glass",
    name: "Glass and bells",
    choices: {
      "healthy-heartbeat": "glass-drop",
      problem: "urgent-bell",
      "capture-completed": "closing-bell",
    },
    levels,
    problemGap: 0.5,
  },
  {
    id: "minimal",
    name: "Minimal signals",
    choices: {
      "healthy-heartbeat": "breath-tick",
      problem: "siren-sweep",
      "capture-completed": "settled-chord",
    },
    levels,
    problemGap: 0,
  },
  {
    id: "kenney",
    name: "Kenney interface",
    choices: {
      "healthy-heartbeat": "kenney-pluck_001",
      problem: "kenney-error_004",
      "capture-completed": "kenney-confirmation_001",
    },
    levels,
    problemGap: 0.5,
  },
];
export function candidate(id: string): Candidate {
  const found = candidates.find((value) => value.id === id);
  if (!found) throw new Error(`Unknown sound candidate: ${id}`);
  return found;
}
