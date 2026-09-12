# RDE #16: Navigate and dismiss issues

Implement GitHub issue #16 as specified in the ticket:

https://github.com/olpa/running_drafts_editor/issues/16

Add live confidence highlighting, issue navigation, issue listing, and undoable
resolve/reopen behavior to the shared line-oriented editing session.

## Required behavior

- Classify current visible recognition tokens from the accepted Whisper token
  probability. Use red below `0.15`, orange from `0.15` below `0.50`, and no
  confidence color from `0.50` upward.
- Do not color pseudo-tokens or tokens without an accepted-token probability.
- Support live session-only configuration with `issue-prob`, `issue-prob red
  VALUE`, and `issue-prob orange VALUE`, including clear validation errors.
- Treat each maximal red-token sequence as an open issue. Paragraph and replay-
  chunk boundaries split issues. Orange tokens are not issues.
- Implement `next` and `prev` with selection, document-order navigation,
  wrapping, and the edge behavior defined in the ticket.
- Implement `issues`, `ignore`, `Nignore`, and `Nunignore`. The listing must
  include temporary numbers, open/resolved state, and escaped current text.
- Persist resolved token ranges. Resolved tokens have no confidence color and
  split surrounding open issues. Resolve and reopen participate fully in saved
  undo/redo history.
- Invalidate a resolution when one of its tokens is edited; undo must restore
  the complete prior state.
- Keep caret and selection presentation clear when confidence colors are
  present. Non-color and redirected operation must remain usable, and clean
  text must not gain issue metadata or terminal controls.
- Do not add processing-failure issues in this task.

## Completion

- Add focused tests for classification boundaries, chunk/paragraph issue
  boundaries, navigation and wrapping, listing, resolution lifecycle,
  persistence, undo/redo, edits that invalidate resolutions, threshold changes,
  invalid commands, and non-color output.
- Update user-facing help and the relevant project documentation. Update
  `AGENTS.md` with the durable command and issue-model decisions without turning
  it into a changelog.
- Run the standard Rust formatting, test, and lint checks and leave the worktree
  ready for review.
