# Running Drafts Editor

Shared language for turning recorded speech into editable text while retaining
the audio and transcription data needed for correction.

## Working material

**Project**:
The complete working set for editing one document, including its audio sources,
chunks, transcriptions, user actions, issues, attention marks, and history.
_Avoid_: Document as a name for the complete working set.

**Document**:
The editable composition that arranges chunks into paragraphs. Its text comes
from each chunk's current transcription.
_Avoid_: Draft, project, visible document.

**Recording**:
The complete original audio opened for transcription and editing.

**Audio source**:
An identifiable source from which a chunk receives its audio, together with the
metadata needed to locate or verify it.

**Provisional chunk**:
A candidate portion of audio whose boundaries are still being determined
during initial transcription. Provisional chunks may overlap.
_Avoid_: Processing window, submitted window, transcription window.

**Chunk**:
A finalized, disjoint portion of audio together with its metadata, user actions,
and transcriptions. Its identity and boundaries remain stable after initial
transcription.
_Avoid_: Provisional chunk, replay unit, processing window.

**Chunk ID**:
The stable, opaque identity assigned when a chunk is finalized.
_Avoid_: Chunk address, chunk ordinal, audio range.

**has_tokens**:
A chunk property that is true when its current transcription exposes at least
one addressable text token. Special tokens do not count; a text token with empty
token text does.

**Paragraph**:
An ordered group of one or more complete chunks presented as one block of text.
Paragraph boundaries occur only between chunks.

## Transcription

**Transcription**:
An immutable proposed rendering of one chunk's speech, together with its
supporting data and transcription circumstances.
_Avoid_: Hypothesis, recognition, retranscription, transcription iteration.

**Initial transcription**:
The process that turns a recording into finalized chunks and produces each
chunk's first transcription.

**Transcription circumstances**:
The audio, model, software version, language, settings, text context, and edit
instructions used to produce a transcription.

**Current transcription**:
The transcription selected as a chunk's current state.

**Latest transcription**:
The most recently produced transcription of a chunk, which may differ from its
current transcription after undo.

**Orphaned transcription**:
A transcription that no reachable project history state can restore through
undo or redo.

**Current text**:
The text supplied by a chunk's current transcription.

**Transcription cleanup**:
The user's job of turning an imperfect transcription into usable text through
user actions and further transcriptions.

**Edit**:
A user-authored text change supplied when producing another transcription.

**User action**:
An action initiated by the user that may contribute to another transcription
or change project state.

## Transcription data

**Token**:
A Whisper vocabulary item with a token ID and token text.
_Avoid_: Word, pseudo-token.

**Text token**:
A token whose token text contributes to a transcription's text.

**Special token**:
A token carrying transcription control or metadata rather than text, such as a
timestamp.

**Token ID**:
The identifier of a token in Whisper's vocabulary. The same ID may occur more
than once in a transcription.

**Token text**:
The text associated with a Whisper token.

**Token alternative**:
A candidate token at one token position in a transcription, with its own token
ID, text, and probability.

**Token probability**:
The numeric probability supplied by Whisper for a token at one position in a
transcription.

**Confidence**:
A UI interpretation of token probability, such as a color derived from a live
threshold.

## Review

**Issue**:
A range of the current transcription identified for possible review, not a
confirmed error or proposed correction.
_Avoid_: Warning, error, suggestion.

**Attention mark**:
A user-created marker before a token indicating that nearby text should be
reviewed later.
_Avoid_: Issue, flag.

## Interaction

**Position**:
A structural place before an item or one past the final item in a container.
_Avoid_: Insertion point, caret.

**Range**:
A half-open span of document content bounded by two positions. It contains its
start, excludes its end, and never contains part of a token.

**Selection**:
The range currently selected in the editor.

**Current position**:
The position defined by a selection whose endpoints are equal.
_Avoid_: Caret.

**Address**:
Displayed notation referring to a position in the current document structure.
An address is derived and may change when the document changes.
