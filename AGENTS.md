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
  paragraph inside a chunk requires splitting that chunk first. `split` and
  `isplit` create a chunk boundary before a token, `asplit` creates one after,
  and `parasplit` splits a paragraph after a chunk marker. `Mmerge` joins
  paragraphs without changing text or joining their chunks; `M@Nmerge` joins
  compatible chunks around a marker.
- Chunks are at most about 30 seconds and may overlap.
- Derived chunk splits retain immutable parent provenance and keep replay
  available: use an exact shared token boundary, the midpoint of a mapped gap
  with aligned status, or the complete parent range with inherited status.
  Derived chunk identity, token membership, parents, and mapping alignment are
  persisted. Chunk merges require one audio source and at most 480,000 samples.
- Recognition runs are immutable; retries and boundary changes create revisions.
- Selection and text editing use ranges of complete visible tokens; a token
  cannot be selected or edited in part. Initial tokens refer to accepted normal
  Whisper tokens. Edits may replace or add them with indivisible pseudo-tokens
  that preserve user-authored text. Timestamp and other special tokens remain
  recognition evidence and are not selectable text.
- Text-edit ranges never cross paragraph boundaries. `M.Ninsert TEXT` inserts
  one pseudo-token before the addressed token, `M.Nappend TEXT` inserts one
  after it, `M.N,M.Ureplace TEXT` replaces an inclusive same-paragraph range,
  and `M.N,M.Udelete` deletes one. The complete supplied `TEXT` is one
  indivisible pseudo-token with unavailable alignment; it is not divided with
  the Whisper tokenizer. When `replace` or `delete` has no address, it uses the
  current complete-token selection and rejects missing, non-token,
  cross-paragraph, or stale selections. Unquoted replacement text retains the
  selected text's leading and trailing whitespace; a fully quoted replacement
  controls both boundaries exactly and supports `\"` and `\\` escapes.
- Token addresses are derived from the current paragraph revision. Stored
  carets and selections use stable token or chunk identities and paragraph
  revisions rather than treating displayed numbers as stable IDs. Token ranges
  may cross paragraphs and store their displayed inclusive endpoint identities.
- Character offsets, character spans, and partial-token positions are not part
  of the document, selection, editing, mapping, replay, or persistence model.
  Token text is opaque to these operations.
- Initial visible tokens are the accepted non-special recognition tokens in
  chunk order. Their exact concatenation is authoritative and is not trimmed.
  If normal tokens are missing or do not reproduce a chunk's text, that complete
  chunk text becomes one indivisible pseudo-token with unavailable alignment;
  the CLI reports the fallback and retains the recognition evidence.
- The CLI renders selectable chunk-boundary symbols distinctly from paragraphs.
- Chunk symbols and recognition metadata are absent from clean text export.
- Edits may make alignment stale or unavailable; never claim false precision.
- Missing audio or optional metadata must not prevent reading and editing text.
- The baseline `rde-document/v1-experimental` JSON format preserves exact visible
  tokens, paragraph revisions, stable IDs, chunk markers, and optional canonical
  audio mappings. `rde edit <document.rde.json>` opens it without recognition;
  session `save [PATH]` uses atomic replacement. Session `load PATH` and `edit
  PATH` replace the current document and reset navigation. `audition --output
  PATH` saves the recognized baseline before the prompt.

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
  paragraph, and `M@Nplay` plays the complete chunk immediately to the left of
  marker `M@N`; `M@Ninfo` inspects that chunk. A bare token or marker address
  moves the caret, `Aselect` selects a token, token range, paragraph, or marker,
  and `Mtokens` lists the individually addressable tokens in a paragraph.
- `[A]play` replays the current or addressed token range, paragraph, or marker;
  text replay adds configurable fixed context while marker replay stays exact.
  `[A]slowplay` uses 0.75 speed, `replay`/`slowreplay` repeat the last resolved
  range, and `stop` ends active playback. Current-revision token audio mappings
  carry exact, aligned, inherited, stale, or unavailable alignment; partial
  coverage is reported separately and mappings across audio sources are refused.
- `M@N,M@U` is a half-open marker-bounded interval: the left boundary is
  included and the right boundary excluded. It selects the visible interval
  between stable markers and replays the complete chunks after the left marker
  through the chunk at the right marker. Marker ranges may cross paragraphs but
  replay refuses missing mappings or multiple audio sources.
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
