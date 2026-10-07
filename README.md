# Running Drafts Editor

Running Drafts Editor turns recorded speech into usable text, with audio and
transcription data supporting replay and correction.

This repository contains two experiments:

- [`cli/`](cli/README.md): the existing Rust CLI for transcription, editing,
  replay, and recovery. It is an independent Cargo package.
- [`web/`](web/README.md): the home for the browser seed experiment. The seed
  will validate browser recording and progressive transcription.

## CLI development

Run Cargo commands from the CLI directory:

```sh
cd cli
cargo build
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo run -- --help
```

See the [CLI README](cli/README.md) for build requirements and usage.

## Shared project information

- [`docs/product.md`](docs/product.md) records product intent.
- [`GLOSSARY.md`](GLOSSARY.md) defines the shared domain language.
- [`cli/docs/adr/`](cli/docs/adr/) records CLI architectural decisions.
- [Editor issues](https://github.com/olpa/running_drafts_editor/issues) contain
  implementation plans and acceptance criteria.
- [Product seed coordination](https://github.com/olpa/running_drafts_product/issues/1)
  tracks the browser-to-inference experiment across repositories.
- [`AGENTS.md`](AGENTS.md) contains shared coding-agent instructions.

This repository is licensed under GPL-3.0-or-later. Dependencies retain their
own licenses.
