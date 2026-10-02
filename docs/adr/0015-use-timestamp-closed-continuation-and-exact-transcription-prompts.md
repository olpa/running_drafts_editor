---
status: accepted
---

# Use timestamp-closed continuation and exact transcription prompts

Initial transcription derives continuation candidates only from the longest
ordered prefix of Whisper segments whose raw timestamps form valid, advancing,
non-overlapping ranges in the submitted decode span. It finalizes the gap-free
sub-prefix through the chosen candidate, or uses a bounded progress fallback;
word alignment, probability, and a second voice-activity authority do not
divide or silently discard decoded text. Malformed or unlocated decoder output
remains project-owned evidence but does not become document text, a
transcription prompt, or audio ownership.

Each decode receives a snapshot of the best compatible transcription prompt
known when it starts. The prompt is the explicitly truncated newest suffix of
exact accepted text-token IDs; it excludes control tokens and is never
reconstructed by rendering and retokenizing decoder output. Token-ID
compatibility is guaranteed by using the same loaded decoder throughout one
initial run. Its limit, omitted-token count, and exact supplied IDs remain
inspectable. A transcription prompt is separate from a forced prefix used to
apply a user edit, and later changes do not rewrite the circumstances of an
earlier transcription. Cross-model prompt compatibility belongs to later
correction work.

This completes the continuation-boundary and prompt decisions left open by
ADR-0013. Exact search durations and pause thresholds remain replaceable
quality parameters rather than architectural commitments.
