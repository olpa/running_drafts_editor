---
status: accepted
---

# Reuse Whisper token IDs as prompts

Initial transcription passes normal text-token IDs from the last accepted
segment directly to the next Whisper window. Converting them to text and
tokenizing again could change the exact sequence; timestamp and other special
tokens are excluded because they belong to the earlier window's control and
time context. Direct reuse preserves linguistic continuity without carrying
invalid window-local state forward.
