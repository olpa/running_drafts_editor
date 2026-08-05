# Implementation specification: Silero-based recognition chunk planner

**Status:** ready for immediate development  
**Date:** 2026-08-05  
**Language:** Rust  
**Audience:** implementing developer and reviewer

## 1. Goal

Build a Rust library that divides canonical audio into complete, legal,
revisioned recognition chunks. Use Silero VAD to prefer low-speech boundaries.
Use a deterministic fixed boundary when Silero cannot provide a suitable cut.

This task produces recognition plans only. It does not run Whisper, reconcile
text, or create paragraphs.

## 2. Fixed decisions

- Input audio for planning is mono, 16 kHz, normalized `f32` PCM.
- Canonical positions are zero-based integer sample offsets.
- Ranges are half-open: `[start_sample, end_sample)`.
- Whisper's initial maximum submitted range is 480,000 samples (30 seconds).
- Silero VAD is the selected detector.
- Silero inference uses ONNX Runtime from Rust, following the upstream Rust
  example where useful.
- The future recognizer is `olpa/whisper-rs`, branch `backtrack`, pinned to an
  exact commit by integration work.
- Initial padding and overlap are zero.
- Silero never removes samples from core coverage.
- Detector failure activates a fixed-window fallback.

## 3. Scope

Implement:

- domain types for source facts, detector evidence, configuration, chunks,
  boundary reasons, failures, and recognition plans;
- a Silero ONNX detector adapter;
- a pure deterministic planner;
- plan validation;
- stable JSON serialization suitable for tests and later storage;
- unit, property, golden, integration, and failure tests;
- a small internal command-line harness for development if the repository has
  no existing integration entry point.

Do not implement:

- general media decoding or resampling unless already available in the project;
- Whisper execution;
- recognized text or timestamps;
- overlap reconciliation;
- paragraph construction or editing;
- diarization;
- public CLI grammar;
- automatic model downloads.

## 4. Dependency rules

Use the official Silero Rust example as guidance:
<https://github.com/snakers4/silero-vad/tree/master/examples/rust-example>.

It currently demonstrates `ort`, `ndarray`, and `hound`. Adapt its model-state
handling carefully; do not copy example-level panic/error handling into the
library.

Requirements:

- Pin `ort` and other direct dependencies in `Cargo.lock`.
- Load the Silero ONNX model from an explicit path supplied by the caller.
- Never download models during planning or tests.
- Compute or accept the model SHA-256 and include it in evidence.
- Configure ONNX Runtime for deterministic CPU execution in tests where possible.
- Keep ONNX-specific code behind a `SpeechDetector` trait.
- Do not add `whisper-rs` to this planner unless repository structure requires
  a shared type. Recognition is a later task.

## 5. Public Rust model

Names may match existing project conventions, but preserve these meanings.

```rust
pub struct CanonicalAudio<'a> {
    pub samples: &'a [f32],
    pub sample_rate_hz: u32, // must be 16_000
    pub channels: u16,       // must be 1
    pub source_sha256: String,
}

pub struct RecognizerContract {
    pub name: String,
    pub version: String,
    pub max_submitted_samples: u64, // initially 480_000
}

pub struct PlannerConfig {
    pub search_back_samples: u64,
    pub minimum_chunk_samples: u64,
    pub speech_threshold: f32,
    pub minimum_low_speech_samples: u64,
    pub left_padding_samples: u64,  // must be 0 in v1
    pub right_padding_samples: u64, // must be 0 in v1
}

pub struct FrameEvidence {
    pub start_sample: u64,
    pub end_sample: u64,
    pub speech_probability: f32,
}

pub trait SpeechDetector {
    fn identity(&self) -> DetectorIdentity;
    fn detect(&mut self, audio: &CanonicalAudio<'_>)
        -> Result<Vec<FrameEvidence>, DetectorError>;
}

pub struct SampleRange {
    pub start_sample: u64,
    pub end_sample: u64,
}

pub enum BoundaryKind {
    SourceEnd,
    VadValley,
    HardLimitNoCandidate,
    HardLimitDetectorUnavailable,
}

pub struct RecognitionChunk {
    pub id: String,
    pub ordinal: u32,
    pub core: SampleRange,
    pub submitted: SampleRange,
    pub boundary: BoundaryDecision,
    pub preflight: PreflightResult,
}

pub struct RecognitionPlan {
    pub schema: String,
    pub id: String,
    pub revision: u64,
    pub source: SourceFacts,
    pub recognizer: RecognizerContract,
    pub detector: DetectorRun,
    pub planner: PlannerRun,
    pub chunks: Vec<RecognitionChunk>,
    pub failures: Vec<PlanFailure>,
}
```

Use a safe integer type for all arithmetic. Convert slice lengths to `u64` with
checked conversions. Reject overflow rather than wrapping.

