<script setup lang="ts">
import {
  computed,
  onBeforeUnmount,
  onMounted,
  provide,
  reactive,
  ref,
  shallowRef,
  watch,
  type WatchStopHandle,
} from "vue";
import CaptureAlertsView from "../../alerts/CaptureAlerts.vue";
import {
  captureAlertId,
  captureAlertsKey,
  createCaptureAlerts,
} from "../../alerts/capture-alerts.ts";
import {
  candidate,
  candidates,
  palettes,
  productionPalette,
  roleNames,
  roles,
  type Role,
} from "./candidates.ts";
import ProductionCapture from "./ProductionCapture.vue";
import { createLabScenario, scenarios, type LabScenario, type ScenarioName } from "./scenario.ts";
import { createLabSound, type SoundSettings } from "./sound.ts";
import { createScenarioPlayback, type ScenarioPlayback } from "./playback.ts";

const props = withDefaults(
  defineProps<{
    scenarioName?: ScenarioName;
    clockMode?: "automatic" | "manual" | "real-time";
    playbackSpeed?: number;
    intervalSeconds?: number;
    securingDeadlineMs?: number;
    simulatePlaybackFailure?: boolean;
  }>(),
  {
    scenarioName: "palette",
    clockMode: "manual",
    playbackSpeed: 1,
    intervalSeconds: 5,
    securingDeadlineMs: 2000,
    simulatePlaybackFailure: false,
  },
);
const selectedPalette = ref(productionPalette.id);
const defaults = productionPalette;
const settings = reactive<SoundSettings>({
  choices: { ...defaults.choices },
  levels: { ...defaults.levels },
  master: 1,
  pitch: 0,
  duration: 1,
  problemGap: 0,
  unavailable: props.simulatePlaybackFailure,
});
const scenario = shallowRef<LabScenario | null>(null);
const revision = ref(0);
const playback = shallowRef<ScenarioPlayback | null>(null);
let stopCaptureWatch: WatchStopHandle | null = null;
const busy = ref(false);
const enabling = ref(false);
const audioError = ref("");
const setupError = ref("");
const activity = ref<{ number: number; revision: number; time: string; message: string }[]>([]);
let entryNumber = 0;
let unmounted = false;
function log(message: string): void {
  if (unmounted) return;
  activity.value.push({
    number: ++entryNumber,
    revision: revision.value,
    time: ((scenario.value?.scheduler.now() ?? 0) / 1000).toFixed(2),
    message,
  });
}
const sound = createLabSound(settings, log, (message) => {
  audioError.value = message;
});
const alerts = createCaptureAlerts(sound.cues);
provide(captureAlertsKey, alerts);
const instruction = computed(() => scenarios[props.scenarioName]);
const effectiveSpeed = computed(() => (props.clockMode === "real-time" ? 1 : props.playbackSpeed));
const playLabel = computed(() => {
  const state = playback.value?.status.value;
  if (state === "paused") return "Resume scenario";
  const prefix = alerts.alerts.value.length ? "Acknowledge and " : "";
  return `${prefix}${state === "finished" ? "replay" : "play"} scenario`.replace(
    /^./,
    (character) => character.toUpperCase(),
  );
});
const intervalMs = computed(() => props.intervalSeconds * 1000);
const palette = computed(() => palettes.find((value) => value.id === selectedPalette.value)!);
function resetPalette(): void {
  settings.choices = { ...palette.value.choices };
  settings.levels = { ...palette.value.levels };
  settings.master = 1;
  settings.pitch = 0;
  settings.duration = 1;
  settings.problemGap = palette.value.problemGap;
  log(`Palette defaults: ${palette.value.id}; master 100%`);
}
watch(selectedPalette, resetPalette);
watch(
  () => props.simulatePlaybackFailure,
  (value) => {
    settings.unavailable = value;
  },
);
function rebuild(): void {
  stopCaptureWatch?.();
  stopCaptureWatch = null;
  playback.value?.dispose();
  playback.value = null;
  scenario.value?.dispose();
  scenario.value = null;
  revision.value++;
  setupError.value = "";
  try {
    if (
      !Number.isFinite(props.intervalSeconds) ||
      props.intervalSeconds < 1 ||
      props.intervalSeconds > 20
    )
      throw new Error("The laboratory frame interval must be between 1 and 20 seconds.");
    if (
      !Number.isFinite(props.playbackSpeed) ||
      props.playbackSpeed < 1 ||
      props.playbackSpeed > 10
    )
      throw new Error("Scenario playback speed must be between 1 and 10.");
    const next = createLabScenario({
      name: props.scenarioName,
      manual: true,
      intervalMs: intervalMs.value,
      deadlineMs: props.securingDeadlineMs,
      cues: sound.cues,
      alerts,
      log,
    });
    scenario.value = next;
    const controller = createScenarioPlayback(
      next,
      props.scenarioName,
      () => effectiveSpeed.value,
      log,
      (error) => {
        setupError.value = String(error);
      },
    );
    playback.value = controller;
    stopCaptureWatch = watch(
      () => next.app.snapshot.value.capturing,
      (capturing) => {
        if (capturing && props.clockMode !== "manual" && controller.status.value === "ready") {
          void controller.play().catch((error) => {
            setupError.value = String(error);
          });
        }
      },
    );
    log(
      `Fresh simulated Section lifecycle: ${props.scenarioName}; ${props.clockMode}; interval ${props.intervalSeconds}s; deadline ${props.securingDeadlineMs}ms. Palette retained.`,
    );
  } catch (error) {
    setupError.value = String(error);
    log(`ERROR: ${String(error)}`);
  }
}
watch(
  () => [props.scenarioName, props.clockMode, props.intervalSeconds, props.securingDeadlineMs],
  rebuild,
);
onMounted(rebuild);
onBeforeUnmount(() => {
  unmounted = true;
  stopCaptureWatch?.();
  playback.value?.dispose();
  scenario.value?.dispose();
  alerts.dispose();
  sound.cues.dispose();
});
async function enable(): Promise<void> {
  enabling.value = true;
  await sound.enable();
  enabling.value = false;
}
async function playScenario(): Promise<void> {
  if (!playback.value) return;
  if (playback.value.status.value === "finished") {
    alerts.acknowledgeAll();
    rebuild();
  } else if (playback.value.status.value === "ready") {
    alerts.acknowledgeAll();
  }
  const current = playback.value;
  await enable();
  if (current !== playback.value || unmounted) return;
  try {
    await current?.play();
  } catch (error) {
    setupError.value = String(error);
  }
}
async function advance(ms: number): Promise<void> {
  if (busy.value) return;
  const current = scenario.value;
  busy.value = true;
  try {
    await current?.advance(ms);
  } catch (error) {
    setupError.value = String(error);
  } finally {
    busy.value = false;
  }
}
function audition(role: Role, id?: string): void {
  if (id) settings.choices[role] = id;
  log(`Manual semantic audition: ${roleNames[role]} (no Section state change)`);
  if (role === "problem") {
    alerts.raise({ kind: "audio-asset-sealing-failed", sectionId: "sound-lab-audition" });
    return;
  }
  sound.cues.play(role);
}
const auditionAlertId = captureAlertId({
  kind: "audio-asset-sealing-failed",
  sectionId: "sound-lab-audition",
});
const auditionAlertActive = computed(() =>
  alerts.alerts.value.some((alert) => captureAlertId(alert) === auditionAlertId),
);
const overdueFrameActive = computed(() =>
  alerts.alerts.value.some((alert) => alert.kind === "transport-frame-overdue"),
);
</script>

