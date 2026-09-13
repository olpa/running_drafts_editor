# Transcription chunking

This document explains the current initial-transcription procedure and why it
replaced a separate voice-activity planner. The domain terms are defined in
root `CONTEXT.md`; current Rust identifiers still use legacy terminology
pending GitHub issue #57.

## Why provisional chunks overlap

Whisper accepts at most about 30 seconds of audio at once. Cutting independent
30-second pieces can deprive speech near an edge of useful context and can cut
through a phrase. Initial transcription therefore examines overlapping
provisional chunks while assigning every source sample to one consecutive,
non-overlapping owned range.

Overlap is transcription evidence, not final ownership. Finalized chunks are
disjoint, so replay and document structure never need to decide which of two
chunks owns the same audio.

## Current procedure

The current code represents the provisional stage with `ProcessingWindow` and
`core` values rather than a type named `ProvisionalChunk`.

1. Convert the source to canonical mono 16 kHz audio. All positions below are
   sample offsets in that coordinate system.
2. Place a cursor at the first sample not owned by an earlier step.
3. Submit up to three seconds of left context, a target owned range of 24
   seconds, and three seconds of right context. The submitted audio is never
   longer than Whisper's 30-second limit.
4. Decode the submitted audio with timestamps. Retain every decoded segment as
   immutable evidence, including segments not selected for visible text.
5. In the right-context area, choose the latest decoded segment end between the
   24-second target and the end of the submitted audio. That timestamp becomes
   the owned range's end. At source end, use source end; when decoding fails or
   supplies no usable timestamp, use the 24-second target.
6. Accept a decoded segment for the ordered text sequence when its midpoint is
   at or after the current cursor and its end is no later than the chosen
   boundary. This is deliberately minimal overlap reconciliation; the other
   hypotheses remain available as evidence.
7. Advance the cursor to the boundary and repeat. The resulting owned ranges
   cover the source consecutively without gaps or overlap, and every iteration
   advances even when transcription fails.
8. Pass normal token IDs from the last accepted segment directly as the next
   Whisper prompt. Timestamp and other special tokens remain evidence but are
   not valid context for the next window.

The 24-second target and three-second contexts are experimental defaults, not
domain limits. Their canonical values are 384,000 and 48,000 samples.

## From accepted segments to finalized chunks

After all provisional windows have been processed, the implementation groups
the accepted Whisper segments into finalized chunks. It keeps every segment
whole so it does not invent a boundary inside text for which Whisper supplied
only a segment-level timestamp.

The current experimental defaults use 8, 32, and 64 normal text tokens as the
minimum, target, and maximum sizes. Gaps of 300, 800, and 2,000 milliseconds are
usable, strong, and long pauses:

- A long pause always ends a chunk.
- A strong pause ends a chunk after the minimum size.
- Usable pauses compete near the target size; the score rewards longer pauses
  and penalizes distance from the target token count: `pause_ms - 20 ×
  distance_from_32_tokens`.
- At the maximum size, the closest earlier whole-segment boundary is used.
- Source end finishes the last chunk.

Boundary reasons and pause lengths remain inspectable. Initial paragraphs join
consecutive finalized chunks and end at a long-pause or source-end boundary.
Other chunk boundaries remain visible inside the paragraph.

## Why a separate voice detector was rejected

The first implementation used Silero voice activity detection to plan chunks
before Whisper ran. On representative quiet but intelligible speech, Silero
reported very low speech probabilities and proposed boundaries inside audible
speech. Lowering the threshold enough to retain those passages made the
threshold useless as a general speech decision.

The chosen design therefore lets Whisper's own decoded timestamps establish
owned ranges. Pause duration is still useful after transcription, when it is a
gap between accepted timestamped segments and helps group them into convenient
finalized chunks. It is not treated as proof that a pre-transcription interval
contains no voice.

The experiment and its measurements remain in [GitHub issue #25](https://github.com/olpa/running_drafts_editor/issues/25).
The initial Whisper design and overlap requirements are in [issue #2](https://github.com/olpa/running_drafts_editor/issues/2)
and [issue #3](https://github.com/olpa/running_drafts_editor/issues/3).
