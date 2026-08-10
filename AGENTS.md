# Agent Context

## Mission

Build Running Drafts Editor: turn existing audio and an imperfect transcript
into usable text. The user edits text; audio and recognition data support replay
and correction.

Current phase: technical feasibility through a line-oriented CLI for a typical
dumb terminal. Implement the assigned ticket without expanding into a TUI or
production product.
The repository is one Rust package: `rde` is the executable and `src/lib.rs`
exposes reusable tool modules. Recognition chunking lives in the `chunking`
module rather than a separate product or crate.

## Read when needed

1. `doc/cli-mvp1.md` — ordered CLI work plan and current scope.
2. `doc/chunking.md` — durable recognition-chunking context.
3. `doc/transcript-cleanup-ui-proposal-v0.1.md` — technical model and invariants.
4. `doc/thought-recovery-transcriber-prd-v0.1.md` — product intent and non-goals.

When documents disagree, preserve visible text and data recoverability, follow
the CLI plan for MVP scope, and surface the conflict instead of inventing a
large design.

## Durable constraints

- Visible paragraph text is authoritative.
- Paragraphs are editing units made from one or more complete replay chunks;
  every paragraph boundary is also a replay-chunk boundary. Splitting a
  paragraph inside a chunk requires splitting that chunk first.
- Chunks are at most about 30 seconds and may overlap.
- Recognition runs are immutable; retries and boundary changes create revisions.
- Selection and text editing use ranges of complete visible tokens; a token
  cannot be selected or edited in part. Initial tokens refer to accepted normal
  Whisper tokens. Edits may replace or add them with indivisible pseudo-tokens
  that preserve user-authored text. Timestamp and other special tokens remain
  recognition evidence and are not selectable text.
- Token addresses are derived from the current paragraph revision. Stored
  selections use stable token identities and the paragraph revision rather than
  treating displayed token numbers as stable IDs.
- Character offsets, character spans, and partial-token positions are not part
  of the document, selection, editing, mapping, replay, or persistence model.
  Token text is opaque to these operations.
- The CLI renders selectable chunk-boundary symbols distinctly from paragraphs.
- Chunk symbols and recognition metadata are absent from clean text export.
- Edits may make alignment stale or unavailable; never claim false precision.
- Missing audio or optional metadata must not prevent reading and editing text.
- Async or refreshed recognition must not overwrite newer user edits.
- Canonical audio and recognition positions use mono 16 kHz sample offsets;
  recognition identities exclude local paths and nondeterministic diagnostics.
- WAV input accepts common 8/16/24/32-bit integer PCM and 32-bit float formats;
  multiple channels are averaged and the result is resampled to canonical mono
  16 kHz audio before recognition.
- Chunk boundaries are derived during Whisper recognition from timestamped
  decoded segments; there is no separate pre-recognition planner.
- A missing or unreadable CLI model is a preflight error.
- `rde audition --input <audio.wav> --model <whisper.bin>` is a
  developer-only dumb-terminal harness: it runs Whisper, lists accepted decoded
  segments with text and timestamp-derived sample ranges, and plays them through
  a replaceable, ffplay-compatible subprocess selected with `--player`.
- The dumb-terminal shell uses an `ed`-inspired address-first grammar at the
  `rde>` prompt. `M.N` addresses a visible token, `M@N` a chunk marker, and
  `M.N,M.U` an inclusive displayed token range. Commands may attach to an
  address or follow it after whitespace. `p` prints the document, `Mp` prints a
  paragraph, and `M@Nplay` and `M@Ninfo` act on a chunk marker.
- Whisper recognition uses explicit windows of at most 480,000 samples. The
  experimental default targets a 384,000-sample core with 48,000 samples of
  context per side. It reuses normal text-token IDs from the last accepted
  segment directly as the next prompt, without converting through text;
  timestamp and other special tokens remain evidence but are not prompt input.
  It uses the latest segment end in the 48,000-sample right context as the
  boundary, or the target core end when that area has no usable timestamp.
- Every window hypothesis is retained in the immutable run; midpoint ownership
  provides only minimal overlap deduplication pending full reconciliation.
- After recognition, accepted Whisper segments are grouped whole into replay
  chunks. Defaults are 8/32/64 normal text tokens and 300/800/2,000 ms usable,
  strong, and unconditional long pauses; stored boundary reasons remain
  inspectable.
- Initial paragraphs join consecutive replay chunks and end at long-pause or
  source-end boundaries. The CLI renders a marker after every chunk and
  addresses it as `M@N`, with `M` as the paragraph number and `N` as the chunk
  number inside that paragraph; `M@Ninfo` shows its chunk information. Visible tokens use `M.N`.
- Whisper Rust and C++ sources are pinned and vendored under
  `vendor/whisper-rs`; `RDE-VENDOR.md` records their exact provenance. The
  backend builds statically without project-specific build variables or a
  runtime shared-library path. Whisper model binaries remain external.

## MVP boundary

In scope: chunking, recognition backing, overlap reconciliation, paragraphs,
cursor/selection, replay, text edits, split/merge, undo, alternatives, voice
replacement, issues, search, refresh, persistence, recovery, and export.

Out of scope: waveforms, timeline scrubbing, full-screen TUI, publishing, rich
annotations, multi-speaker UI, collaboration, LSP, and mobile UX design.

## Working rules

- Keep each change limited to its ticket and the smallest supporting work.
- Prefer simple, inspectable representations over generic frameworks.
- Preserve old recognition evidence; users never edit it directly.
- Add focused tests for implemented behavior and failure paths.
- Do not commit editor swap files, recordings, credentials, or generated output.
- Record unresolved choices rather than silently fixing open product questions.
- Use clear B2-level English in project text whenever possible. Prefer common
  words such as “boundary” over less familiar technical metaphors.

## Keep this context current

Update `AGENTS.md` in the same change whenever implementation introduces a new
feature, durable constraint, architectural decision, command convention, or
important technical insight. Replace stale statements; keep the file brief and
broad. Do not turn it into a changelog or duplicate details available in the
source documents.
