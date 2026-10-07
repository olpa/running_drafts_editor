---
status: accepted
---

# Target attention marks by chunk and token identity

The project owns attention marks, and each mark targets a stable chunk ID and
token identity rather than a displayed address. This makes paragraph moves
irrelevant to the mark, prevents a new transcription from inheriting a mark
only because similar text occupies the same position, and lets undo restore
the mark with the exact chunk transcription state in which it was created.

Every stored mark includes both targets. A mark whose target cannot be resolved
in its current or historical state is invalid rather than silently retargeted.
Issue #57 dropped the earlier unreleased format and its mark migration.
