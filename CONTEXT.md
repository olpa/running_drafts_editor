# Running Drafts Editor

Shared language for turning recorded speech into editable text.

## Language

**Project**:
The complete working set needed to edit one document, including its recording,
audio sources, chunks, transcriptions, user actions, issues, attention marks,
and history.
_Avoid_: Document as a synonym for the complete working set.

**Document**:
The editable composition that arranges chunks into paragraphs. Its visible text
comes from each chunk's current transcription.
_Avoid_: Draft, project, visible document.

**Recording**:
The complete original audio that the user opens for transcription and editing.

**Audio source**:
An identifiable source from which a chunk receives its audio, together with the
metadata needed to locate or verify that audio. A recording is the usual audio
source; later user-supplied audio may provide another one.
_Avoid_: Recording as a synonym when discussing source identity.

**Chunk**:
A disjoint portion of input audio, limited to about 30 seconds, together with
its metadata, edits, and transcriptions; transcribing that portion again
preserves the chunk's identity. It is a finalized provisional chunk whose
boundaries stay fixed during editing, with overlapping transcription data
retained as evidence. Its `has_tokens` property may be false, including between
chunks for which it is true.
_Avoid_: Processing window, submitted window, replay unit as synonyms for chunk.

**has_tokens**:
A chunk property that is true exactly when the chunk exposes at least one
addressable token from its current transcription. Special tokens do not make it
true; an addressable token with empty token text does. The property follows the
current transcription rather than describing whether the chunk's rendered text
is empty.

**Chunk ID**:
The stable, opaque identity assigned as a side effect of producing a finalized
chunk. It remains unchanged when another transcription is produced, when the
chunk is edited, or when it is moved between paragraphs, and normally remains
hidden from the user.
_Avoid_: Chunk address, chunk ordinal, audio range.

**Provisional chunk**:
A candidate audio portion used during initial transcription whose boundaries
are not yet settled. Provisional chunks may overlap to help find better
boundaries; once finalized, they become disjoint chunks.
_Avoid_: Transcription window, processing window, submitted window.

**Paragraph**:
An ordered group of one or more complete chunks, presented together as a block
of text with its chunk boundaries visible. Paragraph breaks occur only between
chunks. A paragraph remains valid and playable when all its chunks have
`has_tokens` set to false.

**Transcription**:
A proposed rendering of a chunk's speech as text, together with the supporting
data and transcription circumstances. One or more user changes can lead from
one transcription to another; this transition has no separate domain term.
Each later transcription records the transcription that was current when those
changes were made. The project has one linear undo and redo history rather than
supported branches. Transcribing after every change is a UI policy.
_Avoid_: Hypothesis, recognition as a synonym for transcription,
retranscription, transcription iteration.

**Transcription cleanup**:
The user's job of turning an imperfect transcription into usable text through
user actions and further transcriptions. It does not name an individual
operation or transcription.

**Initial transcription**:
The process that turns a recording into finalized chunks, assigns their chunk
IDs, and produces each chunk's first transcription.

**Transcription circumstances**:
The submitted audio, model and software version, language and settings, supplied
text context, and edit instructions used to produce a transcription. Hardware
and runtime details are optional diagnostics.

**Edit**:
A user-authored change to text, supplied when producing a subsequent transcription.

**User action**:
An action initiated by the user, such as editing text, changing the language or
model, or requesting another transcription. Several changes may be collected
before producing a new transcription. An action used only by orphaned
transcriptions may also be deleted; an action referenced by reachable history
remains stored. If producing the transcription fails, its collected actions are
discarded rather than kept in a separate state, and the existing redo history
remains available.

**Current transcription**:
The transcription selected as the chunk's current state. Undo may return to an
earlier transcription while later transcriptions remain available to redo. Only
a completed transcription can become current; an operational failure leaves it
unchanged, preserves redo history, and discards the user actions supplied for
that operation.

**Orphaned transcription**:
A transcription that no reachable project history state can restore through
undo or redo. A new user action after undo may orphan transcriptions from the
cleared redo history; orphaned transcriptions and their otherwise-unused user
actions may be deleted.

**Latest transcription**:
The most recently produced transcription of a chunk, which may differ from its
current transcription after undo.

**Current text**:
The text supplied by the chunk's current transcription.

**Token**:
A Whisper token, identified by its token ID and associated token text.
_Avoid_: Word, pseudo-token as synonyms for token.

**Text token**:
A token whose token text contributes to a transcription's text. Text tokens can
appear in a document and receive user-facing addresses.

**Special token**:
A token carrying transcription control or metadata, such as a timestamp. It
remains transcription data and does not contribute to current text.

**Token ID**:
The identifier of a token in Whisper's vocabulary; the same ID can occur more
than once in a transcription.

**Token text**:
The text associated with a Whisper token.

**Token alternative**:
A candidate token at a particular token position in a transcription, with its
own vocabulary token ID, text, and probability. A token can have several
alternatives; choosing one is a user edit that can lead to a new transcription.

**Token probability**:
The numeric probability supplied by Whisper for a token at a particular
position in a transcription. It is transcription data, not a guarantee of
correctness.

**Confidence**:
A UI interpretation of token probability, such as a low-confidence color based
on a threshold. It is distinct from the probability stored with a transcription.

**Issue**:
A range of the current transcription that the system identifies for possible
review. An issue indicates uncertainty or another reason for attention, not a
confirmed error or a proposed correction.
_Avoid_: Review item, warning, error, suggestion.

**Attention mark**:
A user-created marker placed before a token to indicate that the nearby text
should be reviewed later. It is distinct from a system-generated issue.
_Avoid_: Issue, flag.

**Range**:
A half-open span of document content bounded by two positions: it includes its
start and excludes its end. A range around a chunk for which `has_tokens` is
false contains that chunk and is structurally nonempty even though it contains
no addressable tokens. A range is empty only when its endpoints are the same
position, and it never contains part of a token. Use document range or audio
range when the kind is ambiguous.

**Selection**:
The range currently selected in the editor.

**Position**:
A structural place immediately before a paragraph, chunk, or token, or one past
the final item in a container. Addresses at different structural depths may
refer to the same position.
_Avoid_: Insertion point, caret.

**Current position**:
The position at both ends of a zero-length selection, used for an action without
an explicit address.
_Avoid_: Caret.

**Address**:
The displayed notation that refers to a position in the current document
structure. When used as a range endpoint, a paragraph or chunk address refers
to that item's structural start. An existing deeper start-position address
refers to the same position. A chunk for which `has_tokens` is false retains its
structural boundaries but has no token-level address. For an attached command,
address depth may select the layer on which the command acts. An address is
derived and may change when the document changes.
