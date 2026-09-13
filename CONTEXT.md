# Running Drafts Editor

Shared language for turning recorded speech into editable text while retaining
the audio and transcription data needed for correction.

## Language

### Working material

**Project**:
The complete working set for editing one document, including its recording,
audio sources, chunks, transcriptions, user actions, issues, attention marks,
and history. A project owns one linear undo and redo history.
_Avoid_: Document as a name for the complete working set.

**Document**:
The editable composition that arranges chunks into paragraphs. Its visible text
comes from each chunk's current transcription.
_Avoid_: Draft, project, visible document.

**Recording**:
The complete original audio opened for transcription and editing.

**Audio source**:
An identifiable source from which a chunk receives its audio, together with the
metadata needed to locate or verify it. A recording is the usual audio source;
later user-supplied audio may provide another one.
_Avoid_: Recording as a synonym when discussing source identity.

**Provisional chunk**:
A candidate portion of audio whose boundaries are still being determined
during initial transcription. Provisional chunks may overlap; finalized chunks
may not.
_Avoid_: Processing window, submitted window, transcription window.

**Chunk**:
A finalized, disjoint portion of input audio, limited to about 30 seconds,
together with its metadata, edits, and transcriptions. Its identity and
boundaries remain stable while overlapping provisional transcription data
remains available as evidence.
_Avoid_: Provisional chunk, replay unit, processing window.

**Chunk ID**:
The stable, opaque identity assigned when a chunk is finalized. It remains
unchanged after editing, producing another transcription, or moving the chunk
between paragraphs, and normally remains hidden from the user.
_Avoid_: Chunk address, chunk ordinal, audio range.

**has_tokens**:
A chunk property that is true exactly when its current transcription exposes at
least one addressable token. Special tokens do not count; an addressable token
with empty token text does, while a false value leaves the chunk's
structural boundaries and playback intact.

**Paragraph**:
An ordered group of one or more complete chunks presented as one block of text.
Paragraph boundaries occur only between chunks, and a paragraph remains valid
and playable when none of its chunks has addressable tokens.

### Transcription

**Transcription**:
An immutable proposed rendering of one chunk's speech, together with its
supporting data and transcription circumstances. A later transcription records
the transcription that was current when its user actions were made; the
transition between them has no separate domain term.
_Avoid_: Hypothesis, recognition as a synonym for transcription,
retranscription, transcription iteration.

**Initial transcription**:
The process that turns a recording into finalized chunks, assigns their chunk
IDs, and produces each chunk's first transcription.

**Transcription circumstances**:
The audio, model, software version, language, settings, text context, and edit
instructions used to produce a transcription. Hardware and runtime details are
optional diagnostics.

**Current transcription**:
The completed transcription selected as a chunk's current state. Undo may make
an earlier transcription current while a later one remains available to redo.

**Latest transcription**:
The most recently produced transcription of a chunk, which may differ from its
current transcription after undo.

**Orphaned transcription**:
A transcription that no reachable project history state can restore through
undo or redo. It and any user actions referenced only by orphaned
transcriptions may be deleted.

**Current text**:
The text supplied by a chunk's current transcription.

**Transcription cleanup**:
The user's job of turning an imperfect transcription into usable text through
user actions and further transcriptions. It does not name an individual
operation or transcription.

**Edit**:
A user-authored text change supplied when producing another transcription.

**User action**:
An action initiated by the user, such as editing text, changing the language or
model, or requesting another transcription. Several user actions may contribute
to one later transcription.

### Transcription data

**Token**:
A Whisper token identified by its vocabulary token ID and token text.
_Avoid_: Word, pseudo-token as synonyms for token.

**Text token**:
A token whose token text contributes to a transcription's text. Text tokens may
appear in a document and receive user-facing addresses.

**Special token**:
A token carrying transcription control or metadata rather than text, such as a
timestamp. It remains transcription data and does not contribute to current
text.

**Token ID**:
The identifier of a token in Whisper's vocabulary. The same ID may occur more
than once in a transcription.

**Token text**:
The text associated with a Whisper token.

**Token alternative**:
A candidate token at one token position in a transcription, with its own token
ID, text, and probability. Choosing one is an edit that may contribute to
another transcription.

**Token probability**:
The numeric probability supplied by Whisper for a token at one position in a
transcription. It is transcription data, not a guarantee of correctness.

**Confidence**:
A UI interpretation of token probability, such as a color derived from a live
threshold. It is distinct from the probability stored in a transcription.

### Review

**Issue**:
A range of the current transcription identified for possible review, not a
confirmed error or proposed correction.
_Avoid_: Review item, warning, error, suggestion.

**Attention mark**:
A user-created marker before a token indicating that nearby text should be
reviewed later. It is distinct from a system-generated issue.
_Avoid_: Issue, flag.

### Interaction

**Position**:
A structural place immediately before a paragraph, chunk, or token, or one past
the final item in a container. Positions at different structural depths may
refer to the same place.
_Avoid_: Insertion point, caret.

**Range**:
A half-open span of document content bounded by two positions that never
contains part of a token; use document range or audio range when the kind is
ambiguous. A chunk remains inside the range between its
structural boundaries even without addressable tokens; only equal endpoints
make a range empty.

**Selection**:
The range currently selected in the editor.

**Current position**:
The position at both ends of a zero-length selection, used for an action without
an explicit address.
_Avoid_: Caret.

**Address**:
Displayed notation referring to a position in the current document structure.
Its structural depth identifies a paragraph, chunk, or token while different
depths may name the same range endpoint; an address is derived and may change
when the document changes.
