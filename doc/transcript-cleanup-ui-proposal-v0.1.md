# Running Drafts Editor: Technical Specification

**Version:** 0.2 (draft)

**Status:** Design specification; serialization and replay heuristics remain open

## 1. Scope

This specification defines the smallest document and interaction-support model
needed to review an existing transcript, replay matching audio, correct visible
text, use voice replacement, and split or merge paragraphs.

It does not specify recording capture, a publishing system, a waveform editor,
subtitle authoring, a general annotation framework, or a professional
multi-speaker transcription workflow. It also does not prescribe a particular
database, file syntax, recognizer, UI framework, or contextual decoding
algorithm.

The model has three layers:

1. **Visible document:** authoritative paragraphs and indivisible visible
   tokens.
2. **Recognition backing:** immutable results and audio alignment used as
   evidence and support.
3. **Derived mappings:** fallible links from complete visible tokens to backing
   data.

This separation prevents recognition internals from becoming the user's
document structure.

## 2. Terminology

- **Document:** the editable transcript artifact plus optional recognition
  backing and mappings.
- **Visible text:** the readable text produced by concatenating complete visible
  tokens. Text inside a token is not separately addressable.
- **Paragraph:** an ordered, user-facing block of visible text.
- **Selection:** a caret, a range of complete visible tokens, a paragraph, or a
  chunk-boundary marker. A token cannot be selected in part.
- **Issue:** a derived indication that a visible-token range may need attention;
  it is not necessarily an error.
- **Audio source:** the existing recording and its stable identifier, duration,
  and optional channel metadata.
- **Recognition plan:** a revisioned description of audio ranges to process.
- **Recognition chunk:** one bounded input range in a plan, normally no longer
  than the recognizer's approximately 30-second limit.
- **Recognition run:** one recognizer execution for a chunk, including status,
  model/config identity when available, and output.
- **Recognition token:** an immutable recognizer-produced unit with text and
  optional alternatives, confidence, and audio range.
- **Visible token:** an indivisible selection and editing unit whose text
  contributes to the current paragraph. It either refers to an accepted normal
  recognition token or is a pseudo-token created for user-authored text.
- **Pseudo-token:** an indivisible visible token created by an edit. It preserves
  exact user-authored text without pretending to be recognizer output.
- **Mapping:** a derived association between one or more complete visible
  tokens and recognition or audio sources.
- **Provenance:** enough information to explain where text or alignment came
  from and to recover from failed processing, without a full archival system.

Character offsets, character spans, and partial-token positions do not exist
in the document model or serialized format. Selection, editing, mappings,
issues, and replay resolution address complete visible-token identities or
ranges. Token text is opaque to these operations.

## 3. Visible document model

The visible document is the primary editable representation.

```text
Document
  id
  schemaVersion
  paragraphs[]              ordered
  audioSources[]            references; may be unavailable
  recognitionBacking?       optional
  mappings[]?               optional, rebuildable or degradable
  extensions?               optional namespaced metadata

Paragraph
  id                         stable across ordinary text edits
  text                       authoritative visible string
  revision                   increments on visible/structural edits
  originRefs[]?              lightweight provenance references
  extensions?
```

Paragraph order and each paragraph's text define the exported document.
Recognition text must never silently override them after a user edit. User-edit
history may be maintained by an undo/event layer, but the first persistent
format need only preserve enough recent operations or snapshots to provide the
promised undo behavior.

A document with only `schemaVersion` and valid paragraphs remains readable and
editable. Audio, recognition, mappings, issues, and extensions are optional for
basic recovery.

## 4. Recognition backing model

Recognition backing stores evidence; users do not edit it directly.

```text
RecognitionPlan
  id
  createdAt
  reason                     initial | split | merge | replacement | recovery
  supersedesPlanId?
  chunks[]

RecognitionChunk
  id
  audioSourceId
  channelId?                 optional; single mixed source is the v1 default
  startTime
  endTime
  overlapBefore?
  overlapAfter?
  intendedParagraphIds[]?    routing hint, not identity

RecognitionRun
  id
  chunkId
  createdAt
  status                     pending | succeeded | failed
  recognizerRef?             model/config reference where available
  tokens[]                   empty when unavailable or failed
  error?                     structured, non-destructive failure information

Token
  id                         unique within its run
  text
  audioRange?
  confidence?
  alternatives[]?
```

