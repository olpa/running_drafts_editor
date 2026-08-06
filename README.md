# Running Drafts Editor

Running Drafts Editor (`rde`) is an experimental line-oriented tool for turning
existing recordings and imperfect recognition into usable text. The first
implemented module creates legal, revisioned recognition chunk plans. It does
not run Whisper or edit transcript text yet.

## Build and test

The Rust toolchain is pinned in `rust-toolchain.toml`; crate versions and
checksums are pinned in `Cargo.lock`.

```console
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Tests use synthetic PCM and do not download a Silero model. The integration
boundary verifies missing/corrupt model failures without network access.

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

## Reproducibility and licenses

The adapter follows the Silero VAD V5 ONNX contract from upstream commit
`76e3dc408eb2a5c655c34e230d2d5459b4439daa`: 512 new samples at 16 kHz,
64 carried context samples, and recurrent state shaped `[2, 1, 128]`. Callers
must also provide an exact model-file SHA-256.

Silero VAD code and published models are MIT-licensed. `ort` is MIT/Apache-2.0;
prebuilt ONNX Runtime artifacts have their own MIT license. This repository
does not redistribute model files. Review licenses again before packaging.
