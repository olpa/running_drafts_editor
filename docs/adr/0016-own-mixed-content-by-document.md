---
status: accepted
---

# Own mixed content and standalone chunks by the document

The Project owns one Document whose ordered mixed content consists of
standalone Chunks and ParagraphBreaks. A Chunk contains its current
Transcription, audio reference, transcription-profile reference, predecessor
reference, and chunk-local annotations; paragraphs and displayed addresses are
derived by scanning the content. Decode spans may be exported as ignored
inspection evidence, but they do not own Chunks or recovery state. This keeps
the recovery format close to the visible document and makes each Chunk usable
without traversing decoder-process internals.

This supersedes ADR-0014. The rejected alternatives were decode-span-owned
mixed content, which made stable product entities subordinate to transient
processing details, and a separate Chunk collection plus paragraph-break set,
which made a sequential document require cross-file-style lookup.
