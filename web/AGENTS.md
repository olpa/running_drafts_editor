# Web Agent Context

This directory owns the browser seed experiment. Follow shared rules in the
root `AGENTS.md` and the assigned GitHub issue and its comments.

Read `../docs/product.md` for product intent, `../GLOSSARY.md` for shared
domain language, and `GLOSSARY.md` for browser recording and delivery terms.
CLI-specific architecture and terminal interaction rules are
documented under `../cli/`; the web seed's scope comes from its own issues.

The first implementation is defined by editor issue #76: a Vite/TypeScript
application and Python/Playwright walking skeleton with mocked services.
Backend and inference implementation belong to `olpa/running_drafts_backend`.

Keep application code, browser tests, and web developer instructions here.
