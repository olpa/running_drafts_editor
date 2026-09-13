---
status: accepted
---

# Derive chunk boundaries during transcription

Initial transcription uses overlapping provisional audio to find boundaries
from Whisper timestamps, then produces finalized disjoint chunks while
retaining the overlapping evidence. A separate Silero VAD planning experiment
assigned low speech probability to clearly audible quiet speech, so it was
removed as the authority for boundaries. This avoids a second planner whose
decisions can disagree with the transcription evidence. The
[current procedure and experiment history](../transcription-chunking.md) are
recorded separately.
