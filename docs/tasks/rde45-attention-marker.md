# RDE #45: Mark a position for correction after export

Implement GitHub issue #45 as specified in the ticket:

https://github.com/olpa/running_drafts_editor/issues/45

Let a user flag a transcript position without correcting it in RDE. The flag is
part of a one-way handoff: RDE shows it while editing and emits it in plain-text
export so an external tool knows to inspect the nearby transcript. There is no
round trip or import contract.

## Product semantics

- An attention mark identifies a position immediately before one visible token.
  It does not assert that only that token is wrong; the external tool is expected
  to inspect the nearby text and context.
- Use `⚑` (U+2691 BLACK FLAG) as the fixed visible and exported symbol.
- The mark is document metadata anchored to the stable identity of the token on
  its right. It is not token text, a pseudo-token, a replay-chunk marker, or a
  recognition-derived issue.
- At most one attention mark may be attached to a token. A document may contain
  any number of marked positions.
- The exported artifact is disposable. Do not add exported IDs, a sidecar,
  machine-readable metadata, or support for importing corrected output.

## Commands

- Add `[M.N]mark` to mark the position immediately before token `M.N`.
- Add `[M.N]unmark` to remove that mark.
- Without an address, `mark` and `unmark` use a token caret or a single-token
  selection. For a multi-token selection, they use the first selected token in
  document order and report its current address.
- Reject a missing current token, paragraph selection, chunk-marker caret or
  selection, stale selection, and any other non-token target with a clear error.
- `mark` reports an error when the target is already marked. `unmark` reports an
  error when it is not marked. Neither case creates history.
- Successful `mark` and `unmark` operations are document edits. They persist,
  clear redo history, and participate in `undo`, `redo`, `Nundo`, and `Nredo`.

## Rendering and export

- Render `⚑` immediately before the complete text of the anchored token. Do not
  inspect, split, trim, or relocate whitespace within the token.
- On a color-capable interactive terminal, render the symbol red and then reset
  terminal color without affecting token confidence, caret, or selection style.
- Redirected and non-color display emits the same literal symbol without ANSI
  controls. The mark must remain visible without color.
- Plain-text export emits the literal symbol at the same position. It adds no
  extra whitespace and otherwise preserves exact paragraph text and paragraph
  separators.
- The mark is intentional handoff content, not a recognition internal. Chunk
  markers, confidence presentation, and recognition metadata remain absent from
  export.

## Editing and structure

- Inserting or appending other tokens does not move or remove a mark anchored to
  an existing token.
- Replacing, deleting, or installing new recognition truth over the anchored
  token removes its mark. This treats correction of the flagged location as
  resolution. Undo restores both the prior token and its mark.
- Paragraph and replay-chunk split or merge operations retain marks because the
  anchored visible-token identities survive those operations.
- Loading a document resets navigation as usual but retains all persisted marks.
  Missing audio or recognition metadata does not affect them.

## Persistence

- Extend the experimental document format with typed attention-mark state that
  refers to stable visible-token identities. Do not encode the symbol into
  authoritative token text.
- Validate that every loaded mark refers to a current visible token and that no
  token is marked more than once. Report invalid data through the document-load
  error path; do not silently reinterpret it as visible text.
- Preserve marks in saved undo/redo snapshots or events so history remains
  restorable after reopening the document.

## Completion

- Add focused tests for addressed and current-target commands, selection-start
  behavior, invalid targets, duplicate operations, rendering with and without
  color, exact export placement, multiple marks, persistence, undo/redo, edits
  that remove marks, structural edits that retain them, and malformed saved
  mark references.
- Update command help, export documentation, the persistence schema description,
  and relevant project documentation. Update `AGENTS.md` with the durable marker
  and export rules without turning it into a changelog.
- Run the standard Rust formatting, test, and lint checks and leave the worktree
  ready for review.
