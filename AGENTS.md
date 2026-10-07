# Repository Agent Context

## Mission and layout

Build Running Drafts Editor: turn recorded speech into usable text, with audio
and transcription data supporting replay and correction.

- `cli/` is the independent Rust CLI experiment. Read `cli/AGENTS.md` before
  working there. Run Cargo commands from `cli/`.
- `web/` is the browser seed experiment. Read `web/AGENTS.md` before working
  there. Its implementation is defined by the assigned GitHub issue.
- `GLOSSARY.md` defines shared domain language.
- `docs/product.md` records shared product intent.
- `docs/agents/` contains shared agent procedures.

CLI architectural decisions live in `cli/docs/adr/`. Apply them to CLI work;
check the assigned issue before applying them to the web seed.

## Working rules

- Read the assigned issue and its comments for scope and acceptance criteria.
  Current behavior is defined by code, tests, and CLI help.
- Read `docs/product.md` before changing scope or user-facing workflow.
- Keep each change limited to its issue and the smallest supporting work.
- Surface conflicts between sources rather than blending incompatible rules.
- Preserve exact user-visible text and reachable project data in the supported
  format. Users never edit transcription evidence directly.
- Add focused tests for behavior and failure paths.
- Record a newly agreed domain term in `GLOSSARY.md` immediately. Record an
  architectural decision only when it is hard to reverse, surprising without
  context, and chosen among real alternatives.
- Keep plans and specifications in GitHub Issues. Keep implementation details
  in code, tests, and help. Use a pointed technical note only when a cross-cutting
  procedure or rationale is costly to reconstruct from them.
- Use clear B2-level English in project text.
- Leave recordings, model binaries, credentials, generated output, and editor
  swap files untracked. Small test fixtures under `web/e2e/fixtures/` are the
  exception.
- Keep the CLI and web experiments independent. Create shared code only when
  a concrete requirement calls for it.

## Agent skills

- Read `docs/agents/issue-tracker.md` before issue operations.
- Read `docs/agents/triage-labels.md` before triaging issues.
- Read `docs/agents/domain.md` before domain exploration or architectural work.