Runs and their output are append-only records. A retry creates a new run. A
boundary change creates a new plan and chunks. Old plans/runs may be compacted
under a retention policy only after undo and live mappings no longer depend on
them; compaction is a storage operation, never an in-place rewrite of evidence.

Confidence and alternatives are optional recognizer observations, not truth and
not part of the visible text authority.

## 5. Visible-token and audio mappings

Mappings connect complete visible tokens in a particular paragraph revision
to backing sources.

```text
TokenMapping
  paragraphId
  paragraphRevision
  visibleTokenIds[]           one or more complete, ordered tokens
  recognitionTokenRefs[]?     zero, one, or many evidence references
  audioRanges[]?              source/channel/start/end records
  provenanceRefs[]?           run, edit, or replacement references
  alignmentState
```

The alignment vocabulary is deliberately small:

- **exact:** deterministically retained for unchanged recognized tokens;
- **aligned:** produced or re-established by a model/aligner;
- **inherited:** conservatively carried from surrounding or replaced content;
- **stale:** its source tokens changed and precision is no longer trustworthy;
- **unavailable:** no defensible mapping exists.

`exact` describes relationship preservation, not recognition correctness.
Estimated audio bounds use `inherited`; their origin may record the estimation
method. This avoids a separate quality label whose meaning would overlap.

Mappings may overlap, leave tokens unmapped, reference several tokens or runs,
or cover pseudo-tokens written by the user. Adjacent mappings need not share a
state. Alternatives apply only while their source token mapping is sufficiently
current; after a
conflicting edit they are hidden or marked stale rather than forced onto the new
text.

For a visible-token selection, the resolver:

1. intersects mappings by complete token identity for the current paragraph
   revision;
2. collects supported audio ranges and token/provenance references;
3. coalesces compatible adjacent ranges from the same source/channel;
4. assigns the least precise participating state to the result; and
5. reports gaps explicitly rather than inventing exact timing.

Mappings across older paragraph revisions are provenance, not current replay
authority. Implementations may transform unaffected mappings forward after an
edit but must update their revision and state.

## 6. Paragraph model

A paragraph is a visible editing and navigation unit, not an audio duration
promise. It may be backed by one chunk, multiple chunks, partially user-written
content, or multiple recognition revisions. Conversely, one recognition chunk
may contribute to multiple paragraphs.

Paragraph IDs should remain stable for text-only edits. Split and merge create
new paragraph identity as described below so references cannot silently change
meaning. Paragraph-level controls are UI concerns and are not serialized into
the content.

## 7. Recognition chunk model

Long audio is divided into ranges no longer than the recognizer's effective
maximum (approximately 30 seconds). Plans store explicit start/end times and any
intentional overlap. Boundaries may prefer pauses or paragraph evidence, but no
particular segmentation algorithm is required yet.

Overlap can improve boundary recognition but may produce duplicate text. The
reconciliation step must:

- compare overlapping token text and audio ranges;
- choose or align one visible sequence without deleting either run;
- record which run portions contributed to the visible result; and
- surface an issue when it cannot reconcile with sufficient confidence.

String equality alone is insufficient because repeated words can be legitimate.
The exact overlap algorithm and thresholds remain open.

Changing a paragraph boundary may create a new recognition plan whose chunks
better support the new structure. It never mutates old chunk bounds. A merged
paragraph longer than the limit remains backed by multiple chunks.

## 8. Editing model

Edits target complete visible tokens and produce a new paragraph revision.
Supported operations include insertion, deletion, replacement, alternative
selection, and structural split/merge. Typing and voice replacement both use
the same replacement primitive once replacement text is available. Replacement
text creates one or more indivisible pseudo-tokens; it never mutates a
recognition token.

On a token edit:

1. apply the edit to authoritative paragraph text;
2. record an undoable operation or before-state;
3. preserve exact/aligned mappings for retained token identities;
4. remove or mark stale mappings for replaced or deleted tokens;
5. give inserted pseudo-tokens `unavailable` alignment unless they have
   replacement audio or a later aligner establishes a mapping; and
6. recompute affected issues without changing visible text.

Selections may cross mappings and replay-chunk boundaries but always contain
complete tokens. A future multi-paragraph replacement can be represented as
paragraph operations plus token-range edits; v1 support for multi-paragraph
selection actions remains an implementation decision.

Choosing a recognizer alternative inserts ordinary visible text as one or more
pseudo-tokens. It retains a provenance reference to that alternative only when
useful; it does not mutate or masquerade as the original recognition token.

