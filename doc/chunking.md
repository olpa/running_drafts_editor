# Recognition chunking: long-term technical context

**Status:** durable project context  
**Date:** 2026-08-05  
**Audience:** software developers who do not need prior audio or speech-recognition knowledge

## 1. Why this document exists

The project turns existing audio recordings into text. Long recordings cannot
always be sent to a speech recognizer as one unit. We therefore need to divide
audio into smaller ranges.

There are two different structures in the product:

- **Recognition chunks** are audio ranges sent to the speech recognizer.
- **Paragraphs** are readable and editable blocks of visible text.

These structures are related, but they are not the same. This document explains
the long-term design so future work does not join them by accident.

## 2. Decisions already made

The board has selected:

- **Rust** for the implementation language.
- **Silero VAD** for detecting speech activity.
- The official Silero Rust example as implementation guidance:
  <https://github.com/snakers4/silero-vad/tree/master/examples/rust-example>
- **Whisper** as the speech recognizer.
- The `backtrack` branch of this Rust binding:
  <https://github.com/olpa/whisper-rs/tree/backtrack>
- Whisper-compatible input: mono, 16 kHz, normalized `f32` PCM samples.

Every external repository, native library, and model file must be pinned to an
exact version or commit in production. A branch name is not a reproducible pin.

## 3. Short glossary

### Audio source

The original recording and its stable content identity. The identity should be
a cryptographic hash of the source bytes or another immutable content key.

### Canonical audio

Decoded audio used by the chunking system: one channel, 16,000 samples per
second, normalized `f32` PCM. Canonical audio gives Silero and Whisper the same
sample clock.

### Sample

One audio value. At 16 kHz, 16,000 samples represent one second. Samples are the
canonical time unit. Seconds are only a display format.

### Voice activity detection (VAD)

A model that estimates whether an audio frame contains human speech. VAD does
not recognize words. Silero VAD returns speech probabilities or speech ranges.

### Recognition chunk

A range of source samples prepared for one recognition operation. It has a
stable identity, a core range, an optional submitted range, and boundary
evidence.

### Core range

The samples owned by a chunk for coverage purposes. Core ranges must form an
exact partition of the intended audio: no missing samples and no duplicated
samples.

### Submitted range

The samples actually sent to Whisper. It can be wider than the core range when
padding or overlap is enabled. Submitted ranges may overlap, but this must be
explicit.

### Recognition plan

A revisioned ordered list of recognition chunks for one source, recognizer
contract, detector, and configuration.

### Paragraph

A readable, editable range of authoritative text. Paragraphs may use
punctuation, meaning, pauses, imported formatting, or user edits.

### Boundary evidence

Information explaining a selected cut, such as a low Silero probability, a
detected pause, or a hard-limit fallback.

## 4. The main design rule

Recognition chunks and paragraphs must have independent identities.

A paragraph may contain words from several chunks. One chunk may contribute
words to several paragraphs. The relationship is many-to-many and is recorded
through time-aligned words or other provenance links.

Paragraph editing must not silently change recognition chunks. If the product
later supports re-chunking after an edit, it must create a new recognition-plan
revision and keep the old revision available for explanation and comparison.

## 5. Why chunk boundaries matter

A hard cut can split a word or phoneme. The recognizer then receives incomplete
acoustic context and may omit, duplicate, or replace text near the cut.

A pause is usually a better cut position, but recordings are not simple:

- continuous speech may contain no pause near the maximum length;
- a breath or hesitation may look like a short pause;
- quiet speech can look like silence;
- background noise can hide a real pause;
- music may be mistaken for speech or non-speech;
- duration metadata may be missing or inaccurate.

The system therefore treats Silero output as useful evidence, not as truth.

## 6. Long-term architecture

The design has separate stages:

1. **Decode and normalize:** convert media into canonical audio.
2. **Detect:** run Silero and produce speech evidence.
3. **Plan:** choose legal chunk boundaries using evidence and fallback rules.
4. **Recognize:** send submitted ranges to Whisper.
5. **Reconcile:** resolve duplicated or conflicting text if overlap exists.
6. **Align:** connect recognized words or spans to source time.
7. **Form paragraphs:** create readable text units.

Stages may share types, but they must not be merged into one hidden operation.
Each stage should preserve enough input identity, configuration, and output to
explain its result.

## 7. Recognizer contract

The selected Rust binding passes a slice of mono 16 kHz `f32` samples to
Whisper. Whisper uses a 30-second acoustic window. For the first implementation,
one submitted chunk must contain no more than:

```text
30 seconds × 16,000 samples/second = 480,000 samples
```

This limit includes any left or right padding. If the selected `backtrack`
branch later exposes a different safe contract, that change must be documented,
tested, and revisioned rather than silently changing the planner.

Whisper result timestamps use a coarser unit than the source sample clock. They
must not replace the sample-accurate plan boundaries.

## 8. Detector contract

Silero runs over canonical audio and produces ordered frame evidence. The
implementation may store every probability for experiments, but a production
plan may store a compact summary plus the hash/location of the complete evidence.

Detector output must include:

- detector and model version;
- model-file hash;
- sample rate and frame size;
- configuration values;
- deterministic runtime information where relevant;
- speech probability or speech/non-speech ranges;
- an explicit error if detection fails.

