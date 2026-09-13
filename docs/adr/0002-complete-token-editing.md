---
status: accepted
---

# Edit complete text units

Selection, editing, mapping, replay, and persistence operate on complete
addressable text units; character offsets and partial-unit positions are not
part of the document model. This keeps token-to-audio claims honest and makes
user-authored text indivisible when no defensible internal alignment exists.
When normal Whisper tokens are missing or do not exactly reproduce a chunk's
text, the complete chunk text becomes one such unit with unavailable alignment
while the mismatched transcription evidence is retained.
The current code calls these units visible tokens and pseudo-tokens; their
canonical names remain to be resolved by GitHub issue #57.
