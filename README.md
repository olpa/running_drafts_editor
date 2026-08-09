# Running Drafts Editor

Running Drafts Editor (`rde`) is an experimental line-oriented tool for turning
existing recordings and imperfect recognition into usable text. It can run
bounded, overlapping Whisper recognition for decoded-text audition. Transcript
editing is not implemented yet.

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

## Experimental decoded-audition command

First, build the executable:

```console
cargo build
```

The command requires two external files:

- `--input` must point to a PCM WAV recording. The tool accepts 8-, 16-, 24-,
  and 32-bit integer PCM or 32-bit float samples, averages multiple channels,
  and resamples the audio to mono 16 kHz internally.
- `--model` must point to a Whisper ggml model. Models are not included in this
  repository.

Run the `audition` command. Options such as `--input` cannot be passed directly
to `rde`:

```console
./target/debug/rde audition \
  --input recording-f32.wav \
  --model ggml-tiny.bin \
  --language de
```

For all recognition and chunking options, run:

```console
./target/debug/rde audition --help
```

Recognition may take some time. When it finishes, the tool shows the recognized
text with markers such as `⟦1.2⟧` and opens a `chunk>` prompt. Type `help` at
that prompt to see the available session commands:

```text
Session commands:
  Nplay, Np  play chunk N; for example, 3p
  M.Ninfo    show details for marker M.N; for example, 2.3info
  list, l    show the recognized text and chunk markers
  help, h    show this help
  quit, q    exit
```

The listing contains replay chunks built from whole accepted Whisper segments.
Pause length and normal text-token count choose their boundaries. `3play` or
`3p` replays the exact range of the third chunk. `2.3info` shows the time range,
duration, token count, boundary reason, and text for marker `2.3`. All
overlapping window hypotheses remain immutable evidence; midpoint ownership is
only the initial deterministic deduplication rule.

## Reproducibility and licenses

The command hashes the caller-supplied Whisper model. This repository does not
redistribute recognition models. The vendored whisper-rs and whisper.cpp
license files remain with their source snapshots. Review licenses again before
packaging.