Detector failure is not a reason to produce an illegal or incomplete plan. The
planner must use fixed legal windows as a fallback.

## 9. Planner behavior

The planner starts at the first uncovered core sample. It calculates the last
legal end sample for the next submitted range. It then searches a configured
area before that limit for a good low-speech position.

Candidate selection should be deterministic. Given the same source, model,
versions, and configuration, it must select the same samples.

If no candidate is good enough, the planner cuts at the last legal sample and
records `hard_limit_no_candidate`. If Silero is unavailable or fails, it uses
the same legal fixed-window behavior and records the detector error.

The final short range must never be dropped. It may be joined to the previous
range only when the joined submitted range remains legal.

## 10. Coverage and overlap

Core ranges should normally satisfy all of these rules:

```text
first.start = 0
last.end = decoded_sample_count
chunk[i].end = chunk[i + 1].start
chunk.start < chunk.end
```

Initial submitted ranges equal core ranges. Initial overlap is zero.

Future overlap can add context on both sides of a seam. However, it also causes
Whisper to recognize some speech twice. Text reconciliation is a separate
problem and must be implemented and tested before non-zero overlap becomes a
default.

## 11. Paragraph relationship

Paragraph evidence is optional and arrives after recognition or alignment. A
future planner may give a small preference to an aligned paragraph boundary
inside its already legal search area.

Paragraph evidence must never:

- remove audio from coverage;
- move a cut outside the legal area;
- create or determine a chunk identity;
- force every paragraph to become one chunk;
- force every chunk seam to become a paragraph;
- make recognition depend on an existing transcript.

## 12. Reproducibility and identity

A plan identity should depend on canonical content such as:

- source content hash and decoded sample facts;
- Whisper adapter and pinned implementation commit;
- Whisper model identity;
- Silero code/model identity;
- planner version and full configuration;
- selected core and submitted ranges.

Creation time and local file path should not make otherwise equal plans unequal.
Use canonical serialization before hashing.

Chunk identity should depend on the plan identity, ordinal, and sample ranges.
It must not depend on paragraph number or recognized text.

## 13. Configuration policy

The following values are experimental until measured on the project corpus:

- Silero speech threshold;
- hysteresis thresholds;
- minimum speech and non-speech duration;
- search-window length;
- minimum chunk length;
- boundary scoring and tie-breaking;
- acoustic padding;
- overlap length;
- source-specific profiles.

Code may require these values or provide a named experimental profile. It must
not present untested numbers as settled product policy. All values must appear
in plan output.

## 14. Expected difficult cases

| Case | Expected result |
|---|---|
| Clear pause before limit | Cut at a deterministic low-speech point |
| Continuous speech | Cut at hard limit and record fallback |
| Short hesitation | Avoid unnecessary tiny chunks |
| Long silence | Select one deterministic point; avoid empty requests |
| Background noise | Use Silero evidence; fall back if no good valley exists |
| Music | Preserve coverage even when classification is imperfect |
| Very short source | Emit one exact legal chunk |
| Exactly 480,000 samples | Emit one exact legal chunk |
| One sample over limit | Emit at least two complete legal chunks |
| Short final tail | Preserve it |
| Detector error | Emit fixed legal plan with recorded error |
| Edited paragraph | Do not change existing plan |

## 15. Testing strategy

The project needs several test layers:

- Unit tests for sample arithmetic, scoring, and fallback.
- Property tests for complete coverage, legality, ordering, and termination.
- Golden tests for stable serialized plans.
- Silero integration tests with a pinned model and small reviewed fixtures.
- Whisper seam experiments comparing boundary-local omissions, duplicates, and
  substitutions.
- Portability tests for supported desktop/mobile targets.
- Failure tests for missing models, invalid audio, runtime errors, and corrupt
  evidence.

The test corpus should include clean pauses, continuous speech, breaths,
hesitation, short/long silence, noise, music, short/near-limit/long recordings,
and correct, wrong, or absent paragraph breaks. Fixtures must be generated
locally or clearly redistributable.

## 16. Dependencies and pinning

The Silero Rust example currently demonstrates ONNX Runtime through the `ort`
crate and WAV input through `hound`. The Whisper binding wraps `whisper.cpp`.
These examples are guidance, not stable dependency specifications.

Before release, record and pin:

- Rust toolchain and target triples;
- `ort` and ONNX Runtime versions;
- Silero ONNX model SHA-256;
- `olpa/whisper-rs` exact commit on `backtrack`;
- its `whisper.cpp` submodule commit;
- Whisper model name, format, and SHA-256;
- decoder/resampler implementation and version.

Check licenses for code, native binaries, codecs, and model files separately.

## 17. What this subsystem does not own

The chunk planner does not own:

- speech recognition execution or retry policy;
- duplicate-text reconciliation;
- authoritative transcript construction;
- paragraph editing or formatting;
- speaker diarization;
- waveform or full-screen user interfaces;
- automatic re-planning after text edits.

These features may consume recognition plans, but must not weaken the plan's
coverage, legality, identity, or reproducibility rules.

## 18. Future decision record

When experiments select thresholds, padding, or overlap, create a dated decision
record. It should name the corpus, exact dependency/model pins, compared
strategies, seam and global recognition results, runtime cost, rejected choices,
and remaining risks. Do not hide these decisions inside default constants.
