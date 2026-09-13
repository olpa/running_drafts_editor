---
status: accepted
---

# Derive chunk boundaries during transcription

Initial transcription uses overlapping provisional audio so speech at an input
edge keeps enough context, then derives boundaries from Whisper timestamps and
retains the overlapping evidence. Finalized chunks must have disjoint audio
ownership. A separate Silero VAD planning experiment assigned low speech
probability to clearly audible quiet speech, so it was removed as the authority
for boundaries; this avoids a second planner whose decisions can disagree with
the transcription evidence. The
[current procedure and experiment history](../transcription-chunking.md) are
recorded separately.
