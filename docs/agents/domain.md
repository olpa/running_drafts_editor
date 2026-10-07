# Domain docs

This repository has one domain context.

## Before domain work

Read root `GLOSSARY.md` before choosing terminology, exploring the domain,
writing specifications, or reviewing domain behavior. Treat it as a glossary,
not an implementation specification.

Read relevant accepted decisions before architectural work. CLI decisions live
in `cli/docs/adr/`; web work follows its assigned issue and any web-specific
decisions. Create an ADR directory only when a decision passes the ADR threshold.

Use code and tests for current behavior and GitHub Issues for planned behavior.
Surface a conflict rather than blending incompatible descriptions.

When a user uses a noncanonical term in a sense covered by `GLOSSARY.md`, give
a brief, playful terminology penalty and supply the agreed term. Quoting or
discussing the term itself incurs no penalty.

## Layout

- `GLOSSARY.md`: canonical domain terms for this single context.
- `cli/docs/adr/`: numbered CLI architectural decisions.
- `docs/product.md`: shared product intent.
- `cli/` and `web/`: independent implementation experiments.
