# Running Drafts Editor

Running Drafts Editor (`rde`) is an experimental line-oriented tool for turning
existing recordings and imperfect recognition into usable text. It can
transcribe a recording to a saved JSON document and open that document in a
line-oriented editor. The `open-audio` command exposes fresh recognition details.

## Build and test

The Rust toolchain is pinned in `rust-toolchain.toml`; crate versions and
checksums are pinned in `Cargo.lock`.

```console
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Tests use synthetic PCM and do not download speech models. Exact source
snapshots of the selected `olpa/whisper-rs` fork and its `whisper.cpp` backend
live under `vendor/whisper-rs`; their provenance is recorded there. Cargo and
CMake compile whisper.cpp into the executable, so no HandsfreeVC checkout,
project-specific environment variable, or runtime shared-library path is
needed. A first native build can take about a minute and requires a C/C++
compiler, CMake, and libclang for bindgen.

## Transcribe and edit

First, build the executable:

```console
cargo build
```

Transcription requires two external files:

- `AUDIO` must point to a PCM WAV recording. The tool accepts 8-, 16-, 24-,
  and 32-bit integer PCM or 32-bit float samples, averages multiple channels,
  and resamples the audio to mono 16 kHz internally. Compressed WAV files and
  non-WAV formats are not supported.
- `--model` must point to a Whisper ggml model. Models are not included in this
  repository.

Create a versioned JSON document without opening an interactive prompt:

```console
./target/debug/rde transcribe recording.wav \
  --model ggml-tiny.bin \
  --output draft.rde.json \
  --language de
```

Then open it in the line-oriented editor:

```console
./target/debug/rde edit draft.rde.json
```

For all recognition and chunking options, run `rde transcribe --help`.
Recognition may take some time. On success, `transcribe` saves the document
atomically and exits. It does not start playback or read an interactive prompt.

## Open audio after transcription

Use the same recognition pipeline and open its result interactively:

```console
./target/debug/rde open-audio \
  --input recording.wav \
  --model ggml-tiny.bin \
  --language de
```

When recognition finishes, the tool shows the recognized document and opens an
`rde>` prompt. Type `help` there to see the current session commands.

The listing contains replay chunks built from whole accepted Whisper segments.
Pause length and normal text-token count choose their boundaries. Marker play
replays an exact chunk, and marker info shows its time range, duration, token
count, boundary reason, and text. All
overlapping window hypotheses remain immutable evidence; midpoint ownership is
only the initial deterministic deduplication rule.

## Reproducibility and licenses

The command hashes the caller-supplied Whisper model. This repository does not
redistribute recognition models. The vendored whisper-rs and whisper.cpp
license files remain with their source snapshots. Review licenses again before
packaging.