## 6. Input validation

Before running Silero:

- reject sample rate other than 16,000;
- reject channel count other than one;
- reject a source hash with invalid project format;
- reject NaN and infinite samples;
- define whether values outside `[-1.0, 1.0]` are rejected or clipped; prefer
  rejection unless the existing audio layer guarantees normalization;
- reject zero `max_submitted_samples`;
- reject zero `minimum_chunk_samples`;
- reject `minimum_chunk_samples > max_submitted_samples`;
- reject thresholds outside `[0.0, 1.0]`;
- reject non-zero padding in v1 with a clear unsupported-configuration error;
- accept empty audio only if the project has an explicit empty-plan policy.

Recommended v1 empty policy: return a valid plan with no chunks and decoded
sample count zero. Do not call Silero.

## 7. Silero adapter

### Initialization

The adapter receives:

- model path;
- expected model SHA-256;
- ONNX Runtime execution settings;
- Silero model/sample-rate configuration.

Verify the file hash before creating the session. Report model-not-found,
hash-mismatch, invalid-model, and runtime-initialization errors separately.

### Inference

- Feed samples in the frame size required by the pinned Silero model.
- Preserve the model's recurrent state exactly as required by that version.
- Reset state between audio sources.
- Zero-pad only the final detector frame if the model requires a complete frame.
- Clip evidence end positions to the real decoded sample count.
- Return finite probabilities in `[0.0, 1.0]` and exact sample ranges.
- Treat malformed model output as a detector error.

The adapter must not merge frames into chunks. It only produces evidence.

### Detector failure

Return the error to the planner. The planner records a summarized safe error and
continues using fixed legal windows. Do not store sensitive full local paths in
portable plan output.

## 8. Planner algorithm

The planner is a pure function of source facts, recognizer contract,
configuration, and detector evidence/result.

For v1, use this process:

1. Set `core_start = 0`.
2. If remaining samples are at most `max_submitted_samples`, create the final
   chunk ending at source end.
3. Otherwise set `hard_end = core_start + max_submitted_samples` using checked
   arithmetic.
4. Set `search_start` to the greater of:
   - `core_start + minimum_chunk_samples`; and
   - `hard_end - search_back_samples` using saturating subtraction.
5. Set `search_end = hard_end`.
6. Find low-speech candidate runs fully or partly inside the search interval.
   A frame is low speech when `speech_probability < speech_threshold`.
7. Join consecutive low-speech frames. Clip the run to the search interval.
8. Keep runs whose clipped length is at least
   `minimum_low_speech_samples`.
9. For each qualifying run, use its integer midpoint as its candidate sample.
10. Select the candidate with the lowest mean speech probability. On equal
    probability, select the candidate nearest `hard_end`. If still equal,
    select the later sample. Document exact floating comparison behavior.
11. If a candidate exists, end the core at that sample and record `VadValley`,
    the search interval, selected run, and probability summary.
12. If none exists, end at `hard_end` and record `HardLimitNoCandidate`.
13. If detection failed, skip candidate search, end at `hard_end`, and record
    `HardLimitDetectorUnavailable` plus the detector error code.
14. Create `submitted = core` because v1 padding/overlap is zero.
15. Set `core_start = core_end` and repeat.

The selected sample must always be greater than `core_start`. If evidence is
missing, unordered, overlapping unexpectedly, non-finite, or outside the source,
validation must reject it or treat the detector run as failed. Never loop on an
unchanged start position.

## 9. Important policy boundary

The algorithm above makes the planning rule exact, but its numeric configuration
is not product policy. Do not invent hidden defaults for:

- `search_back_samples`;
- `minimum_chunk_samples`;
- `speech_threshold`;
- `minimum_low_speech_samples`.

Tests may define clearly named fixture values. A development harness may require
all values explicitly or use a profile named `experimental-v1`. Every value
must appear in serialized plan data.

## 10. Plan validation

Run validation before returning success. Check:

- source sample count matches the planning input;
- ordinals are continuous from zero;
- each range has `start < end`;
- first core starts at zero;
- adjacent cores meet exactly;
- final core ends at source sample count;
- submitted contains its core;
- submitted range is inside the source;
- submitted length is at most recognizer maximum;
- v1 submitted equals core;
- selected boundaries are inside their declared search intervals;
- boundary kinds and evidence are consistent;
- chunk and plan IDs match canonical content.

Validation failure returns no successful partial plan.

## 11. Serialization and identity

Serialize with a schema value such as:

```text
recognition-plan/v1-experimental
```

JSON output must use stable field names and integer samples. Optional display
seconds may be included as derived strings or numbers, but are never canonical.

Create the plan hash from canonical serialized content excluding:

