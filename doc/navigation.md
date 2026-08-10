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
not selectable text. An edit may add or replace text with pseudo-tokens. A
pseudo-token preserves user-authored text and does not claim recognition
provenance.

Recognition tokens and pseudo-tokens are indivisible selection and editing
units. The user cannot select, delete, or replace only part of a token. An edit
replaces or inserts complete tokens.

Paragraph text is the exact concatenation of its visible-token text. Token text
may contain leading or trailing whitespace. Chunk markers contribute no text
and are absent from clean export.

Recognition-token identities refer to immutable evidence. Pseudo-tokens have
stable identities of their own. Displayed addresses follow current paragraph
and token order and are not stable identities.

## Addresses and commands

| Form | Meaning |
| --- | --- |
| `M` | paragraph `M` |
| `M.N` | visible token `T` in paragraph `M` |
| `M.N,M.U` | tokens `T` through `U`, inclusive when displayed |
| `M@N` | chunk-boundary marker `N` in paragraph `M` |
| `.` | current token, marker, or selection when permitted |

Numbers are positive and one-based. The common `M.N` form addresses tokens;
the distinct `M@N` form is reserved for less frequent chunk-marker operations. The general command form is:

```text
[address or range] command [arguments]
```

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

## Cursor and selection state

The cursor is a caret on a visible token or chunk marker. A selection is one
visible token, a non-empty range of complete visible tokens, one paragraph, or
one chunk marker. Multi-paragraph token ranges are not initially required.

Displayed ranges use inclusive endpoints for convenient commands. Internally,
a token range should be half-open. A stored selection records stable endpoint
identities and the paragraph revision, not only displayed token numbers.

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