## 9. Voice replacement flow

1. Capture the selected paragraph ID, revision, and complete token identities.
2. Record a short correction as a distinct audio asset or ephemeral buffer.
3. Run replacement recognition, optionally with nearby visible text or original
   audio as context.
4. If the source selection has not changed, preview or apply recognized text
   according to the eventual UX decision.
5. Replace the selected visible tokens through the normal edit operation.
6. Map replacement pseudo-tokens to correction audio and its recognition run when
   retained; otherwise record available run provenance and degrade audio mapping
   according to retention policy.
7. If recognition fails, leave visible text unchanged and allow retry, typing,
   or cancellation.

If the source paragraph changed while recognition was pending, the system must
not apply the result blindly. It should ask the user to reselect or safely rebase
only when the original range can be identified without ambiguity.

Whether correction audio persists, whether a preview is required, and how
contextual decoding works are open. Privacy and export behavior must follow the
chosen retention policy.

## 10. Replay semantics

Replay is resolved from a current visible selection to audio; it is not direct
token playback.

### Quick replay

- Input: one visible token or an existing complete-token range from one
  intentional gesture.
- Resolution: use exact/aligned mappings when possible, then defensible
  inherited ranges.
- Context: expand the target by a short window whose fixed, pause-aware,
  sentence-aware, or hybrid algorithm remains open.
- Output: begin immediately when local audio is ready and highlight mapped
  current text during playback.
- UI: expose compact stop/status feedback without a media screen or waveform.

### Extended replay

Secondary actions can replay a selection, a paragraph, the last range again, or
at a slower rate. Paragraph replay may span and sequence several chunks without
presenting chunk transitions to the user.

If a selection has gaps, the resolver may use a broader paragraph-level range,
replay only defensibly mapped portions, or report that audio is unavailable. It
must communicate reduced precision and must not jump to unrelated audio. The
fallback order is an open product/technical decision.

## 11. Split and merge behavior

### Split

Splitting paragraph `P` at the boundary before visible token `T`:

1. creates two new ordered paragraphs from the complete tokens before `T` and
   the complete tokens from `T` onward;
2. tombstones or supersedes `P` while retaining it for undo/provenance;
3. partitions mappings by token identity;
4. divides mappings that cover tokens on both sides without splitting a token;
5. creates an undoable structural operation; and
6. may schedule a new recognition plan aligned to the new boundary.

### Merge

Merging adjacent paragraphs `A` and `B`:

1. creates one new paragraph with an explicit separator policy (normally a
   space, unless punctuation/whitespace already supplies one);
2. supersedes `A` and `B` while retaining their provenance for undo;
3. combines their mappings, creating an indivisible pseudo-token with
   unavailable alignment when a separator is needed;
4. preserves the ordered backing sources even when discontinuous;
5. creates an undoable structural operation; and
6. may schedule a new plan, still divided into legal chunks if combined audio
   exceeds approximately 30 seconds.

The merge separator policy and rules for discontinuous or overlapping backing
audio require prototyping. Neither operation directly edits historical runs.

## 12. Revision and provenance requirements

The initial model must support:

- undo of visible edits and paragraph split/merge for a defined session or
  persistence window;
- immutable recognition run output and retry history while referenced;
- attribution of current mappings to their recognition runs or user/replacement
  edits;
- a new plan when recognition boundaries change;
- recovery after a pending or failed run without losing visible text; and
- detection of stale asynchronous results by paragraph revision.

It need not preserve every intermediate confidence score, decoder state, or
recognizer invocation forever. Retention may compact unreferenced backing after
the undo window, provided visible text and required audio references survive.
Which provenance must survive export is open.

## 13. Serialization considerations

The persistent format should be versioned, deterministic enough for recovery,
and tolerant of unknown fields. A simple container may hold a human-readable
text representation plus sidecar metadata, or one structured document may hold
both. The smallest useful choice is unresolved.

Regardless of syntax:

- visible paragraph text and order must be recoverable without optional
  recognition metadata;
- unknown extension fields must be preserved when practical and never prevent
  reading known text;
- IDs and range-indexing rules must be explicit;
- audio may be embedded or referenced, but missing media must degrade safely;
- writes should be atomic or journaled enough to avoid losing authoritative
  text; and
- schema migration must not reinterpret visible text from recognition output.

