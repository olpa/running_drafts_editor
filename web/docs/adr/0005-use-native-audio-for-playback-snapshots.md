---
status: accepted
---

# Use native audio for immutable Playback Snapshots

Play each activated Section's immutable Playback Snapshot through a native
`HTMLAudioElement`. Let the browser own byte-range selection, buffering,
seeking, cancellation, and private caching; keep the application responsible
only for resolving the snapshot URL, long-recording navigation controls, and
safe local feedback. This pairs the editor with the ordinary-media contract in
product ADR 0010 without materializing media in the browser or introducing a
custom streaming pipeline.

Playback state belongs to one mounted Section and is independent of Capture and
other Section players. A native media failure may hide an HTTP cause, so the
application makes one diagnostic one-byte request to the same snapshot and
discards its body. Original-audio download remains a separate representation.

## Considered alternatives

- Application `fetch` plus Blob URLs would require assembling media before
  playback and would duplicate browser range and cache behavior.
- Media Source Extensions would add a custom buffering and seeking pipeline
  without a product requirement that native playback cannot satisfy.
- One global player would couple independent Sections and make mounted-page
  state a cross-application policy.
