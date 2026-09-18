---
status: accepted
---

# Keep finalized chunks stable

Finalized chunk identity and boundaries remain fixed after initial
transcription, so editing may only rearrange complete chunks through paragraph
splitting and merging. The CLI does not define chunk split or join commands,
and reusable document APIs do not expose those operations, so callers cannot
derive new chunks with uncertain audio ownership.

## Unreleased formats

Issue #57 explicitly dropped support for earlier unreleased project formats.
The supported format does not carry derived chunks or boundary-changing history.
Within that format, save/reopen and undo/redo preserve fixed chunk identities
and boundaries and all reachable transcription evidence.
