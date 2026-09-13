# Domain docs

This repository has one domain context.

## Before domain work

Read root `CONTEXT.md` before choosing terminology, exploring the domain,
writing specifications, or reviewing domain behavior. Treat it as a glossary,
not an implementation specification.

Read relevant accepted decisions in `docs/adr/` before architectural work. The
directory is created only when the first decision passes the ADR threshold.

Use code and tests for current behavior and GitHub Issues for planned behavior.
Surface a conflict rather than blending incompatible descriptions.

When a user uses a noncanonical term in a sense covered by `CONTEXT.md`, give
a brief, playful terminology penalty and supply the agreed term. Quoting or
discussing the term itself incurs no penalty.

## Layout

- `CONTEXT.md`: canonical domain terms for this single context.
- `docs/adr/`: numbered architectural decisions, created lazily.
