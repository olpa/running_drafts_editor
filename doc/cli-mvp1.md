# CLI MVP1 Work Plan

Check the technical feasibility of the document, recognition, alignment, replay,
and editing models in a line-oriented program running in a typical dumb
terminal. Waveforms, full-screen terminal UI, publishing, rich annotations,
multiple speakers, collaboration, and language-server integration are out of
scope.

## [x] Recognize audio and create recognition chunks

Recognize long audio through overlapping ranges of about 30 seconds or less.
Derive advancing chunk boundaries from recognition timestamps and store the
boundaries and overlap explicitly.

## [x] Transcribe chunks and preserve recognition runs

Store tokens, confidence, alternatives, timing, and failures as immutable run
output. Retrying creates another run.

## Reconcile overlapping chunk text

Produce one visible sequence from overlapping results without deleting source
runs. Flag ambiguous reconciliation as an issue.

## [x] Show replay-chunk boundaries in recognized text

Group replay chunks into initial paragraphs and show a distinct marker after
every chunk. Address markers as `M@N`, where `M` is the paragraph number and
`N` is the chunk number inside it. Show existing chunk details with `M@Ninfo`.
Reserve `M.N` for visible-token addresses.

## [x] Implement a compact CLI command grammar

Use a small line-oriented, address-first command set with help and predictable
errors. The grammar takes inspiration from `ed`: an optional paragraph, token,
marker, or range address precedes a command. It is not a modal editor and does
not reproduce `ed` exactly. See `navigation.md`.

## [x] Add cursor and token-range selection

Maintain a cursor over visible text tokens and support caret, single-token,
token-range, and paragraph selection. Token ranges may cross paragraphs and
always contain complete tokens. Initial tokens refer to accepted normal Whisper
tokens; mismatched or missing token evidence and later user edits may introduce
indivisible pseudo-tokens. Chunk-boundary symbols are individually selectable.
See `navigation.md`.

## [x] Show document position and selection clearly

Render paragraph boundaries, chunk-boundary symbols, cursor, and selected text
as visually distinct terminal output.

## Save and restore the visible-document baseline

Persist paragraphs, audio references, and stable IDs early. A document must
remain readable when recognition metadata is absent.

## Replay at the cursor or selection

Play the mapped audio with a short configurable context window. Report when
alignment is inherited, stale, partial, or unavailable.

## Add extended replay commands

Support replaying a selection or paragraph, replaying again, stopping, and
slower playback. Keep waveform and timeline controls out of scope.

## Edit authoritative visible text

Support insertion, replacement, and deletion over complete-token ranges.
Preserve mappings for retained tokens and degrade affected alignment honestly.

## Split and merge visible paragraphs

Split at the cursor and merge adjacent paragraphs. A paragraph longer than 30
seconds remains backed by multiple internal chunks.

## Add undo for text and paragraph operations

Undo insertion, replacement, deletion, split, and merge together with their
mapping changes.

## Inspect and choose recognition alternatives

Show alternatives relevant to the current visible range and apply one as an
ordinary text replacement. Hide alternatives invalidated by later edits.

## Replace a selection by voice

Record a short correction, recognize it, and replace the selected text. On
failure or a stale selection, leave visible text unchanged.

## Navigate and dismiss issues

Support next issue, previous issue, and intentional ignore. Start with simple
confidence and processing-failure signals without fixing a final ranking rule.

## Search visible text

Find text and move the cursor or selection to a result without consulting token
boundaries.

## Add an explicit developer recognition refresh

Allow experiments with language or recognition settings on a chunk or audio
range. Present results without silently overwriting user-edited text.

## Complete end-to-end persistence

Extend the baseline with recognition backing, mappings, issues, and enough
history for undo. Preserve recovery when optional metadata is unknown or lost.

## Recover from missing audio and failed recognition

Keep reading and text editing available. Explain unavailable replay locally and
allow recognition retry without losing visible work.

## Export usable text

Write clean paragraph text without tokens, confidence, chunk markers, or other
recognition internals.
