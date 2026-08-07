# Recognition chunking: technical context

## Terms

**Source audio** is the complete recording being recognized. It is decoded as
mono 16 kHz audio.

**Sample** is one audio value. Sample offsets are the exact internal time unit;
at 16 kHz, 16,000 samples equal one second.

**Cursor** is the first source sample not yet owned by a completed processing
step. It moves forward after every recognition call.

**Window** or **submitted window** is the audio range sent to Whisper in one
call. It can contain audio before and after the range that this call is expected
to own, and is never longer than 30 seconds.

**Core** is the non-overlapping source range assigned to one processing window.
It starts at the cursor and ends at the selected boundary. Consecutive cores cover
the source without gaps or overlap.

**Target core** is the preferred core length, currently 24 seconds. It guides
boundary selection but is not necessarily the final core length.

**Context** is audio submitted outside the intended core so Whisper can decode
speech near a boundary with surrounding sound. Neighboring windows therefore
overlap even though their cores do not.

**Segment** is a timestamped piece of decoded text returned by Whisper for one
window. A segment is a recognition hypothesis, not an editing paragraph.

**Accepted segment** is a segment selected as the initial text result for a
core. Other segments from the same window are retained as evidence even when
they are not accepted.

**Boundary** is the source position where one core ends and the next begins. The
implementation normally chooses a Whisper segment end timestamp near the target
core end.

**Prompt** is the bounded tail of previously accepted text supplied to the next
Whisper call for linguistic context. It does not change ownership of audio.

**Recognition run** is the immutable record of all windows, hypotheses,
accepted segments, prompts, boundaries, and failures produced by one execution.

**Chunk** is the user-facing replay unit listed by `chunk audition`. In the
current implementation each listed chunk corresponds to one accepted segment.
It is distinct from a processing window and from a future editable paragraph.

**Fragment** has no precise meaning in the current data model. Use it only
informally for an unspecified piece of audio or text; use window, core, segment,
or chunk when one of those meanings is intended.

## Recognition-driven chunking

Chunking is performed while Whisper recognizes the audio. There is no separate
pass that decides all boundaries in advance.

The implementation keeps a cursor at the first unowned sample. For every step
it submits an overlapping Whisper window containing:

- up to 3 seconds before the cursor;
- a target core of 24 seconds starting at the cursor;
- up to 3 seconds after the target core.

The submitted window is therefore at most Whisper's 30-second input limit.
Whisper returns decoded segments with timestamps. Their ranges are translated
from window-relative timestamps to absolute source sample positions.

The next boundary is the end timestamp of the decoded segment closest to the
24-second target. A timestamp must be after the current cursor and inside the
submitted window. If two candidates are equally close, the later one wins. The
last window always ends at the end of the source.

Only segments whose midpoint is at or after the current cursor and whose end is
at or before the chosen boundary are accepted for that chunk. This is the
initial overlap-deduplication rule; all window hypotheses are still retained as
recognition evidence.

The accepted text is carried into the next recognition call as context, limited
to its last 1,000 characters. If recognition fails or yields no usable
timestamp, the cursor advances by 10 seconds so processing remains bounded and
eventually covers the complete source.

## Earlier experiment

We tried Silero VAD as a separate pre-recognition chunk planner. On representative
sample data it assigned very low speech probabilities to clearly audible,
continuously recognized speech and therefore proposed misleading boundaries.
We removed that implementation and decided to derive chunks simultaneously
with recognition, using Whisper timestamps and decoded text as the evidence.
