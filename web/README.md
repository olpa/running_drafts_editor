# Browser seed experiment

This directory is the home for the Running Drafts browser seed. Its current #84
slice tests authenticated browser recording and durable audio delivery through
the backend; the broader seed also covers progressive transcription work.

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

- Capture: `RecorderFrameSource` in `src/capture.ts` uses one continuous mono
  MediaRecorder. It sends ordered WebM/Opus pieces every 10 seconds by default.
  These transport frames are successive bytes of one recording; backend decode
  spans and transcript chunks have separate boundaries.
- Backend: `src/api.ts` uses credentialed `/api/v1/projects`, Section, frame,
  completion, and interruption endpoints. Each recorder run creates one Section
  and Audio Asset; transcription and polling are outside #84's authenticated
  delivery slice.

## Run browser E2E tests

The Python/Playwright browser suite, fixtures, mock backend, and runner setup
now live in [`running_drafts_product/e2e`](../../running_drafts_product/e2e/README.md).
Product issues #4 and #7 own migrating that suite from the removed seed
`/api/recordings` contract to authenticated Project delivery. Do not add a
second authoritative Playwright suite here.

Start the application here with `npm install` and `npm run dev`.
`npm run check` type-checks the application and unit tests. `npm test` runs
health-client, capture, delivery, and recording-controller unit tests with Node.js 24.
`npm run build` builds the page.

The development page is split into two surfaces. Backend liveness and
database-backed readiness checks are on the left; the existing browser seed is
kept on the right. Health checks use `http://localhost:18081` by default. A
deployed environment serves `/config.js` before the application loads to set
its public backend URL through `window.__BACKEND_BASE_URL__`. Health and
application API paths are derived from that one URL.

## Browser authentication

The editor discovers enabled sign-in providers from the configured backend and
bootstraps the current Account from its HttpOnly Application Session cookie.
Run the backend on the configured URL, configure its exact editor origin and
OIDC callback, then start this application with `npm run dev`. A fresh browser
shows only providers enabled by the backend. Sign-in navigates the full page to
the backend and returns to the current editor path and query. The signed-in
summary shows the backend-reported session expiry in local date and time.

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

## Recording modules

`<recording-panel>` in `src/components/recording-panel.ts` owns the recording
controls and visual/audio feedback. It can run independently of `<seed-app>` and
publishes `recording-event` events for snapshots, cues, and diagnostic log
entries. The panel owns its page-lifetime controller. A temporary disconnect
stops active capture and releases microphone resources while preserving its
Project, Sections, Account ownership, and pending bytes for reconnection;
`finishModule()` performs permanent disposal when the page module is finished.

`RecordingController` in `src/recording.ts` owns one Project, Section creation,
all page-lifetime delivery queues, start/stop, duration and backlog limits,
recovery, sealing, and semantic cue decisions. Its interface is `start()`,
`stop()`, `uploadPendingAudio(sectionId)`, `retryClose(sectionId)`,
Account-checked authentication suspension/resumption, `dispose()`, `snapshot`,
and `subscribe()`. It accepts backend and capture adapters plus clocks and
identity generation.

`RecorderFrameSource` and `FrameDelivery` retain their separate capture and
submission responsibilities. `openRecordingCapture` owns strict WebM/Opus
capability checking, mono microphone acquisition, verification, and release.
`<seed-app>` composes the panel with authentication and the diagnostic log.

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
