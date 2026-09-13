---
status: accepted
---

# Keep finalized chunks stable

Finalized chunk identity and boundaries remain fixed after initial
transcription, so editing may only rearrange complete chunks through paragraph
splitting and merging. The CLI does not define chunk split or join commands,
and reusable document APIs do not expose those operations, so callers cannot
derive new chunks with uncertain audio ownership.

## Legacy history

Projects saved by older versions may contain derived chunk records and
undo/redo states that change chunk boundaries. Loading, saving, undoing, and
redoing those states remains supported so their text and transcription evidence
stay reachable; restoring a recorded historical state does not authorize a new
chunk mutation. Silently dropping those states would make recovery less honest
than preserving the historical structure.
