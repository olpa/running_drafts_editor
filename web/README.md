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
Vite/TypeScript page and a Python/Playwright happy-path test. Two blocks are
mocked and will be replaced one at a time:

- Capture: `MockFrameSource` in `src/capture.ts` produces placeholder
  transport frames. #77 replaces it with MediaRecorder.
- Backend: `e2e/mock_backend.py` intercepts the page's `/api` requests. #78
  deletes it and runs the same test against the real backend.

The assumed backend contract is in `src/api.ts`. The test's expected values
are in `e2e/expected/happy-path.json`; replacing a block updates that file, not
the test code.

## Run the happy path

Requirements: Node.js 24 and Python 3.12.

```sh
npm install
python3 -m venv e2e/.venv
e2e/.venv/bin/pip install -r e2e/requirements.txt
e2e/.venv/bin/python -m playwright install chromium
```

Start the dev server in one terminal:

```sh
npm run dev
```

Run the test in another:

```sh
e2e/.venv/bin/pytest e2e
```

The test uses `http://localhost:5173` by default; pass `--base-url` to use
another server. Chromium's fake microphone plays `e2e/fixtures/speech-en.riff`,
a 16 kHz mono PCM WAV recording, in a loop.

`npm run check` type-checks the application.

## Diagnostic log

The dark panel is a fixed 100×10 character log. Entries are written as one
stream and start with `▸` and the seconds since Start. When the panel is full,
writing continues at the top-left and clears a gap ahead of the highlighted
write position. Codes: `f3 sent` and `f3 ack` for transport frame 3, `+c2` for
the second chunk, `srv <status>` for a backend status change, `ERR` for errors.
