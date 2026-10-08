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

**Secured audio**:
The portion of captured audio acknowledged as durably retained by the server,
independently of transcription progress.
_Avoid_: Uploaded audio as proof of durable retention, transcribed audio.

**Unsecured audio**:
The portion of captured audio whose durable retention by the server has not been
acknowledged, including audio still being finalized or awaiting an upload
acknowledgement.
_Avoid_: Lost audio, untranscribed audio.