If a textual source format is introduced for Neovim, Zed, or other desktop
editors, it must remain understandable and editable without LSP or Tree-sitter.
Language tooling may add semantic highlighting, hover details, issue
diagnostics, code actions, replay commands, navigation, folding, and paragraph
operations, but cannot replace the rendering/storage model or expose tokens as
the primary source.

## 14. Extensibility

Namespaced extension objects may attach small metadata records to documents,
paragraphs, runs, or mappings. Unknown extensions are optional and ignorable.
This is not a generic annotation-layer system.

The audio model may carry an optional channel identifier, but v1 assumes one
personal draft or mixed track. Speaker diarization, overlapping speech, and
multi-channel UI are out of scope. Future annotations should earn dedicated
typed fields through demonstrated product need rather than forcing v1 into a
universal schema.

## 15. Invariants

1. Visible paragraph text is authoritative after every user edit.
2. Recognition output is append-only evidence and is not destructively
   rewritten by editing or re-recognition.
3. Paragraphs are user-facing editing units; chunks are internal processing
   units. Neither permanently owns or equals the other.
4. Selections and text edits contain complete visible tokens. They may cross
   mappings, replay chunks, and, where supported, paragraph boundaries, but
   never split a token.
5. Replay is derived from available alignment and reports degraded precision;
   it does not require alignment to exist.
6. Any edit may invalidate precise alignment but must not invalidate visible
   text.
7. Split and merge may create new plans/runs and never rewrite processing
   history in place.
8. Async recognition results apply only against the expected visible revision
   or a demonstrably safe rebase.
9. Failed or missing optional recognition metadata cannot make the document
   unreadable or prevent ordinary text editing.
10. Unknown metadata cannot change interpretation of known visible text and
    should not make the document unreadable.
11. A chunk submitted to a recognizer must respect that recognizer's configured
    duration limit; one paragraph may use multiple chunks.
12. Undo restores both visible structure and the applicable mapping references
    without requiring mutation of recognition runs.

## 16. Error handling

- **Recognition failure:** retain existing visible text/backing, mark the run
  failed, create an issue if useful, and allow retry or manual correction.
- **Missing audio:** keep the document editable; disable replay with a clear
  local explanation rather than treating the document as corrupt.
- **Incomplete/stale mapping:** avoid false precision, use the selected fallback
  policy, and keep typing/deletion available.
- **Overlapping-chunk ambiguity:** preserve both runs, choose no silent
  destructive reconciliation, and surface an issue for the affected visible
  range.
- **Concurrent edit during async work:** reject, rebase safely, or present the
  result for explicit application; never overwrite newer user text.
- **Malformed optional metadata:** isolate and ignore the damaged optional
  record where possible while loading authoritative paragraphs.
- **Interrupted save:** recover the last complete visible revision through
  atomic replacement, journaling, or an equivalent storage guarantee.
- **Voice replacement failure/permission denial:** keep the selection and text,
  then offer retry, typing, or cancellation.

## 17. Open technical questions

- What is the smallest useful persistent format: readable primary text with a
  sidecar, a structured container, or another design?
- Which recognition provenance and correction audio must survive save, export,
  and undo compaction?
- What exact algorithm maps edited pseudo-tokens back to audio, especially
  after the user replaces their recognition-backed tokens?
- What replay fallback order is safest for incomplete alignment?
- How should quick-replay context be computed: fixed, pause-aware,
  sentence-aware, or hybrid?
- How are overlapping chunk results aligned and duplicate text reconciled?
- How are recognition plans scheduled after split, merge, replacement, and
  ordinary edits?
- When does background re-recognition run, and how are its suggestions presented
  without altering authoritative text?
- How should a merge preserve/replay discontinuous audio, and when should it
  trigger rechunking if the paragraph exceeds 30 seconds?
- What undo depth or retention window is required, and when may unreferenced runs
  be compacted?
- How are issue signals represented, ranked, dismissed, and invalidated across
  paragraph revisions?
- Should correction audio be persistent, encrypted, embedded, or ephemeral?
- Is multi-paragraph selection required in v1, and how should its mappings be
  transformed?
- Which parts of desktop integration belong in a textual format, LSP, or
  Tree-sitter without making any of them required for readability?

These are the unresolved technical decisions for the current specification.
The unresolved product decisions are maintained in Section 16 of the PRD so
that interaction choices do not become accidental technical requirements.
