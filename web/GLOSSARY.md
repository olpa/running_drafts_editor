# Browser recording

Language for browser recording, audio delivery, and recording feedback.
The [shared glossary](../GLOSSARY.md) defines recording, recording ID,
transport frame, decode span, and chunk; those definitions also apply here.

## Language

**Capture**:
Acquiring audio from the microphone for a recording.
_Avoid_: Recording when referring only to microphone activity.

**Captured audio**:
Audio already acquired from the microphone for a recording.
Different portions can be secured or unsecured while capture continues or after
it stops.

**Capture finalization**:
Collecting the remaining captured audio after capture stops.
It is complete when all captured audio is available for delivery, even if some
audio remains unsecured.
_Avoid_: Recording completion, transcription completion.

**Capture interruption**:
An unexpected end to one recorder run because its delivery backlog reached the
limit or microphone capture failed. It describes browser capture only; it does
not claim that the backend accepted an interruption command or sealed the Audio
Asset.
_Avoid_: Interrupted asset unless backend sealing is confirmed.

**Secured audio**:
The portion of captured audio acknowledged as durably retained by the server,
independently of transcription progress.
_Avoid_: Uploaded audio as proof of durable retention, transcribed audio.

**Unsecured audio**:
The portion of captured audio whose durable retention by the server has not been
acknowledged, including audio still being finalized or awaiting an upload
acknowledgement.
_Avoid_: Lost audio, untranscribed audio.

**Delivery recovery**:
User-initiated delivery of retained unsecured audio after automatic attempts for
that Section have stopped. Recovery preserves its Audio Asset identity, frame
sequence, timing, and bytes, and remains independent of later recorder runs.
_Avoid_: Resume recording, reopen asset.
