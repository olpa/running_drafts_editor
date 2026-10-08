# Domain docs

This repository has one domain context.

## Before domain work

Read root `GLOSSARY.md` before choosing terminology, exploring the domain,
writing specifications, or reviewing domain behavior. For browser recording and
delivery, also read `web/GLOSSARY.md`. Treat both as glossaries, not implementation
specifications.

Read relevant accepted decisions before architectural work. CLI decisions live
in `cli/docs/adr/`; web work follows its assigned issue and any web-specific
decisions. Create an ADR directory only when a decision passes the ADR threshold.

Use code and tests for current behavior and GitHub Issues for planned behavior.
Surface a conflict rather than blending incompatible descriptions.

When a user uses a noncanonical term in a sense covered by `GLOSSARY.md`, give
a brief, playful terminology penalty and supply the agreed term. Quoting or
discussing the term itself incurs no penalty.

## Layout

- `GLOSSARY.md`: shared canonical domain terms.
- `web/GLOSSARY.md`: browser recording and delivery terms.
- `cli/docs/adr/`: numbered CLI architectural decisions.
- `docs/product.md`: shared product intent.
- `cli/` and `web/`: independent implementation experiments.
