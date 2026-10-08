# Browser seed experiment

This directory is the home for the Running Drafts browser seed. It tests
browser recording and progressive transcription through the backend and private
inference service.

[Product issue #1](https://github.com/olpa/running_drafts_product/issues/1)
coordinates the seed across repositories. Backend and inference implementation
belong to `olpa/running_drafts_backend`.

Shared [product intent](../docs/product.md) and [domain language](../GLOSSARY.md)
live at the repository root. The independent [Rust CLI](../cli/README.md) is
another technical experiment for the same product.

## Walking skeleton

[Issue #76](https://github.com/olpa/running_drafts_editor/issues/76) provides a
Vite/TypeScript page and a Python/Playwright happy-path test. Capture is real;
backend behavior remains mocked:

- Capture: `RecorderFrameSource` in `src/capture.ts` uses one continuous
  MediaRecorder. It sends ordered WebM/Opus pieces every 10 seconds by default.
  These transport frames are successive bytes of one recording; backend decode
  spans and transcript chunks have separate boundaries.
- Backend: `e2e/mock_backend.py` intercepts the page's `/api` requests. #78
  deletes it and runs the same test against the real backend.

The assumed backend contract is in `src/api.ts`. The test's expected values
are in `e2e/expected/happy-path.json`. The happy path now also checks real
transport bytes, one recorder, microphone release, and acknowledgement cues.

## Run the happy path

Requirements: Node.js 24 and Python 3.12.

```sh
npm install
python3 -m venv e2e/.venv
e2e/.venv/bin/pip install -r e2e/requirements.txt
e2e/.venv/bin/python -m playwright install chromium firefox
```

Start the dev server in one terminal:

```sh
npm run dev
```

Run the test in another:

```sh
e2e/.venv/bin/pytest e2e
```

To watch the journey in a visible Chromium window, slowed down so that a
person can follow it (about 25 seconds, with audible start and stop cues):

```sh
e2e/.venv/bin/pytest e2e/test_happy_path.py --demo
```

`--demo` uses longer transport frames and polling, pauses at key moments, and
implies `--headed --slowmo 600`. Pass `--slowmo` yourself to change the pace.

The test uses `http://localhost:5173` by default; pass `--base-url` to use
another server. Chromium's fake microphone plays `e2e/fixtures/speech-en.riff`,
a 16 kHz mono PCM WAV recording, in a loop.

`npm run check` type-checks the application and unit tests. `npm test` runs
capture, delivery, and recording-controller unit tests with Node.js 24. `npm run build` builds the page.

The audio continuity test requires `ffmpeg` on PATH. It records a known rising
frequency tone through Chromium's fake microphone, decodes the combined upload
bytes, and checks for jumps in source time across transport boundaries. This is
test tooling, not browser or backend implementation.

Run Firefox recording and recovery checks with:

```sh
e2e/.venv/bin/pytest e2e --browser firefox
```

Firefox uses its built-in fake microphone tone. The known-waveform file test is
Chromium-only and is skipped in Firefox.

## Recording modules

`<recording-panel>` in `src/components/recording-panel.ts` owns the recording
controls and visual/audio feedback. It can run independently of `<seed-app>` and
publishes `recording-event` events for snapshots, cues, chunks, diagnostic log
entries, and a new-recording reset. The panel owns its controller and releases
microphone resources when disconnected.

`RecordingController` in `src/recording.ts` owns start, stop, retry, duration and
backlog limits, completion, and semantic cue decisions. Its interface is
`start(language)`, `stop()`, `retry()`, `dispose()`, `snapshot`, and `subscribe()`.
It accepts a backend, a capture factory, and a clock. Controller unit tests use
fake capture/backend adapters and controlled timers, without a browser or DOM.

`RecorderFrameSource` and `FrameDelivery` retain their separate capture and
submission responsibilities. `openRecordingCapture` owns browser microphone
acquisition and release. `<seed-app>` only composes the panel, transcript, and
circular diagnostic log. Browser tests also exercise the panel on its own and
its disconnect/reconnect lifecycle.

## Recording and recovery

The page keeps one microphone stream and recorder open. Stop waits for the final
partial transport frame before sending finish. The ten-minute recording limit
and transport interval are configurable through `window.__SEED_CONFIG__`.

Audio stays in browser memory until acknowledged. The sender retries the oldest
pending transport frame after 5, 10, and 17 seconds from each preceding attempt's
start. Requests time out independently after 60 seconds, so retries can overlap.
The backend must accept duplicates idempotently.

The three-frame backlog limit includes the current frame: two completed pending
frames plus one being captured. If delivery stays blocked, recording stops at
the current boundary. New automatic attempts stop, while existing attempts may
finish. **Upload pending audio** retries retained audio without restarting
capture. Starting another recording is blocked until the current one completes.
Leaving with captured or pending audio warns; reload recovery is not implemented.
The warning and pending duration include final audio still being flushed by the
recorder after capture stops.

Start and completion play short rising/falling cues. Each newly acknowledged
frame plays a quiet rising health cue and advances the visible secured duration.
At the time limit, the falling stop cue plays immediately instead of waiting for
transcription completion, and does not repeat when processing completes.
Delayed upload plays an attention tone followed by Morse U (`..-`) once per delay
episode. Backlog or microphone interruption immediately plays the attention tone
followed by Morse X (`-..-`) and shows why recording stopped.

The backend mock exercises durable-acceptance acknowledgements. It does not
provide production storage. Real durable acceptance belongs to backend issue #4
and is verified when editor #78 integrates the real service.

Before real-microphone use, check cue recognition and leakage on the intended
speakers/headset: speak while health cues play, force an upload failure, listen
for distinct delay and stopped-capture cues, and inspect the recording for cue
contamination. Fake-microphone tests cannot verify acoustic leakage.

## Diagnostic log

The dark panel is a fixed 100×10 character log. Entries are written as one
stream and start with `▸` and the seconds since Start. When the panel is full,
writing continues at the top-left and clears a gap ahead of the highlighted
write position. Codes: `f3 sent` and `f3 ack` for transport frame 3, `+c2` for
the second chunk, `srv <status>` for a backend status change, `ERR` for errors.