<template>
  <main class="sound-lab">
    <header>
      <h1>Capture sound laboratory</h1>
      <p>Simulated Capture · No microphone, backend, credentials, or saved Project data</p>
      <p>{{ instruction }}</p>
      <button type="button" :disabled="enabling" @click="enable">
        {{ enabling ? "Loading local sounds…" : "Enable laboratory sound" }}
      </button>
      <div v-if="clockMode !== 'manual' && playback" class="scenario-playback">
        <button v-if="playback.status.value === 'playing'" type="button" @click="playback.pause()">
          Pause scenario
        </button>
        <button v-else type="button" class="primary" :disabled="enabling" @click="playScenario">
          {{ playLabel }}
        </button>
        <p role="status">
          Scenario {{ playback.status.value }} ·
          {{ (playback.positionMs.value / 1000).toFixed(1) }} simulated seconds ·
          {{ effectiveSpeed }}× speed
        </p>
        <p>
          Play enables sound and runs the arranged actions automatically. Pause freezes simulated
          time; repeating sounds continue until Acknowledge. Finished scenarios leave active alerts
          available for review.
        </p>
      </div>
      <p v-else>
        Manual clock: use the production Start control, then the Advance buttons. Select automatic
        review in Storybook Controls to listen to the arranged scenario.
      </p>
      <p v-if="scenarioName === 'single'">
        To repeat: choose <strong>Replay scenario</strong> after automatic playback finishes, or
        choose <strong>Restart this scenario</strong> and use Start again. In manual review, advance
        one frame interval and one deadline, listen for as long as needed, then choose
        <strong>Secure overdue frame</strong>.
      </p>
      <p v-if="audioError" role="status" class="warning">{{ audioError }}</p>
      <p v-if="setupError" role="alert" class="warning">{{ setupError }}</p>
    </header>
    <p v-if="auditionAlertActive" role="status" class="problem-preview">
      This Capture alert was raised by the laboratory's direct Problem audition.
    </p>
    <CaptureAlertsView />
    <section aria-labelledby="palette-heading">
      <h2 id="palette-heading">Palette and listening level</h2>
      <div class="controls">
        <label
          >Complete palette
          <select v-model="selectedPalette">
            <option v-for="value in palettes" :key="value.id" :value="value.id">
              {{ value.name }}
            </option>
          </select>
        </label>
        <label
          >Master volume: {{ Math.round(settings.master * 100) }}%
          <input v-model.number="settings.master" type="range" min="0" max="1" step="0.05" />
        </label>
        <label
          >Synthesized pitch: {{ settings.pitch }} semitones
          <input v-model.number="settings.pitch" type="range" min="-12" max="12" step="1" />
        </label>
        <label
          >Synthesized duration: {{ settings.duration }}×
          <input v-model.number="settings.duration" type="range" min="0.5" max="2" step="0.1" />
        </label>
        <label
          >Repeating sound gap:
          {{ settings.problemGap === 0 ? "continuous, no added pause" : `${settings.problemGap}s` }}
          <input v-model.number="settings.problemGap" type="range" min="0" max="3" step="0.1" />
        </label>
        <button type="button" @click="resetPalette">Reset to selected palette defaults</button>
      </div>
      <p>
        Production start stays fixed. Pitch and duration affect synthesized alternatives only; fixed
        definitions and downloaded assets stay fixed. Problem repeats through the production
        Capture-alert module until every alert resolves or Acknowledge clears them.
      </p>
      <p>
        Each role can use any candidate. The lists group sounds by their initially proposed role.
      </p>
      <div class="roles">
        <article v-for="role in roles" :key="role">
          <h3>{{ roleNames[role] }}</h3>
          <label
            >Candidate for {{ roleNames[role].toLowerCase() }}
            <select v-model="settings.choices[role]">
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
          <label
            >Relative volume: {{ Math.round(settings.levels[role] * 100) }}%
            <input
              v-model.number="settings.levels[role]"
              type="range"
              min="0"
              max="1.5"
              step="0.05"
            />
          </label>
          <button type="button" @click="audition(role)">
            {{ role === "problem" ? "Audition Problem" : "Audition selected sound" }}
          </button>
          <small
            >{{ candidate(settings.choices[role]).id }} ·
            {{ candidate(settings.choices[role]).license }}</small
          >
        </article>
      </div>
    </section>
    <section aria-labelledby="simulation-heading">
      <h2 id="simulation-heading">Production behavior with simulated inputs</h2>
      <p>
        Use Storybook Controls to change the clock, playback speed, frame interval (1–20 seconds),
        or securing deadline (500–2000 ms). Timing changes create a fresh scenario and retain the
        palette and numbered history.
      </p>
      <p>
        <strong>{{
          intervalSeconds === 5
            ? "Production baseline: 5-second frame interval"
            : `Experimental frame interval: ${intervalSeconds}s (production: 5s)`
        }}</strong>
        · Securing deadline {{ securingDeadlineMs }} ms · {{ clockMode }} clock
      </p>
      <p>
        Real-time waiting beyond three seconds is a local exception to ADR 0002 for this
        timing-perception experiment. Speak continuously during real-time review. Manual advancement
        controls Capture and delivery; repeating audio keeps real time. Automatic playback advances
        the injected clock continuously and runs the arranged Start, Stop, recovery, and sealing
        actions.
      </p>
      <ProductionCapture v-if="scenario" :key="revision" :scenario="scenario" />
      <div v-if="scenario" class="actions">
        <template v-if="clockMode === 'manual'">
          <button
            type="button"
            :disabled="busy || (scenarioName === 'single' && overdueFrameActive)"
            @click="advance(100)"
          >
            Advance 100 ms
          </button>
          <button
            type="button"
            :disabled="busy || (scenarioName === 'single' && overdueFrameActive)"
            @click="advance(securingDeadlineMs)"
          >
            Advance deadline
          </button>
          <button
            type="button"
            :disabled="busy || (scenarioName === 'single' && overdueFrameActive)"
            @click="advance(intervalMs)"
          >
            Advance one frame interval
          </button>
          <button
            v-if="scenarioName !== 'single'"
            type="button"
            :disabled="busy"
            @click="advance(intervalMs * 3 + 100)"
          >
            Advance three frame intervals
          </button>
          <button
            v-if="scenario.config.maxRecordingMs < 600000"
            type="button"
            :disabled="busy"
            @click="
              advance(
                Math.max(0, scenario.config.maxRecordingMs - scenario.app.snapshot.value.elapsedMs),
              )
            "
          >
            Advance to duration limit
          </button>
        </template>
        <button
          v-if="scenarioName === 'single'"
          type="button"
          :disabled="!overdueFrameActive"
          @click="scenario.recover()"
        >
          Secure overdue frame
        </button>
        <template v-else>
          <button type="button" @click="scenario.recover()">Recover simulated delivery</button>
          <button type="button" @click="scenario.confirmSealing()">
            Confirm simulated sealing
          </button>
          <button type="button" @click="scenario.failCapture()">Simulate recorder failure</button>
          <button type="button" @click="scenario.failFinalization()">
            Arm finalization failure for next Stop
          </button>
          <button type="button" @click="scenario.failNextSeal()">
            Arm sealing failure for next Stop
          </button>
        </template>
        <button type="button" @click="rebuild">Restart this scenario</button>
      </div>
    </section>
    <details :open="scenarioName === 'palette'">
      <summary>All candidates and provenance ({{ candidates.length }})</summary>
      <p>
        Audition selects the candidate for its role. Problem auditions raise a production Capture
        alert and repeat until Acknowledge. Downloaded assets are bundled, mixed to mono, and
        normalized during playback; source files are unchanged.
      </p>
      <section v-for="role in roles" :key="role" class="catalog">
        <h2>{{ roleNames[role] }}</h2>
        <article v-for="value in candidates.filter((item) => item.role === role)" :key="value.id">
          <button type="button" @click="audition(role, value.id)">Audition {{ value.name }}</button>
          <div>
            <strong>{{ value.id }}</strong> · {{ value.family }}<br />
            {{ value.author }} · {{ value.license }} · {{ value.attribution }}<br />
            <a v-if="value.asset" :href="value.source" target="_blank" rel="noreferrer"
              >Original source: {{ value.source }}</a
            >
            <span v-else>{{ value.source }}</span>
          </div>
        </article>
      </section>
    </details>
    <section aria-labelledby="review-heading">
      <h2 id="review-heading">Listening review</h2>
      <p>
        Prioritize low disruption to continuous speaking. Compare reassurance, urgency,
        distinguishability, and distraction. Try speakers and headphones when available. Keep
        Production as your reference.
      </p>
      <p>
        Record findings in the GitHub issue that requested the current experiment, using candidate
        IDs, palette, tuning, story, clock mode, and playback equipment. Nothing is saved by this
        workbench.
      </p>
    </section>
    <details open>
      <summary>Numbered activity history (retained across scenario rebuilds)</summary>
      <ol class="activity" aria-label="Sound laboratory activity" aria-live="off">
        <li v-for="entry in activity" :key="entry.number" :value="entry.number">
          q{{ entry.revision }} · {{ entry.time }}s · {{ entry.message }}
        </li>
      </ol>
    </details>
  </main>
