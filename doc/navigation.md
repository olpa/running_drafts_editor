# CLI Navigation and Selection

This document defines navigation and selection for the line-oriented CLI. The
command language uses ideas from `ed`: an optional address or range comes
before a command. It is not modal and does not reproduce `ed` exactly.

The CLI is a technical-feasibility shell for a dumb terminal. Normal output
stays readable as prose. Token details appear only when requested.

## Visible sequence

A paragraph contains an ordered sequence of visible tokens and selectable
chunk-boundary markers.

An initial visible token refers to an accepted normal text token in immutable
Whisper evidence. Timestamp and other special tokens remain evidence but are
not selectable text. Normal token text is concatenated exactly without trimming.
An edit may add or replace text with pseudo-tokens. A pseudo-token preserves
visible text and does not claim recognition provenance.

Recognition tokens and pseudo-tokens are indivisible selection and editing
units. The user cannot select, delete, or replace only part of a token. An edit
replaces or inserts complete tokens.

Paragraph text is the exact concatenation of its visible-token text. Token text
may contain leading or trailing whitespace. Chunk markers contribute no text
and are absent from clean export.

Recognition-token identities use the recognition run ID, accepted segment ID,
and original token position. Pseudo-tokens have stable identities of their own.
Displayed addresses follow current paragraph and token order and are not stable
identities.

If accepted normal tokens are missing or their exact concatenation differs from
a replay chunk's authoritative text, the complete chunk text becomes one
indivisible pseudo-token. Its alignment is unavailable. The CLI reports this
fallback and retains the mismatched recognition evidence; it never invents
partial-token positions to repair the difference.

## Addresses and commands

| Form | Meaning |
| --- | --- |
| `M` | paragraph `M` |
| `M.N` | visible token `N` in paragraph `M` |
| `M.N,M.U` | tokens `N` through `U`, inclusive when displayed |
| `M@N` | chunk-boundary marker `N` in paragraph `M` |
| `.` | current token, marker, or selection when permitted |

Numbers are positive and one-based. The common `M.N` form addresses tokens;
the distinct `M@N` form is reserved for less frequent chunk-marker operations.
The general command form is:

```text
[address or range] command [arguments]
```

The address and command may be attached (`2@3info`) or separated by whitespace
(`2@3 info`). Commands include `print`/`p`, `play`, `info`/`i`, `select`/`s`,
`tokens`, `help`/`h`, and `quit`/`q`; `list`/`l` remain print aliases. `play` and
`info` require a chunk-marker address. `print` accepts no address for the whole
document or a paragraph address. `tokens` requires a paragraph address. A bare
token or marker address moves the caret.

Examples describe the intended language; each ticket implements only commands
within its scope:

```text
p                     print the document
2p                    print paragraph 2
2.4                   move the caret to token 2.4
2.4select             select token 2.4
2.4,2.9select         select complete tokens 2.4 through 2.9
2select               select paragraph 2
2@3select             select chunk marker 2@3
2@3info               show information for marker 2@3
2@3play               play the chunk ending at marker 2@3
play                  play the current token or selection
2tokens               inspect paragraph 2 tokens
h                     show help
q                     quit
```

### Marker playback

A chunk marker is the right boundary of a replay chunk. `M@Nplay` plays the
complete replay chunk immediately to the left of marker `M@N`: from marker
`M@(N-1)` to `M@N`. For `M@1`, the left boundary is the start of paragraph `M`.
Every paragraph starts at a replay-chunk boundary, so playback never needs to
split a chunk or cross a paragraph boundary.

The implementation follows the marker's stable chunk reference and plays that
chunk's stored audio range. It does not derive audio positions by subtracting
marker numbers, and it adds no playback context. Context around a cursor or
selection belongs to the later replay ticket. If the marker or its backing
chunk is unavailable, the command reports an error instead of guessing a
range.

## Cursor and selection state

The cursor is a caret on a visible token or chunk marker. A selection is one
visible token, a non-empty range of complete visible tokens, one paragraph, or
one chunk marker. Token ranges may cross paragraph boundaries.

Displayed and stored token ranges use inclusive endpoint identities. Each
endpoint records its stable token identity, paragraph identity, and paragraph
revision rather than only its displayed token number.

After an edit, a selection may remain only when the same token identities map
safely into the new paragraph revision. Otherwise the CLI must use a documented
safe caret or report a stale selection. It must not silently select different
tokens that inherited the same ordinals.

## Normal rendering

Normal rendering concatenates token text without adding separators. It shows
prose and chunk markers instead of every token boundary. Display-only delimiters
make state visible without depending on terminal styling:

```text
2  I think ⟪quite strongly that this works⟫ now. ⟦2@1⟧
2  I think ‹quite› strongly that this works now. ⟦2@1⟧
2  I think this works. ⟪⟦2@1⟧⟫ Then continue. ⟦2@2⟧
```

Here `⟪...⟫` is a selection, `‹...›` is the token holding the caret, and
`⟦M@N⟧` is a chunk marker. These symbols never become text or appear in clean
export. ANSI underline or reverse video may supplement them, but meaning must
not depend on color or control sequences.

## Token inspection

The canonical token view uses one token per line with quoted, escaped text:

```text
rde> 2tokens
2.1  rec     "I"
2.2  rec     " think"
2.3  rec     " quite"
2.4  rec     " strongly"
2.5  pseudo  " that this works"
2.6  rec     " now"
2.7  rec     "."
2@1  marker  chunk boundary
```

Quoting exposes leading whitespace. Tabs, newlines, control characters, quotes,
and backslashes are escaped. `rec` and `pseudo` distinguish recognition-backed
and user-authored tokens without adding metadata to normal prose.

An optional compact view may show adjacent token cells and complete selections:

```text
[I][ think]⟪[ quite][ strongly][ that this works]⟫[ now][.]
```

The vertical form is the baseline because it remains clear with Unicode,
whitespace, long tokens, narrow terminals, and redirected output.

## Edits and pseudo-tokens

Recognition evidence is immutable. Replacing selected recognition tokens
removes their references from the visible sequence and inserts one or more
pseudo-tokens. The original recognition tokens remain as evidence.

A pseudo-token is indivisible after creation. To change it, the user selects
and replaces the complete pseudo-token, just as with a recognition token. A
replacement command may deliberately create several pseudo-tokens, but no
operation implicitly splits an existing token.

Character offsets and character spans do not exist in the document,
selection, editing, mapping, replay, or persistence model. Token text is opaque
to those operations. Rendering and clean export concatenate complete token text
without creating addressable positions inside it.
