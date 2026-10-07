---
status: accepted
---

# Restore transcription settings with project history

Model and language changes select an immutable transcription profile and
produce another transcription for exactly one current chunk. The completed
transcription, selected profile, and active settings are committed in one
project-history transaction. Failure leaves them unchanged; undo and redo
restore them together rather than keeping independent session defaults that
could silently differ from the restored state.

History restores model references without loading or running the model. The
decoder is loaded only when another transcription is needed, so recovery and
reading do not depend on an available model or recording. There is no standalone
command to run the same transcription again.
