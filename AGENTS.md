# Agent Context

## Mission

Build Running Drafts Editor: turn existing audio and an imperfect transcript
into usable text. The user edits text; audio and recognition data support replay
and correction.

Current phase: technical feasibility through a line-oriented CLI for a typical
dumb terminal. Implement the assigned ticket without expanding into a TUI or
production product.

## Read when needed

1. `doc/cli-mvp1.md` — ordered CLI work plan and current scope.
2. `doc/transcript-cleanup-ui-proposal-v0.1.md` — technical model and invariants.
3. `doc/thought-recovery-transcriber-prd-v0.1.md` — product intent and non-goals.

When documents disagree, preserve visible text and data recoverability, follow
the CLI plan for MVP scope, and surface the conflict instead of inventing a
large design.

## Durable constraints

- Visible paragraph text is authoritative.
- Paragraphs are editing units; recognition chunks are processing units.
- Chunks are at most about 30 seconds and may overlap.
- Recognition runs are immutable; retries and boundary changes create revisions.
- Selection uses visible character ranges, not token boundaries.
- The CLI renders selectable chunk-boundary symbols distinctly from paragraphs.
- Chunk symbols and recognition metadata are absent from clean text export.
- Edits may make alignment stale or unavailable; never claim false precision.
- Missing audio or optional metadata must not prevent reading and editing text.
- Async or refreshed recognition must not overwrite newer user edits.

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

## Keep this context current

Update `AGENTS.md` in the same change whenever implementation introduces a new
feature, durable constraint, architectural decision, command convention, or
important technical insight. Replace stale statements; keep the file brief and
broad. Do not turn it into a changelog or duplicate details available in the
source documents.
