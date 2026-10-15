# Running Drafts Editor

Running Drafts Editor turns recorded speech into usable text, with audio and
transcription data supporting replay and correction.

This repository contains a product CLI and two experiments:

- [`cli/`](cli/README.md): the product-level Python client for Projects and
  existing-file audio upload through the backend API.
- [`rust-poc/`](rust-poc/README.md): the Rust CLI proof of concept for
  transcription, editing, replay, and recovery.
- [`web/`](web/README.md): browser recording and delivery with a mocked backend.
  Real backend integration and progressive transcription are the next step.

## Product CLI development

```sh
cd cli
uv sync --frozen
uv run pytest
uv run rde --help
```

See the [product CLI README](cli/README.md) for installation, configuration,
and usage.

## Rust proof-of-concept development

Run Cargo commands from the CLI directory:

```sh
cd rust-poc
cargo build
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo run -- --help
```

See the [Rust proof-of-concept README](rust-poc/README.md) for build requirements
and usage.

## Shared project information

- [`docs/product.md`](docs/product.md) records product intent.
- [`GLOSSARY.md`](GLOSSARY.md) defines the shared domain language.
- [`web/GLOSSARY.md`](web/GLOSSARY.md) defines browser recording and delivery terms.
- [`rust-poc/docs/adr/`](rust-poc/docs/adr/) records CLI architectural decisions.
- [Editor issues](https://github.com/olpa/running_drafts_editor/issues) contain
  implementation plans and acceptance criteria.
- [Product seed coordination](https://github.com/olpa/running_drafts_product/issues/1)
  tracks the browser-to-inference experiment across repositories.
- [`AGENTS.md`](AGENTS.md) contains shared coding-agent instructions.

This repository is licensed under GPL-3.0-or-later. Dependencies retain their
own licenses.
