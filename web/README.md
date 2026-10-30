# Browser seed experiment

This directory is the home for the Running Drafts browser seed. Its current
#84/#85 slice supports authenticated browser recording, durable audio delivery,
Project browsing, and sealed original-audio download through the backend; the
broader seed also covers progressive transcription work.

[Product issue #1](https://github.com/olpa/running_drafts_product/issues/1)
coordinates the seed across repositories. Backend and inference implementation
belong to `olpa/running_drafts_backend`.

Shared [product intent](../docs/product.md) and [domain language](../GLOSSARY.md)
live at the repository root. The independent [Rust CLI proof of concept](../rust-poc/README.md) is
another technical experiment for the same product.
The [web glossary](GLOSSARY.md) defines browser recording and delivery terms.

## Authenticated recording seed

[Issue #84](https://github.com/olpa/running_drafts_editor/issues/84) connects the
Vite/TypeScript page to the authenticated backend Project contract:

- Capture: `MediaRecorderFrameSource` in `src/capture.ts` uses one continuous
  MediaRecorder with mono requested as a preference. It sends ordered WebM/Opus
  pieces every 10 seconds by default.
  These transport frames are successive bytes of one recording; backend decode
  spans and transcript chunks have separate boundaries.
- Backend: `src/api.ts` uses credentialed `/api/v1/projects`, Section, frame,
  completion, and interruption endpoints. Each recorder run creates one Section
  and Audio Asset; transcription and polling are outside #84's authenticated
  delivery slice.
- Browsing: `ProjectBrowser.vue` shows the authenticated Project list in
  backend order, loads one complete Project detail response, displays ordered
  Sections and semantic Audio Asset state, and exposes the backend's native
  download response for sealed assets. The visible Record/Projects navigation
  preserves the page-lifetime recording module while switching surfaces.

## Run browser E2E tests

The Python/Playwright browser suite, fixtures, mock backend, and runner setup
now live in [`running_drafts_product/e2e`](../../running_drafts_product/e2e/README.md).
Product issues #4 and #7 own migrating that suite from the removed seed
`/api/recordings` contract to authenticated Project delivery. Do not add a
second authoritative Playwright suite here.

Start the application here with `npm install` and `npm run dev`.
`npm run check` runs `vue-tsc`, ESLint (`eslint-plugin-vue` recommended rules
and `typescript-eslint`), and a Prettier check; `npm run format` rewrites files
with Prettier. `npm test` runs the Vitest suite (Vue Test Utils, jsdom, and
`vitest-axe`): module and gateway unit tests, component tests, and the
application composition tests. `npm run build` checks and builds the page.

## Run the UI workbench

Run `npm run storybook` to open the local review workbench on port 6006. For a
remote or agent environment, run `npm run storybook -- --no-open` and forward
the port over SSH. The short [Storybook operator guide](docs/storybook.md)
contains the commands and browser instructions.

The development page is split into two surfaces. Backend liveness and
database-backed readiness checks are on the left; the existing browser seed is
kept on the right. Health checks use `http://localhost:18081` by default. A
deployed environment serves `/config.js` before the application loads to set
its public backend URL through `window.__BACKEND_BASE_URL__`. Health and
application API paths are derived from that one URL.

## Browser authentication

The editor bootstraps the current Account from its HttpOnly Application Session
cookie. Run the backend on the configured URL, configure its exact editor origin
and OIDC callback, then start this application with `npm run dev`. Sign-in
providers are a source-owned catalog in `src/session/providers.ts`; Google is
the only entry, and the editor never asks the backend which providers exist.
The current-session response names the provider by its stable key, and the
editor resolves the visible name from the catalog. An unknown key is shown as
received. Adding a provider needs a coordinated backend and editor release.
Sign-in navigates the full page to the backend and returns to the current
editor path and query. The signed-in summary shows the backend-reported session
expiry in local date and time.

`ProfileSummary.vue` in `src/session/` shows the display name, a verified email,
the full Account ID, the provider, the session expiry, and a `Log out` button.
`ProfileWidget.vue` wraps that summary with the corresponding signed-out, loading,
and problem states, including catalog-driven sign-in actions. Both read the
session module; the widget has no production route yet.

All API requests use credentialed CORS. `public/config.js` contains only the
public backend URL; provider secrets and tokens never belong in frontend
configuration or browser storage. Logout ends the local Running Drafts session
and immediately hides Account-derived data. If the backend cannot confirm
logout, the chooser offers a retry.

Before Start, the editor requires at least six hours remaining on the
backend-reported session. A shorter still-valid session is logged out first;
provider actions remain unavailable until logout is confirmed. Authentication
expiry freezes new delivery attempts
without deleting in-memory unsecured audio. Delivery resumes only when session
bootstrap returns the recording's original Account; a different Account sees a
mismatch explanation and cannot receive those bytes. Reload persistence is not
implemented, so the existing leave-page warning still applies.

For a deployed provider smoke test, use a fresh browser profile, choose each
enabled provider, verify callback return to the original local path, record and
secure a short sample, log out, then sign in independently in another fresh
profile and verify the same private Account. CI and ordinary tests use provider
doubles rather than live credentials.

## Deployment

The production image is a static Vite build served by a non-root nginx process.
It accepts only the browser-visible `BACKEND_BASE_URL` at runtime and exposes a
frontend liveness check at `/health/live`. Build a tagged image with
`IMAGE_TAG=0.1.0 make image`.

Host scripts, target environment examples, Compose project names, ports,
rollout, status, logs, and rollback are documented in
[`deployment/README.md`](deployment/README.md). The installed host scripts live
under `/rnotes/RUNNING_DRAFTS_FRONTEND`; neither source nor mutable application
data is copied there.

## Application structure

The UI is a Vue 3 application written with the Composition API. `src/` is
organised by feature, each owning its module, gateway, components, and tests:

- `session/`: Application Session, the provider catalog, sign-in panel, and
  `ProfileSummary.vue`.
- `recording/`: the page-lifetime recording module, one recorder per Project and
  Account, capture, delivery queues, cues, and the recording panel.
- `projects/`: the Project list and detail browser with original-audio downloads.
- `health/`: the development health checks.
- `app/`: the page layout, private workspace, and the composition root.
- `shared/`: configuration, HTTP transport, and formatting used by several features.

`src/app/application.ts` is the only place that creates production adapters. It
builds the gateways, passes them as plain arguments to the module factories
(`createSession`, `createRecording`, `createProjects`, `createHealth`), and
provides the finished modules to components with typed `InjectionKey`s. Components
only inject modules (`useSession()`, `useRecording()`, and so on); they never call
HTTP or browser APIs directly. Tests and Storybook replace an adapter or a whole
module without touching components.

Recording state lives in an application-lifetime module outside any component, so
switching views cannot lose undelivered audio. `createRecording` adds the Account
ownership rules, the diagnostic log, and cue playback. It creates one
`createProjectRecorder` per Account (`src/recording/project-recorder.ts`), the
coordinator that owns Project and Section creation, the start sequence, and the
Section list, and derives its snapshot with `computed()`. Each Section is two modules
with no dependency on each other:

- `section-capture.ts` is the transient part: the microphone, the progress and
  duration timers, and the capture lifecycle. It is the only place the microphone
  is released.
- `section-delivery.ts` is the durable part: the frame queue, one recovery mode
  (automatic, waiting, requested, or blocked), the suspension flag, and sealing the
  Audio Asset. It outlives capture and survives sign-out.

The coordinator maps events from capture into delivery and asks capture to stop when
delivery cannot continue safely (backlog, oversize frame, conflict).

`MediaRecorderFrameSource` and `FrameDelivery` retain their separate capture and
submission responsibilities. `openMicrophoneCapture` owns strict WebM/Opus
capability checking, preferred-mono microphone acquisition, usable-track
verification, recorder finalization, and resource release. Channel count and
sample rate are not capture acceptance gates.

Styles: `src/style.css` holds design tokens and base styles; each component keeps
its own layout in `<style scoped>`. `src/style.test.ts` checks the colour tokens
against WCAG 2.2 AA contrast ratios.

## Recording and recovery

The page keeps one microphone stream and recorder open per Section. Stop waits
for the final partial transport frame, then the Section completes only after all
assigned frames are acknowledged. The ten-minute recording limit, transport
interval, and 2 MiB frame guard are configurable through `window.__SEED_CONFIG__`.
A prominent green **Speak now** indicator and duration progress appear during
microphone capture. They stop indicating capture as soon as capture stops,
independently of pending uploads and backend work.

Audio stays in browser memory until acknowledged. The sender retries the oldest
pending transport frame after 5, 10, and 17 seconds from each preceding attempt's
start. Requests time out independently after 60 seconds, so retries can overlap.
The backend must accept duplicates idempotently.

The three-frame backlog limit applies independently to each Section and includes
the current frame: two completed pending frames plus one being captured. If
delivery stays blocked, capture stops at the current boundary. New automatic
attempts for that Section stop, while existing attempts may finish. Per-Section
**Upload pending audio** actions retry retained bytes without restarting capture.
A new recorder run can create and upload another Section concurrently. Leaving
with captured or pending audio warns; reload recovery is not implemented.
Any frame-upload HTTP `409` stops capture and permanently freezes delivery for
that Section without sealing it. Exact pending audio and returned Problem
Details remain visible for diagnosis, while another Section may start normally.

Start and capture stop play short rising/falling cues. Each newly acknowledged
frame for the actively capturing Section plays a quiet rising health cue. At the
time limit, the falling stop cue plays immediately instead of waiting for
delivery or sealing.
Delayed upload plays an attention tone followed by Morse U (`..-`) once per delay
episode. A Capture interruption immediately plays the attention tone followed
by Morse X (`-..-`) and shows why capture stopped.

Before real-microphone use, check cue recognition and leakage on the intended
speakers/headset: speak while health cues play, force an upload failure, listen
for distinct delay and stopped-capture cues, and inspect the recording for cue
contamination. Fake-microphone tests cannot verify acoustic leakage.

## Diagnostic log

The dark panel is a fixed 100×10 character log. Entries are written as one
stream and start with `▸` and the seconds since Start. When the panel is full,
writing continues at the top-left and clears a gap ahead of the highlighted
write position. Codes include `s2 f3 sent` and `s2 f3 ack` for frame 3 of
Section 2, `complete sent`, `interrupt sent`, and `ERR` for failures.
