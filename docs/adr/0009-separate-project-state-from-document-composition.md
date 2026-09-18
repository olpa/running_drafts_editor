---
status: accepted
---

# Separate project state from document composition

`Document` owns document identity, paragraphs, stable chunk references, and
the current text/token projection, while `Project` owns immutable one-chunk
transcriptions, supporting audio and evidence, issues, attention marks, selected
transcription settings, and one linear history. Composition projections must
match their selected transcriptions rather than becoming another editable text
authority.

Issue #57 dropped compatibility with earlier unreleased formats. The simple
`rde-project/v1-experimental` marker identifies the supported representation;
embedded transcriptions do not have separate schema versions.
