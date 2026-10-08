# Running Drafts Editor

Shared language for turning recorded speech into editable text while retaining
the audio and transcription data needed for correction.

## Language

### Working material

**Project**:
The complete working set for editing one document, including its recording,
audio sources, chunks, transcriptions, user actions, issues, attention marks,
and history.
_Avoid_: Document as a name for the complete working set.

**Document**:
The editable composition stored as ordered mixed content of chunks and
paragraph breaks. Its visible text comes from each chunk's current
transcription, while paragraphs and displayed addresses are derived.
Supporting audio, transcription evidence, review state, and history belong to
the project rather than the document.
_Avoid_: Draft, project, visible document.

**Recording**:
The complete original audio opened for transcription and editing.

**Recording ID**:
The stable, opaque identity of a recording, independent of its storage location.
It is distinct from the chunk IDs assigned during initial transcription.

**Audio source**:
An identifiable source from which a chunk receives its audio, together with the
metadata needed to locate or verify it. A recording is the usual audio source;
later user-supplied audio may provide another one.
_Avoid_: Recording as a synonym when discussing source identity.

**Transport frame**:
An ordered piece of browser-captured audio, identified by its recording ID
and an increasing sequence number, that the browser delivers to the backend.
It carries no transcription meaning; its boundaries are independent of decode
spans and chunks, and decoding may depend on earlier transport frames.
_Avoid_: Frame (alone), chunk, segment, slice, blob.

**Decode span**:
Audio processed in one step of initial transcription, together with its
decoder evidence. Decode spans may overlap and may produce zero or more
finalized chunks, but they do not own those chunks.
_Avoid_: Provisional chunk, processing window, submitted window,
transcription window.

**Continuation boundary**:
A suitable place near the end of a decode span where the next decode span
starts.
_Avoid_: Good break point, handoff boundary, core boundary, cursor.

**Chunk**:
A finalized, disjoint portion of input audio, limited to about 30 seconds,
together with the references and current state needed to replay or transcribe
it. Its identity and boundaries remain stable; its Document owns it directly.
_Avoid_: Decode span, provisional chunk, replay unit, processing window.

**Chunk ID**:
The stable, opaque identity assigned when a chunk is finalized. It remains
unchanged after editing, producing another transcription, or moving the chunk
between paragraphs, and normally remains hidden from the user.
_Avoid_: Chunk address, chunk ordinal, audio range.

**has_tokens**:
A chunk property that is true exactly when its current transcription exposes at
least one addressable token. Control tokens do not count, while an addressable
token with empty token text does.

**Paragraph**:
An ordered group of one or more complete chunks presented as one block of text.
Paragraph boundaries occur only between chunks.

**Paragraph break**:
A content item indicating that following content starts a new paragraph. It
has no stable identity.
_Avoid_: Paragraph marker, paragraph ID.

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

**Whisper segment**:
A timestamp-delimited group that the Whisper transcription API constructs from
the decoder's token sequence. Timestamp tokens delimit the group; it is neither
a sentence nor a finalized chunk.
_Avoid_: Chunk, sentence.

**Transcription circumstances**:
The audio, model, software version, language, settings, transcription prompt,
and edit instructions used to produce a transcription. Hardware and runtime
details are optional diagnostics.

**Transcription profile**:
An immutable, identifiable selection of model, language, and other settings
used to produce a transcription or selected for the next transcription.

**Transcription prompt**:
The ordered sequence of compatible text-token IDs supplied before decoding to
condition a transcription. It contains zero or more transcription prompt
tokens.
_Avoid_: Text context, recognition prompt.

**Forced prefix**:
Text-token IDs that the decoder must emit to apply a user edit while producing
another transcription. It is distinct from a transcription prompt, which only
conditions the decoder.
_Avoid_: Transcription prompt, prompt.

**Current transcription**:
The completed transcription selected as a chunk's current state.

**Latest transcription**:
The most recently produced transcription of a chunk, which may differ from its
current transcription after undo.

**Orphaned transcription**:
A transcription that no reachable project history state can restore through
undo or redo.

**Current text**:
The text supplied by a chunk's current transcription, whether or not it exposes
addressable Whisper tokens.

**Transcription cleanup**:
The user's job of turning an imperfect transcription into usable text through
user actions and further transcriptions. It does not name an individual
operation or transcription.

**Edit**:
A user-authored text change supplied when producing another transcription.

**User action**:
An action initiated by the user, such as supplying an edit or changing the
language or model, that contributes to another transcription.

### Transcription data

**Token**:
A Whisper token identified by its vocabulary token ID and token text.
_Avoid_: Word, pseudo-token as synonyms for token.

**Text token**:
A token whose token text contributes to a transcription's text. Text tokens may
appear in a document and receive user-facing addresses; a text token may have
empty token text.

**Control token**:
A token that controls decoding or carries decoder metadata rather than
contributing to transcription text. Language, task, previous-text, start, end,
and timestamp tokens are control tokens.
_Avoid_: Special token except when referring to a decoder API.

**Timestamp token**:
A control token representing a decoder-relative audio position. Whisper uses
timestamp tokens to delimit Whisper segments.
_Avoid_: Time token, time stamp token.

**Transcription prompt token**:
One occurrence of a compatible text-token ID in a transcription prompt. It does
not contribute to the resulting transcription unless the decoder emits it
again.
_Avoid_: Recognition prompt token.

**Token ID**:
The identifier of a token in Whisper's vocabulary. The same ID may occur more
than once in a transcription.

**Token text**:
The text associated with a Whisper token.

**Token alternative**:
A candidate token at one token position in a transcription, with its own token
ID, text, and probability. A token position may have several alternatives.

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
A chunk-owned annotation before a current token, indicating that nearby text
should be reviewed later. Moving the chunk between paragraphs does not change
the mark; replacing the targeted token removes it, while undo may restore both.
It is distinct from a system-generated issue.
_Avoid_: Issue, flag.

### Interaction

**Position**:
A structural place immediately before a paragraph, chunk, or token, or one past
the final item in a container. Positions at different structural depths may
refer to the same place.
_Avoid_: Insertion point, caret.

**Range**:
A half-open span of document content bounded by two positions. It contains
complete structural items rather than part of a token; equal endpoints define
an empty range. Use document range or audio range when the kind is ambiguous.

**Selection**:
The range currently selected in the editor.

**Current position**:
The position at both ends of a zero-length selection.
_Avoid_: Caret.

**Address**:
Displayed notation referring to a position in the current document structure.
It is derived from that structure rather than serving as a stable identity.
