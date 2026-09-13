---
status: accepted
---

# Make corrections produce another transcription

A visible-token correction synchronously transcribes its one complete chunk
with the correction supplied as a forced decoder prefix, then atomically
installs the completed transcription. Failure leaves the project unchanged;
successful decoding may also revise surrounding tokens. Typed correction text
is therefore an instruction to the decoder rather than durable replacement
text, while deletion remains separate because a forced prefix cannot express
which spoken audio must be removed.
