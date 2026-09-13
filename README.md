# Running Drafts Editor

Running Drafts Editor turns an existing recording and an imperfect
transcription into usable text. The current technical-feasibility version is a
line-oriented CLI: text is authoritative, while transcription data and audio
support replay and correction.

## Build

The project requires the Rust toolchain pinned by `rust-toolchain.toml`, a C/C++
compiler, CMake, and libclang for bindgen. Native compilation builds the pinned
`whisper.cpp` source statically and may take about a minute on the first run.

```sh
cargo build
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Tests use synthetic PCM and do not download a Whisper model.

The `hfvc_lib` and `whisper-rs` dependencies are pinned to Git revisions.
`whisper-rs` builds its pinned `whisper.cpp` submodule statically.

## Use

Transcribe a PCM WAV recording and save an editable file:

```sh
cargo run -- transcribe recording.wav \
  --model ggml-tiny.bin \
  --output draft.rde.json
```

Transcription requires a Whisper ggml model supplied by the user; models are
not included in the repository. Input may use 8-, 16-, 24-, or 32-bit integer
PCM or 32-bit floating-point PCM. The program averages channels and resamples
to mono 16 kHz; compressed WAV and non-WAV formats are unsupported.

Open the saved file in the editor:

```sh
cargo run -- edit draft.rde.json
```

Use `cargo run -- --help` and the session `help` command for the current command
reference. Model binaries, recordings, and generated project files remain
external to the repository.

## Project information

- [`docs/product.md`](docs/product.md) records product intent and interaction
  rationale.
- [`docs/transcription-chunking.md`](docs/transcription-chunking.md) explains
  provisional overlap, final chunk formation, and the earlier VAD experiment.
- [`CONTEXT.md`](CONTEXT.md) defines the domain language.
- [`docs/adr/`](docs/adr/) records durable architectural decisions and their
  reasons.
- [GitHub Issues](https://github.com/olpa/running_drafts_editor/issues) contain
  plans, specifications, and acceptance criteria.
- [`AGENTS.md`](AGENTS.md) tells coding agents how to find project context.

This project is licensed under GPL-3.0-or-later. Its dependencies retain their
own licenses.
