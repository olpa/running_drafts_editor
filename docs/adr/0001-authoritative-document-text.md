---
status: accepted
---

# Keep document text authoritative

The document's current visible text is authoritative for rendering, export,
and recovery. Audio and transcription data are supporting evidence: they may
replace current text only through a successful installed operation and must
never silently overwrite newer user work. A project therefore remains readable
and editable when audio or optional transcription data is missing, and storage
must preserve exact visible text rather than reconstruct it from that data.