</template>

<style scoped>
.sound-lab {
  max-width: 1100px;
  margin: auto;
  padding: 1.5rem;
  display: grid;
  gap: 1.5rem;
  color: var(--text);
}
h1,
h2,
h3 {
  margin: 0 0 0.75rem;
}
p {
  max-width: 85ch;
  line-height: 1.55;
}
.controls,
.roles {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 1rem;
}
label {
  display: grid;
  gap: 0.4rem;
}
select {
  width: 100%;
  min-height: 2.5rem;
}
input[type="range"] {
  width: 100%;
}
.roles article {
  padding: 1rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  display: grid;
  gap: 0.75rem;
  align-content: start;
}
small {
  overflow-wrap: anywhere;
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 1rem;
}
.catalog {
  margin: 1rem 0;
}
.catalog article {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 1rem;
  padding: 0.75rem 0;
  border-bottom: 1px solid var(--line);
  font-size: 0.9rem;
  overflow-wrap: anywhere;
}
.catalog button {
  flex: 0 0 16rem;
}
.activity {
  max-height: 22rem;
  overflow: auto;
  font: 0.85rem/1.6 monospace;
  padding-left: 4rem;
}
.warning {
  color: var(--danger);
  border-left: 4px solid currentcolor;
  padding: 0.75rem;
}
.problem-preview {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
  padding: 1rem;
  border: 2px solid var(--warning);
}
.repeat-problem {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 1rem;
}
summary {
  cursor: pointer;
  font-weight: 650;
}
</style>
