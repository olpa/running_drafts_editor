---
status: accepted
---

# Keep document text authoritative

The current transcription's text is authoritative for document rendering,
export, and recovery. Audio and token data are supporting evidence: they may
replace current text only through a successfully installed transcription and
must never silently overwrite newer user work.

Issue #57 stores exact text in one immutable transcription per chunk rather
than creating independent text units. Reading and export remain available when
audio, a model, or addressable token data is unavailable. Correction requires
the decoder's audio and model; if they are unavailable, it fails without
changing text or history instead of creating non-Whisper tokens.
