import { reactive } from "vue";
import { productionSoundAssignments, type FeedbackRole, type SoundAssignments } from "../cues.ts";
import { candidate, candidates } from "./candidates.ts";

export const feedbackRoles: readonly FeedbackRole[] = [
  "capture-started",
  "healthy-heartbeat",
  "allowance-warning",
  "problem",
  "capture-completed",
];

export const feedbackRoleNames: Record<FeedbackRole, string> = {
  "capture-started": "Capture-started",
  "healthy-heartbeat": "Healthy heartbeat",
  "allowance-warning": "Allowance warning",
  problem: "Problem",
  "capture-completed": "Capture-completed",
};

const productionIds: Record<FeedbackRole, string> = {
  "capture-started": productionSoundAssignments["capture-started"].id,
  "healthy-heartbeat": productionSoundAssignments["healthy-heartbeat"].id,
  "allowance-warning": productionSoundAssignments["allowance-warning"].id,
  problem: productionSoundAssignments.problem.id,
  "capture-completed": productionSoundAssignments["capture-completed"].id,
};

/** Storybook-preview lifetime state: navigation retains it and refresh reconstructs it. */
export const soundSelection = reactive<Record<FeedbackRole, string>>({ ...productionIds });

export function selectedSoundAssignments(): SoundAssignments {
  return Object.fromEntries(
    feedbackRoles.map((role) => [role, candidate(soundSelection[role])]),
  ) as unknown as SoundAssignments;
}

export function modifiedSoundAssignments(): { role: FeedbackRole; candidateName: string }[] {
  return feedbackRoles
    .filter((role) => soundSelection[role] !== productionIds[role])
    .map((role) => ({ role, candidateName: candidate(soundSelection[role]).name }));
}

export function assignCandidate(role: FeedbackRole, candidateId: string): void {
  if (!candidates.some((value) => value.id === candidateId)) {
    throw new Error(`Unknown sound candidate ${candidateId}.`);
  }
  soundSelection[role] = candidateId;
}

export function restoreProduction(role: FeedbackRole): void {
  soundSelection[role] = productionIds[role];
}
