---
status: accepted
---

# Store canonical audio coordinates

Audio and transcription positions use sample offsets in canonical mono 16 kHz
audio rather than offsets in the source file. This gives every supported WAV
encoding and channel layout one stable coordinate system. Token mappings carry
explicit precision such as exact, aligned, inherited, stale, or unavailable so
replay can degrade honestly instead of inventing timing accuracy. Stable source
and transcription identities exclude local paths and nondeterministic runtime
diagnostics.
