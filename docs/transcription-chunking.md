# Transcription chunking

This document preserves the cross-cutting initial-transcription procedure.
[ADR-0003](adr/0003-transcription-driven-chunk-boundaries.md) records why it
replaced a separate voice-activity planner. Code and tests remain the source
for current parameter values and exact implementation behavior.

## Ownership model

Initial transcription examines overlapping provisional chunks while assigning
every source sample to one consecutive, non-overlapping owned core.

Overlap is transcription evidence, not intended final ownership. The agreed
domain model requires finalized chunks to be disjoint, but initial
transcription does not yet guarantee this because an accepted segment may
begin before its owned core. [Issue #58](https://github.com/olpa/running_drafts_editor/issues/58)
tracks that gap. Historical editor versions could also split and join chunks;
current editing keeps finalized chunk identity and boundaries fixed while
retaining old saved states and history as described in [ADR-0008](adr/0008-keep-finalized-chunks-stable.md).

## Procedure

1. Convert the source to canonical mono 16 kHz audio. All positions below are
   sample offsets in that coordinate system.
2. Place a cursor at the first sample not owned by an earlier step.
3. Submit left context, a target core beginning at the cursor, and right
   context, within Whisper's input limit.
4. Decode the submitted audio with timestamps. Retain every decoded segment as
   immutable evidence, including segments not selected for visible text.
5. Around the target end, choose the latest usable decoded-segment end in the
   right context. Use source end for the final core and the target end as the
   bounded-progress fallback.
6. Accept a decoded segment for the ordered text sequence when its midpoint is
   at or after the current cursor and its end is no later than the chosen
   boundary. This is deliberately minimal overlap reconciliation; the other
   decoded segments remain available as transcription evidence.
7. Advance the cursor to the boundary and repeat. The resulting owned ranges
   cover the source consecutively without gaps or overlap, and every iteration
   advances even when one window's decoding fails.
8. Pass text-token IDs from the last accepted segment directly as the next
   Whisper prompt. A text round trip could change the token sequence; timestamp
   and other special tokens carry invalid window-local state.

## From accepted segments to finalized chunks

After all provisional chunks have been processed, the implementation groups
the accepted Whisper segments into finalized chunks. It keeps every segment
whole so it does not invent a boundary inside text for which Whisper supplied
only a segment-level timestamp. Once finalized, a chunk keeps its identity and
boundaries through text editing and paragraph restructuring.

Grouping balances text-token size goals with usable, strong, and long pauses:

- A long pause always ends a chunk.
- A strong pause ends a chunk after the minimum size.
- Usable pauses compete near the target size; the score rewards longer pauses
  and penalizes distance from the target token count.
- At the maximum size, the closest earlier whole-segment boundary is used.
- Source end finishes the last chunk.

Boundary reasons and pause lengths remain inspectable. Initial paragraphs join
consecutive finalized chunks and end at a long-pause or source-end boundary.
Other chunk boundaries remain visible inside the paragraph.

Current defaults and the exact scoring rule live in `RecognitionConfig` and
`PostChunkConfig` in `src/recognition.rs`.

## Experiment record

ADR-0003 records why Silero voice activity detection was rejected as the
pre-transcription boundary authority. Pause duration remains useful after
transcription, when it is a gap between accepted timestamped segments and helps
group them into finalized chunks; it is not proof that an earlier interval
contains no voice.

The experiment and its measurements remain in [GitHub issue #25](https://github.com/olpa/running_drafts_editor/issues/25).
The initial Whisper design and overlap requirements are in [issue #2](https://github.com/olpa/running_drafts_editor/issues/2)
and [issue #3](https://github.com/olpa/running_drafts_editor/issues/3). The
current code uses `ProcessingWindow`, `core`, and `RecognitionChunk` as legacy
names; [issue #57](https://github.com/olpa/running_drafts_editor/issues/57)
owns their vocabulary review.
