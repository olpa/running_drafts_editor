# Running Drafts Editor

Running Drafts Editor (`rde`) is an experimental line-oriented tool for turning
existing recordings and imperfect recognition into usable text. It can create
experimental Silero recognition plans and can run bounded, overlapping Whisper
recognition for decoded-text audition. Transcript editing is not implemented
yet.

## Build and test

The Rust toolchain is pinned in `rust-toolchain.toml`; crate versions and
checksums are pinned in `Cargo.lock`.

```console
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Tests use synthetic PCM and do not download speech models. The pinned
`olpa/whisper-rs` fork links a prebuilt whisper.cpp 1.8.2 distribution and
currently requires `HANDSFREEVC_DEV_HOME` at build time. On Linux its shared
library directory must also be available at runtime, for example:

```console
HANDSFREEVC_DEV_HOME=/path/to/hfvc_dev \
LD_LIBRARY_PATH=/path/to/hfvc_dev/whisper.cpp/linux-x86_64 \
cargo test --all-targets
```

## Experimental chunk-plan command

The input must already be canonical mono, 16 kHz, 32-bit float WAV. The caller
supplies the model; its SHA-256 is derived and checked before loading it.

```console
rde chunk plan \
  --input recording-f32.wav \
  --model silero_vad.onnx
```

The CLI defaults to Silero v5, an 80,000-sample boundary search, a
160,000-sample minimum chunk, a 0.5 speech threshold, 1,600 consecutive
low-speech samples, and a 480,000-sample recognizer limit. Every value and the
derived model hash can still be overridden for reproducible experiments. The
model must exist and be readable before planning starts. JSONL events go to
stdout and diagnostics to stderr. The stream starts with `plan_started`, emits one
`detector_evidence` record per frame, and ends with `plan_complete`. Detector
failures after model preflight
produce a recorded fixed plan; the command never downloads a model. Audio
hashing uses a bounded buffer, while validation and Silero inference consume
512-sample frames without retaining the full decoded recording.

## Experimental decoded-audition command

The audition command now accepts a Whisper ggml model. It uses explicit
processing windows no longer than 30 seconds, overlap on both sides of the
target core, a bounded tail of accepted text as prompt context, and Whisper timestamps
to select advancing seams.

```console
rde chunk audition \
  --input recording-f32.wav \
  --model ggml-tiny.bin \
  --language de
```

The listing contains accepted decoded text segments and timestamp-derived
sample ranges. `3play` or `3p` replays the exact listed range. All overlapping
window hypotheses remain immutable evidence; midpoint ownership is only the
initial deterministic deduplication rule.

## Reproducibility and licenses

The adapter follows the Silero VAD V5 ONNX contract from upstream commit
`76e3dc408eb2a5c655c34e230d2d5459b4439daa`: 512 new samples at 16 kHz,
64 carried context samples, and recurrent state shaped `[2, 1, 128]`. Callers
must also provide an exact model-file SHA-256.

Silero VAD code and published models are MIT-licensed. `ort` is MIT/Apache-2.0;
prebuilt ONNX Runtime artifacts have their own MIT license. This repository
does not redistribute model files. Review licenses again before packaging.
