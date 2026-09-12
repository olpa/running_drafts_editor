# Domain docs

This repository uses one domain context for its single Rust package.

## Before exploring

Read root `CONTEXT.md` when present and relevant decisions in `docs/adr/`.
These files are created lazily by domain-modeling work; proceed when absent.

Read the existing project documents for the area being explored:

- [CLI work plan](../cli-mvp1.md): MVP scope and implementation order.
- [Navigation and selection](../navigation.md): addresses, tokens, and replay.
- [Recognition chunking](../chunking.md): audio terms and recognition boundaries.
- [Technical model](../transcript-cleanup-ui-proposal-v0.1.md): editing and mapping invariants.
- [Product intent](../thought-recovery-transcriber-prd-v0.1.md): goals and non-goals.

## Layout

- `CONTEXT.md`: shared domain terms at the repository root.
- `docs/`: project plans and technical reference documents.
- `docs/agents/`: skill configuration and document reading rules.
- `docs/tasks/`: saved issue-specific agent briefs, indexed in `docs/tasks/README.md`.
- `docs/adr/`: numbered architecture decision records.

Use glossary terms consistently in tickets, proposals, code, and tests.
Record unresolved terminology for domain-modeling work. Surface conflicts with
existing ADRs explicitly. Follow the document precedence rules in `AGENTS.md`.
