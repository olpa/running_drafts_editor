---
status: accepted
---

# Store document content inside decode spans

The project owns its ordered decode spans. Each decode span stores the decoder
evidence and ordered mixed content produced while processing that span. A
chunk belongs to the decode span that finalized it and owns its immutable
transcriptions; separate current project state selects the transcription shown
for that chunk.

The document is a projection made by flattening decode-span content in original
recording order. It does not persist a second ordered list of chunk IDs.
Paragraphs may cross decode-span boundaries. `Chunk` and `ParagraphBreak` are
the currently agreed content items, and paragraph breaks have no separate
stable identity.

The ownership direction is final, but the complete set of content-item variants
and which existing project elements move into mixed content are still open.
This decision does not require validation of paragraph-break arrangement.
Editing and history rules for mutable mixed content require a follow-up
decision.

This decision supersedes ADR-0009. Transcription evidence remains project-owned,
and visible text still comes from each chunk's selected transcription.
