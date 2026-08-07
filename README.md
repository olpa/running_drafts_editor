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

The audition command now accepts a Whisper ggml model. It uses explicit
processing windows no longer than 30 seconds, overlap on both sides of the
target core, direct text-token context from the previous accepted segment, and
Whisper timestamps to select advancing boundaries.

```console
rde chunk audition \
  --input recording-f32.wav \
  --model ggml-tiny.bin \
  --language de
```

The listing contains replay chunks built from whole accepted Whisper segments.
Pause length and normal text-token count choose their boundaries. `3play` or
`3p` replays the exact listed range. All overlapping window hypotheses remain
immutable evidence; midpoint ownership is only the initial deterministic
deduplication rule.

## Reproducibility and licenses

The command hashes the caller-supplied Whisper model. This repository does not
redistribute recognition models. The vendored whisper-rs and whisper.cpp
license files remain with their source snapshots. Review licenses again before
packaging.
