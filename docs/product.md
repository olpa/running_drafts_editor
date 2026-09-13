# Product intent

Running Drafts Editor helps one person recover a rough spoken draft. It starts
with existing audio and an imperfect transcription and ends with usable text
for another writing tool. Recovering the intended thought matters more than
producing a perfect verbatim transcript.

## User and job

The primary user records personal ideas while moving or away from a keyboard.
They want to read first, replay only unclear passages, correct mistakes that
block understanding, and stop when the text is good enough.

The product should reduce the cost of resolving uncertainty without turning
the session into exhaustive proofreading. An issue points to text worth
reviewing; it is neither proof of an error nor work the user must complete.

## Product principles

- **Text first.** The document is what the user reads, edits, and exports.
  Audio and transcription data support that work.
- **Audio in context.** Replay answers a local question about the text. It is
  entered from the document and should recede after answering that question.
- **Honest assistance.** The product exposes uncertainty without claiming false
  precision or silently replacing newer user work.
- **Good enough.** Signals and controls should help recover meaning, not imply
  that every word requires correction.
- **Progressive disclosure.** Reading, quick replay, and ordinary correction
  stay direct. Less common details and operations remain secondary.
- **Hidden internals.** Normal product views use familiar document concepts.
  Model data, token identities, overlap, and decoder state belong to supporting
  data or developer tools.
- **Familiar structure.** Paragraphs present the composition without requiring
  the user to understand how transcription divided the audio. The feasibility
  CLI exposes chunk boundaries for inspection; the intended product need not.

## Product boundary

The current line-oriented CLI is a technical-feasibility surface for the text,
transcription, replay, correction, and recovery model. It is not the intended
final interaction design.

Waveform editing and timeline scrubbing optimize audio manipulation rather than
thought recovery, so they are outside the primary interaction. Publishing,
collaboration, subtitle authoring, and professional multi-speaker transcription
are also outside the product's core job. A future mobile experience may
optimize for one-handed use, but that is a product hypothesis rather than a CLI
requirement.

Voice replacement earns a place only when it is faster or easier than typing.
Quick-replay context, correction-audio retention, issue ranking, and the final
mobile gestures remain open product questions until tested with users.

An early validation target is to turn an eight-minute recording into usable
text in roughly three to six minutes. This is a research hypothesis, not a
service guarantee; user-rated usefulness and time saved matter more than the
percentage of issues reviewed.
