<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef } from "vue";
import { DEFAULT_CONFIG } from "../../shared/config.ts";
import { RadarLog } from "../../shared/radar-log.ts";
import RadarLogView from "../../shared/RadarLogView.vue";
import { CuePlayer, type SoundAssignments } from "../cues.ts";
import { candidate, candidates, roleNames, roles } from "./candidates.ts";
import {
  assignCandidate,
  feedbackRoleNames,
  feedbackRoles,
  restoreProduction,
  soundSelection,
} from "./sound-store.ts";

const selectedId = ref(candidates[0]!.id);
const repeatSeconds = ref(1);
const repeatCount = ref(1);
const log = new RadarLog(
  DEFAULT_CONFIG.logColumns,
  DEFAULT_CONFIG.logRows,
  DEFAULT_CONFIG.logGapChars,
);
const logView = shallowRef(log.view());
const openedAt = globalThis.performance.now();
let player: CuePlayer | null = null;
let timer: ReturnType<typeof globalThis.setTimeout> | null = null;
let remaining = 0;

const selected = computed(() => candidate(selectedId.value));

function write(message: string): void {
  log.write(`${((globalThis.performance.now() - openedAt) / 1000).toFixed(1)} ${message}`);
  logView.value = log.view();
}

function stopAudition(): void {
  if (timer !== null) globalThis.clearTimeout(timer);
  timer = null;
  if (player) {
    player.dispose();
    player = null;
    write("audition stopped");
  }
}

function auditionOnce(): void {
  if (!player || remaining <= 0) return;
  const number = repeatCount.value - remaining + 1;
  player.play("capture-started");
  write(`audition ${selected.value.id} repeat ${number}/${repeatCount.value}`);
  remaining--;
  if (remaining > 0) timer = globalThis.setTimeout(auditionOnce, repeatSeconds.value * 1000);
}

async function audition(): Promise<void> {
  stopAudition();
  const definition = selected.value;
  const assignments = Object.fromEntries(
    feedbackRoles.map((role) => [role, definition]),
  ) as unknown as SoundAssignments;
  player = new CuePlayer((role, phase) => write(`sound ${role} ${phase}`), assignments);
  await player.prepare();
  remaining = Math.max(1, Math.trunc(repeatCount.value));
  write(`audition started: ${definition.id}`);
  auditionOnce();
}

function toggleAssignment(role: (typeof feedbackRoles)[number], checked: boolean): void {
  if (checked) assignCandidate(role, selected.value.id);
  else restoreProduction(role);
  write(`${feedbackRoleNames[role]} ${checked ? "assigned" : "restored"}: ${soundSelection[role]}`);
}

onBeforeUnmount(stopAudition);
</script>

<template>
  <main class="sound-configuration">
    <header>
      <h1>Sound configuration</h1>
      <p>
        Audition bundled candidates and assign them to production cue meanings. Assignments affect
        Capture stories during this Storybook page lifetime and reset on refresh.
      </p>
    </header>

    <section aria-labelledby="candidate-heading">
      <h2 id="candidate-heading">Candidate</h2>
      <label>
        Sound candidate
        <select v-model="selectedId" @change="write(`candidate selected: ${selectedId}`)">
          <optgroup v-for="group in roles" :key="group" :label="roleNames[group]">
            <option
              v-for="value in candidates.filter((item) => item.role === group)"
              :key="value.id"
              :value="value.id"
            >
              {{ value.name }} · {{ value.family }}
            </option>
          </optgroup>
        </select>
      </label>
      <dl class="provenance">
        <div>
          <dt>Source</dt>
          <dd>{{ selected.source }}</dd>
        </div>
        <div>
          <dt>Author</dt>
          <dd>{{ selected.author }}</dd>
        </div>
        <div>
          <dt>License</dt>
          <dd>{{ selected.license }}</dd>
        </div>
        <div>
          <dt>Attribution</dt>
          <dd>{{ selected.attribution }}</dd>
        </div>
      </dl>
    </section>

    <section aria-labelledby="audition-heading">
      <h2 id="audition-heading">Local audition</h2>
      <label
        >Repeat every
        <input v-model.number="repeatSeconds" type="number" min="0.1" step="0.1" /> seconds</label
      >
      <label
        >Stop after
        <input v-model.number="repeatCount" type="number" min="1" step="1" /> repeats</label
      >
      <button type="button" @click="audition">Audition</button>
      <button type="button" @click="stopAudition">Stop audition</button>
      <p>These repetition settings apply only to this local audition.</p>
    </section>

    <fieldset>
      <legend>Assign selected candidate to logical meanings</legend>
      <label v-for="role in feedbackRoles" :key="role">
        <input
          type="checkbox"
          :checked="soundSelection[role] === selected.id"
          @change="toggleAssignment(role, ($event.target as HTMLInputElement).checked)"
        />
        {{ feedbackRoleNames[role] }}
      </label>
    </fieldset>

    <RadarLogView
      :lines="logView.lines"
      :cursor="logView.cursor"
      :columns="DEFAULT_CONFIG.logColumns"
      label="Sound configuration activity log"
      test-id="sound-configuration-log"
    />
  </main>
</template>

<style scoped>
.sound-configuration {
  max-width: 900px;
  margin: auto;
  padding: 1.5rem;
  display: grid;
  gap: 1.5rem;
}
section,
fieldset {
  display: grid;
  gap: 0.75rem;
}
label {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}
select {
  max-width: 34rem;
}
.provenance {
  display: grid;
  gap: 0.5rem;
}
.provenance div {
  display: grid;
  grid-template-columns: 7rem 1fr;
  gap: 0.75rem;
}
.provenance dt {
  font-weight: 700;
}
.provenance dd {
  margin: 0;
  overflow-wrap: anywhere;
}
</style>
