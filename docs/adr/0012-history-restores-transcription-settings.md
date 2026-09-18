---
status: accepted
---

# Restore transcription settings with project history

Model and language changes produce another transcription for exactly one
current chunk and commit the selected settings with that completed transcription
in one project-history transaction. Failure leaves both unchanged; undo and redo
restore the settings with the transcription, rather than keeping independent
session defaults that could silently differ from the restored state.

History restores model references without loading or running the model. The
decoder is loaded only when another transcription is needed, so recovery and
reading do not depend on an available model or recording. There is no standalone
command to run the same transcription again.
