---
status: accepted
---

# Target attention marks by chunk and token identity

The project owns attention marks, and each mark targets a stable chunk ID and
token identity rather than a displayed address. This makes paragraph moves
irrelevant to the mark, prevents a new transcription from inheriting a mark
only because similar text occupies the same position, and lets undo restore
the mark with the exact chunk transcription state in which it was created.

Legacy marks that store only a token identity gain their chunk target from the
current or historical document state that contains them when the project is
loaded. A mark whose target cannot be resolved is invalid rather than silently
retargeted.
