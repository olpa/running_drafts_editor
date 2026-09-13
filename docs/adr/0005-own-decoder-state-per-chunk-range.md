---
status: accepted
---

# Own decoder state per chunk range

A transcription session shares one model context, but each cached exact chunk
audio range owns its own Whisper state and encoder/decoder caches. Those caches
depend on both the audio and decoder history, so sharing them between chunks
would mix incompatible state. Reusing the cache for the same range avoids
encoding its audio again while preserving this isolation.
