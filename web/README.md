# Browser seed experiment

This directory is the home for the Running Drafts browser seed. It will test
browser recording and progressive transcription through the backend and private
inference service.

[Issue #76](https://github.com/olpa/running_drafts_editor/issues/76) establishes
the Vite/TypeScript application and Python/Playwright walking skeleton with
mocked services. No application or build tooling is scaffolded here yet.

[Product issue #1](https://github.com/olpa/running_drafts_product/issues/1)
coordinates the seed across repositories. Backend and inference implementation
belong to `olpa/running_drafts_backend`.

Shared [product intent](../docs/product.md) and [domain language](../GLOSSARY.md)
live at the repository root. The independent [Rust CLI](../cli/README.md) is
another technical experiment for the same product.
