---
status: accepted
---

# Install corrections as completed transcriptions

A correction is a decoder instruction that produces another immutable
transcription, rather than a direct durable mutation of document text. Only a
completed transcription is installed, keeping the previous project state
intact on failure.

## Consequences

The current implementation synchronously transcribes one complete chunk with a
forced prefix and may revise surrounding tokens. Deletion remains separate
because a forced prefix cannot express which spoken audio must be removed.
