# Recognition chunking: technical context

**Status:** durable project context  
**Date:** 2026-08-07

## Purpose

Running Drafts Editor divides long audio into bounded recognition operations
and turns their timestamped results into replayable text chunks. Recognition
chunks are processing units; paragraphs remain independent user-editable units.

## Current decision

Chunking happens as part of Whisper recognition. The recognizer receives
overlapping windows of canonical mono 16 kHz audio, each no longer than its
native 30-second limit. Timestamped decoded segments determine advancing seams.
A bounded tail of accepted text is passed as context for the next window.

The default experiment uses:

- a 384,000-sample target core (24 seconds);
- 48,000 samples (3 seconds) of context on each side;
- a 480,000-sample (30-second) maximum submitted window;
- a 160,000-sample (10-second) minimum fallback advance;
- at most 1,000 characters of previous accepted text as a prompt.

Every submitted-window hypothesis is immutable recognition evidence. Midpoint
ownership is the initial deterministic overlap rule, not a final reconciliation
algorithm. A decode failure records the failed window and advances by the
bounded fallback so that processing always terminates and covers the source.

## Earlier experiment

We tried Silero VAD as a separate pre-recognition chunk planner. On representative
sample data it assigned very low speech probabilities to clearly audible,
continuously recognized speech and therefore proposed misleading boundaries.
We removed that implementation and decided to derive chunks simultaneously
with recognition, using Whisper timestamps and decoded text as the evidence.

## Canonical positions and identity

Audio positions use mono 16 kHz sample offsets. Seconds are display values only.
Source and recognition identities exclude local paths and nondeterministic
diagnostics. Model identity uses a hash of the caller-supplied model file.

Each processing window records:

- its submitted audio range;
- its non-overlapping core range;
- the prompt supplied to recognition;
- all decoded segment and token hypotheses;
- accepted segment identities;
- the reason for its advancing seam;
- any decoding error.

Recognition runs are immutable. Retrying with another model, language, prompt,
or boundary policy creates a new run rather than mutating old evidence.

## Replay and visible text

The developer audition command lists accepted decoded segments with their text
and timestamp-derived sample ranges. Playback uses exactly the listed range.
Later document work may group several chunks into a paragraph or split a chunk
across paragraphs. Chunk metadata must never appear in clean text export.

Visible paragraph text remains authoritative. Recognition metadata supports
replay and correction but must not overwrite newer edits or claim precision
after alignment becomes stale.

## Dependencies and reproducibility

The selected `olpa/whisper-rs` and `olpa/whisper.cpp` source snapshots are
vendored under `vendor/whisper-rs`. Their exact commits and local static-build
adaptation are recorded in `vendor/whisper-rs/RDE-VENDOR.md`. Recognition model
binaries remain external and should be identified by SHA-256.

Tests should cover bounded complete core coverage, overlap ownership, timestamp
normalization, prompt propagation, decode failure, missing models, invalid
audio, replay precision, and stable recognition-run serialization.
