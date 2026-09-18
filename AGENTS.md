# Agent Context

## Mission

Build Running Drafts Editor: turn existing audio and an imperfect
transcription into usable text. The user edits text; audio and transcription
data support replay and correction.

The current product is a technical-feasibility, line-oriented CLI for a dumb
terminal. Keep work inside that boundary unless the assigned GitHub issue
changes it. The repository is one Rust package: `rde` is the executable and
`src/lib.rs` exposes reusable modules.

## Sources of truth

- Current behavior: code, tests, and `rde --help`.
- Planned behavior and acceptance criteria: the assigned GitHub issue and its
  comments.
- Product intent and interaction rationale: `docs/product.md`; read it before
  changing scope, workflow, replay, correction, export, or user-facing UX.
- Initial-transcription chunking: `docs/transcription-chunking.md`; read it
  before changing provisional chunks, boundary selection, overlap ownership,
  accepted-segment grouping, or pause handling.
- Domain language: `CONTEXT.md`; use it in code, tests, and issues.
- Durable architecture: read the relevant accepted record in `docs/adr/`
  before changing document authority or recovery, editing units, chunk boundary
  formation, audio coordinates, correction transactions, or decoder and prompt
  state.
- Finalized chunk identity and boundaries: [ADR-0008](docs/adr/0008-keep-finalized-chunks-stable.md).
  Editing may move complete chunks between paragraphs but does not split or join
  chunks.
- Project and document ownership: [ADR-0009](docs/adr/0009-separate-project-state-from-document-composition.md).
  Transcriptions belong to the project and each proposes text for one chunk.
  Earlier unreleased project formats are not supported.
- Attention-mark targets: [ADR-0010](docs/adr/0010-target-attention-marks-by-chunk-and-token.md).
  Store stable chunk and token identities and derive displayed addresses from
  the current document structure.
- Transcription settings and history: [ADR-0012](docs/adr/0012-history-restores-transcription-settings.md).
  Model and language changes produce another transcription for one current
  chunk in one transaction; undo and redo restore the selected settings too.

Surface conflicts between these sources. Preserve exact user-visible text and
reachable project data in the supported format while resolving them.

CLI addresses are one-based positions in document structure: `N` is before a
paragraph, `N.M` is before a chunk, and `N.M.K` is before a token in that
chunk. Ranges are half-open (`A,B`), and each nonempty container permits its
one-past-the-end position. Numeric addresses are displayed coordinates, never
stable identities.

## Working rules

- Keep each change limited to its issue and the smallest supporting work.
- Preserve reachable transcription evidence; users never edit it directly.
- Add focused tests for behavior and failure paths.
- Record a newly agreed domain term in `CONTEXT.md` immediately. Record an
  architectural decision only when it is hard to reverse, surprising without
  context, and chosen among real alternatives.
- Keep plans and specifications in GitHub Issues. Keep implementation details
  in code, tests, and CLI help. Use a pointed technical note only when a
  cross-cutting procedure or rationale is costly to reconstruct from them.
- Use clear B2-level English in project text.
- Leave recordings, model binaries, credentials, generated output, and editor
  swap files untracked.

## Agent skills

### Issue tracker

Issues and specifications live in GitHub Issues. Read
`docs/agents/issue-tracker.md` before issue operations.

### Triage labels

Triage uses five canonical roles. Read `docs/agents/triage-labels.md` before
triaging issues.

### Domain docs

This is a single-context repository. Read `docs/agents/domain.md` before domain
exploration or architectural work.
