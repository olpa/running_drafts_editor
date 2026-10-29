# Web Agent Context

This directory owns the browser seed experiment. Follow shared rules in the
root `AGENTS.md` and the assigned GitHub issue and its comments.

Read `../docs/product.md` for product intent, `../GLOSSARY.md` for shared
domain language, and `GLOSSARY.md` for browser recording and delivery terms.
CLI-specific architecture and terminal interaction rules are
documented under `../rust-poc/`; the web seed's scope comes from its own issues.

The first implementation was defined by editor issue #76. Issue #106 moved the
UI to Vue 3; `docs/adr/0001-vue-application-modules.md` records its architecture
and `docs/adr/` holds the other web decisions. Apply them only to `web/`.
Backend and inference implementation belong to `olpa/running_drafts_backend`.

Components consume application modules and never call HTTP or browser APIs
directly. Run `npm run check` and `npm test` from `web/` before finishing; the
browser acceptance suite is `e2e/` in `olpa/running_drafts_product`, which runs
this checkout through the product sandbox. Selectors that suite depends on
(`data-testid`, roles) are part of the contract: change them together with that
suite.

Keep application code, browser tests, and web developer instructions here.
