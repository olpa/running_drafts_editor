# Transcription chunking

This document preserves the cross-cutting initial-transcription procedure.
[ADR-0013](adr/0013-finalize-chunks-incrementally-from-decode-spans.md)
records the boundary model, and
[ADR-0014](adr/0014-store-document-content-inside-decode-spans.md) records
the ownership direction. [ADR-0015](adr/0015-use-timestamp-closed-continuation-and-exact-transcription-prompts.md)
records the continuation-candidate and transcription-prompt policy. Code and
tests remain the source for current parameter values and implemented behavior.
[Issue #58](https://github.com/olpa/running_drafts_editor/issues/58)
records the implementation work for disjoint finalized chunk ranges.

## Ownership model

Initial transcription processes one active decode span at a time. A decode
span contains the bounded audio submitted in one step, the resulting decoder
evidence, and the content finalized from that evidence. It is decoded once.

The next decode span begins at a continuation boundary near the end of the
current span. It therefore decodes the current span's remaining suffix again.
Decode spans may overlap, but finalized chunks from the same audio source must
be ordered and disjoint. Silence and other unused audio may remain outside all
finalized chunk ranges.

## Procedure

1. Convert the source to canonical mono 16 kHz audio. All positions below are
   sample offsets in that coordinate system.
2. Submit one bounded decode span beginning at the previous continuation
   boundary. Do not add a separate left- or right-context region.
3. Decode the span once with timestamps. Retain the complete decoder result as
   evidence, including the suffix that does not produce finalized content in
   this pass.
4. Feed the result into the forward-only paragraph-construction fold so it can
   identify chunk and paragraph boundaries. It does not revise content
   finalized by an earlier step.
5. Form the longest ordered prefix of Whisper segments with valid, advancing,
   non-overlapping timestamps. Within the final six seconds, prefer the latest
   segment end followed by at least 800 ms of silence; otherwise use the latest
   segment end there, then the latest earlier end. Put the boundary before the
   pause. If there is no usable end, advance the complete submitted span without
   accepting unlocated text.
6. Retain the processed decode span, its evidence, and its produced content in
   the project. It is no longer the active span.
7. Start the next decode span at the continuation boundary. The suffix is
   decoded again and may produce visible text only in this later pass.
8. Accumulate the exact accepted text-token IDs in recording order. Before the
   next decode, retain the newest suffix that fits the loaded model's prompt
   capacity and pass it as the transcription prompt. Do not recreate IDs with a
   text round trip, and do not pass control tokens. Clear the history after a
   decode failure or a complete fallback span with no accepted text.

Every non-final step must advance. A silence-only span may produce no chunk and
advance to its submitted end. If decoding fails or useful timestamps are
missing, a bounded-progress fallback retains raw timestamp and text evidence
but does not invent an audio range for unlocated text. At source end, the fold
finalizes the valid prefix and trailing silence stays unowned.

## Incremental chunk and paragraph construction

The fold applies the existing pause and token-size policy incrementally instead
of grouping all accepted segments after the whole recording has been decoded:

- A long pause ends a chunk and a paragraph.
- A strong pause ends a chunk after the minimum size.
- Usable pauses compete near the target size; the score rewards longer pauses
  and penalizes distance from the target token count.
- At the maximum size, the closest earlier complete segment boundary is used.
- Source end finishes the last eligible chunk and paragraph.

The last finalized chunk in a non-final accepted prefix ends at the
continuation boundary with boundary reason `Continuation`. This boundary does
not by itself end a paragraph. Other finalized chunks and paragraph breaks may
precede it in the same decode span.

Finalized chunks keep their identities and ranges through later editing as
required by [ADR-0008](adr/0008-keep-finalized-chunks-stable.md). Their order is
the original recording order. The current document is obtained from the mixed
content stored across decode spans rather than from a duplicate chunk-ID list.

## Experiment record

ADR-0013 preserves the reason from superseded ADR-0003 for rejecting Silero
voice activity detection as a separate boundary authority. The experiment
assigned low speech probability to clearly audible quiet speech. Pause duration
remains useful after transcription as evidence for chunk and paragraph breaks;
it is not proof that an earlier interval contains no voice.

The experiment and its measurements remain in [GitHub issue #25](https://github.com/olpa/running_drafts_editor/issues/25).
The initial Whisper design and overlap requirements are in
[issue #2](https://github.com/olpa/running_drafts_editor/issues/2) and
[issue #3](https://github.com/olpa/running_drafts_editor/issues/3).
