---
status: accepted
---

# Edit complete tokens or chunks

Token positions address complete Whisper text tokens; character offsets and
partial-token positions are not part of the document model. When text tokens
are missing or do not exactly reproduce the current transcription's text,
the chunk exposes only structural positions while retaining its text and
evidence; complete-chunk correction remains possible without inventing tokens.
Issue #57 replaced the earlier complete-text-unit representation with this
strict token model.
