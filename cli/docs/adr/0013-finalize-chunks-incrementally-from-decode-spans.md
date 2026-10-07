---
status: accepted
---

# Finalize chunks incrementally from decode spans

Initial transcription processes one active decode span at a time. It decodes
that span once, advances the forward-only paragraph-construction fold, chooses
a continuation boundary near the end, and finalizes zero or more chunks from
the accepted prefix. Finalized chunk ranges from the same audio source are
ordered and disjoint, although gaps are allowed.

The next decode span starts at the continuation boundary. Its submitted audio
may therefore overlap the previous span's unaccepted suffix, but that suffix is
only decoder evidence for the previous pass. After a pass, its decode span is
no longer active, while its evidence and produced content remain in the
project. Finalized chunk identities and ranges remain stable as required by
ADR-0008.

The exact continuation-boundary heuristic and the decoder prompt or context
policy are follow-up decisions. They must not change the rule that finalized
chunks have disjoint audio ranges.

This decision supersedes ADR-0003. It keeps Whisper evidence as the source of
boundary information and does not introduce Silero VAD as a second boundary
authority.