- plan ID itself;
- creation timestamp;
- local source/model paths;
- non-deterministic runtime measurements.

Include source hash, decoded facts, recognizer contract, detector identity/model
hash, planner version/config, boundary decisions, and ranges. Create each chunk
ID from the plan content identity plus ordinal and ranges. If self-referential
hashing is awkward, define a separate `plan_inputs_hash` and derive IDs from it.
Document the method with a golden fixture.

## 12. Failure model

Use typed errors for failures that prevent a plan:

- invalid canonical audio;
- invalid configuration;
- integer overflow;
- invalid detector evidence;
- internal invariant failure;
- serialization/hash failure.

Silero initialization/inference errors do **not** prevent a fixed-window plan,
unless the caller explicitly selects a future strict mode. Record them in the
detector run and plan failures with a stable error code.

Do not panic for user input, missing files, corrupt models, runtime errors, or
invalid evidence. Panics are acceptable only for impossible internal states and
should be removed where practical.

## 13. Tests

### Pure planner unit tests

- Empty input returns the documented empty plan.
- One sample and a very short source produce one chunk.
- Exactly 480,000 samples produce one chunk.
- 480,001 samples produce complete legal chunks.
- Clear qualifying valley is selected.
- Valley before the search interval is ignored.
- Too-short valley is ignored.
- Lowest-mean candidate wins.
- Tie rules choose the expected later/near-limit candidate.
- Continuous high probability uses hard limit.
- Final short tail is preserved.
- Detector failure produces the fixed plan and stable reason.
- Non-zero v1 padding is rejected.
- Invalid and overflowing configurations are rejected.

### Property tests

Generate source lengths, legal contracts, configurations, and valid frame
evidence. Assert:

- planner terminates;
- core union is exactly `[0, sample_count)`;
- core ranges do not overlap;
- submitted range length never exceeds maximum;
- all positions are in bounds;
- repeated input gives the same plan;
- changing source/config/model identity changes the plan-input identity.

### Silero integration tests

With a pinned model and small reviewed fixtures:

- model hash is verified;
- state resets between sources;
- final partial frame is clipped correctly;
- silence produces low probabilities in expected regions;
- speech fixture produces speech evidence;
- corrupt/missing/wrong-hash models trigger fixed fallback;
- two runs on the supported CPU environment produce equivalent evidence and the
  same plan.

Do not assert overly precise probability values across all ONNX platforms.
Golden-test the selected supported environment and use semantic ranges elsewhere.

### Golden plan tests

Commit small JSON fixtures for:

- clear pause;
- continuous speech fallback;
- detector-unavailable fallback;
- exact limit and one-sample-over cases.

Review golden changes as contract changes, not routine snapshot updates.

## 14. Development harness

If needed, add a non-public binary that:

1. accepts canonical WAV or an existing project source reference;
2. receives Silero model path/hash and all planner values explicitly;
3. writes one plan JSON document to stdout;
4. writes diagnostics to stderr;
5. performs no network access and no Whisper recognition.

Mark its grammar experimental. Library behavior is the deliverable.

## 15. Observability

Optional runtime diagnostics may include:

- decoded samples and duration;
- detector frame count and runtime;
- number of candidate valleys;
- chosen boundary/reason;
- chunk count and maximum length;
- fallback count;
- validation result.

Diagnostics must not change canonical plan identity and should not expose full
private paths by default.

## 16. Completion checklist

The task is complete when:

- Rust formatting, linting, and tests pass;
- Silero model loading and stateful inference work from Rust;
- pure planner and fallback are implemented as specified;
- every nonempty canonical source receives complete legal core coverage;
- every submitted range is at most 480,000 samples;
- no audio is removed because Silero calls it non-speech;
- plan validation is mandatory;
- deterministic JSON and identity have golden tests;
- detector errors produce recorded fixed plans without panics;
- dependency/model pins and licenses are documented;
- the README explains how to run tests and the internal harness;
- no Whisper execution, paragraphs, or overlap reconciliation were added.

## 17. Reviewer notes

Reject the change if it:

- uses floating-point seconds as canonical boundaries;
- deletes silence or speech from core coverage;
- treats each Silero speech region as a final recognition chunk;
- allows a submitted range over 480,000 samples;
- silently uses unrecorded thresholds;
- downloads a model automatically;
- depends on paragraph identity;
- hides detector failure;
- produces partial successful plans;
- adds overlap without reconciliation evidence;
- mixes recognition execution into the planner.

## 18. Follow-up work

After this task, run the real-speech experiment corpus through this planner and
the selected `whisper-rs` branch. Compare fixed and Silero-guided seams. Measure
omissions, duplicates, substitutions, global/seam WER, runtime, and memory. Use
that evidence to select configuration, decide padding/overlap, and create a
dated decision record.
