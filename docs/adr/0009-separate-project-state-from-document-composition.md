---
status: accepted
---

# Separate project state from document composition

`Document` owns document identity, paragraphs, authoritative visible text, and
stable chunk references, while `Project` owns the supporting audio,
transcription evidence, issues, attention marks, and one linear history. The
Rust boundary changes without nesting or reinterpreting the legacy
`rde-document/v1-experimental` fields. Keeping that historical schema name is
less risky than migrating recoverable history only to make the name match, and
a future storage-backend change can introduce a genuinely new schema.
