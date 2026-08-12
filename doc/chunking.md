# Recognition chunking: technical context

## Terms

**Source WAV** is the complete input file. Its original bytes are hashed before
decoding so the source remains identifiable.

**Canonical audio** is the decoded mono 16 kHz floating-point audio used by
recognition. All stored sample positions refer to this form, not to sample
positions in the source WAV.

**Sample** is one audio value. Sample offsets are the exact internal time unit;
at 16 kHz, 16,000 samples equal one second.

**Cursor** is the first source sample not yet owned by a completed processing
step. It moves forward after every recognition call.

**Window** or **submitted window** is the audio range sent to Whisper in one
call. It can contain audio before and after the range that this call is expected
to own, and is never longer than 30 seconds.

**Core** is the non-overlapping source range assigned to one processing window.
It starts at the cursor and ends at the selected boundary. Consecutive cores
cover the source without gaps or overlap.

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

**Prompt** is the sequence of normal text tokens from the previous accepted
segment, supplied to the next Whisper call for linguistic context. Timestamp
and other special tokens are excluded. A prompt does not change ownership of
audio.

**Recognition run** is the immutable record of all windows, hypotheses,
accepted segments, prompts, boundaries, and failures produced by one execution.

**Chunk** is the user-facing replay unit listed by `rde audition`. In the
current implementation it groups one or more whole accepted segments according
to pause length and token count. It is distinct from a processing window.

**Paragraph** is a readable block made from one or more complete replay chunks.
Every paragraph boundary is also a replay-chunk boundary. A chunk cannot belong
to more than one paragraph.

**Fragment** has no precise meaning in the current data model. Use it only
informally for an unspecified piece of audio or text; use window, core, segment,
or chunk when one of those meanings is intended.

## WAV decoding and conversion

The CLI accepts uncompressed WAV files with 8-, 16-, 24-, or 32-bit integer PCM
samples, or 32-bit floating-point PCM samples. The input may use any positive
sample rate and one or more channels. Compressed audio formats are not decoded.

Integer samples are normalized to the floating-point range from -1 to 1.
Floating-point samples must already be finite and inside that range. For
multi-channel input, the tool averages the channels in each audio frame. It
then converts the mono audio to 16 kHz with linear interpolation. Recognition,
chunk boundaries, and stored audio ranges use the converted samples.

The source identity keeps the SHA-256 hash of the original WAV bytes. Canonical
source facts record one channel, a 16 kHz sample rate, and the converted sample
count. Playback converts canonical sample positions to seconds and plays those
times from the original WAV.

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

The last 3 seconds form the boundary search area. The implementation uses the
latest Whisper segment-end timestamp between 24 and 27 seconds after the
cursor. Timestamps before the 24-second target are ignored. If the search area
has no usable timestamp, the boundary stays at the 24-second target. The last
window always ends at the end of the source.

Only segments whose midpoint is at or after the current cursor and whose end is
at or before the chosen boundary are accepted for that chunk. This is the
initial overlap-deduplication rule; all window hypotheses are still retained as
recognition evidence.

If recognition fails, the boundary also stays at the 24-second target. Normal
cores are therefore between 24 and 27 seconds; only the final core can be
shorter.

## Recognition

The tool uses Whisper through the vendored `whisper-rs` and `whisper.cpp`
sources. The user supplies a Whisper model file. The tool hashes the model and
stores the hash with the recognition result, so results from different models
can be distinguished.

Whisper receives mono 16 kHz floating-point audio. Each window described above
is recognized separately with beam search. The current beam size is 5.
Recognition keeps the source language and does not translate it. The language
can be set on the command line or left as `auto`.

Whisper's automatic context between calls is disabled because the tool manages
the windows. Instead, normal text-token IDs from the last accepted segment are
passed directly as the prompt for the next window. Timestamp tokens are
relative to their old window, and control tokens belong to the old recognition
call, so neither type is copied into the next prompt. The tool does not convert
the remaining tokens to text and tokenize the text again. Direct reuse
preserves the exact text-token sequence and helps names, spelling, and sentence
flow remain consistent across boundaries.

Timestamp output is enabled for both segments and tokens. For every segment,
the tool stores:

- the decoded text;
- its audio range;
- its no-speech probability;
- its tokens.

For every token, the tool stores its text, probability, optional audio range,
whether it is special, and alternative token candidates. The default limit is
20 alternatives. Whisper reports time in centiseconds. The tool converts these
values to absolute mono 16 kHz sample positions before storing them. Special
tokens remain in this evidence even though they are removed from prompts.

All segment hypotheses from every submitted window are kept. Accepted segments
are the input to post-recognition chunking, but they do not replace or delete
the other hypotheses. If one window fails, its error is recorded and
recognition continues with the next core. A run is marked as successful,
partial, or failed according to its window results.

Each recognition run is immutable and has an identity based on the source,
recognizer, model, settings, windows, and accepted segments. Running recognition
again creates another result instead of changing the old evidence.

## Post-recognition chunking

Recognition windows are technical units. After recognition, the tool groups
accepted Whisper segments into larger chunks for reading and replay. It keeps
each Whisper segment whole, so it does not cut inside a word or invent a more
precise audio boundary.

The initial settings are:

- minimum size: 8 normal text tokens;
- target size: 32 normal text tokens;
- maximum size: 64 normal text tokens;
- usable pause: 300 ms;
- strong pause: 800 ms;
- long pause: 2,000 ms.

Only normal text tokens count toward size. Timestamp and control tokens remain
recognition evidence but do not affect the count.

A pause is the gap between the end of one accepted Whisper segment and the
start of the next. Overlapping segments have a pause of zero. The rules are
applied from left to right:

1. A long pause always ends the current chunk, even when it is short.
2. A strong pause ends a chunk that has at least the minimum token count.
3. Usable pauses are candidates for a boundary near the target token count.
4. At the maximum token count, the best earlier pause is used. If there is no
   pause candidate, the tool uses the nearest whole-segment boundary.
5. The last accepted segment ends the final chunk.

When several usable pauses are candidates, the tool scores each one as:

```text
pause milliseconds - 20 × distance from the 32-token target
```

The higher score wins. A long-pause boundary must not be removed later merely
to make a short chunk larger.

Each chunk stores its source segment IDs, text, audio range, normal text-token
count, boundary reason, and pause length when available. Chunk text preserves
the exact accepted segment text without trimming so its normal tokens can remain
indivisible visible units. Boundary reasons are
`long_pause`, `strong_pause`, `scored_pause`, `maximum_tokens`, and
`source_end`.

## Paragraphs and visible chunk boundaries

The initial document joins consecutive replay chunks into paragraphs. A
`long_pause` boundary ends a paragraph. The `source_end` boundary ends the final
paragraph. Other chunk boundaries stay inside the paragraph.

The CLI shows the accepted text as a continuous flow. It renders a distinct
marker after every replay chunk, including the chunk at the end of each
paragraph and the chunk at the end of the document. A marker has the address
`M@N`, where `M` is the paragraph number and `N` is the left-to-right chunk
number inside that paragraph. For example, `2@3info` shows information about
the third chunk in the second paragraph. Boundary selection and boundary
changes are future work.

Paragraph operations preserve complete chunks. Joining two paragraphs removes
only the paragraph break; it does not join their chunks or change exact token
text. A paragraph can be split only at an existing chunk boundary. The CLI can
first create such a boundary before or after any complete visible token by
deriving two chunks from the immutable parent chunk.

This CLI feasibility rule is narrower than the earlier product and technical
proposals, which allow paragraph splits at arbitrary text positions and a
many-to-many relationship between paragraphs and chunks. For the current CLI,
the chunk must be split first; `split`/`isplit` and `asplit` perform that
explicit step.

## Rust domain types

The Rust types form three layers.

### Audio input

`WavInput` is the result of reading and converting the source WAV. It carries
the canonical audio used by Whisper and the identity of the original file.
`SourceFacts` records the canonical sample rate, channel count, sample count,
and source identity used by a recognition run. Recognition positions always
refer to these canonical facts.

### Recognition evidence

`RecognitionRun` is an immutable recognition result. It contains processing
windows, all decoded segment hypotheses, the accepted segment sequence, and
the replay chunks built from that sequence. `RecognitionChunk` is a complete
replay unit with text, an audio range, source segment references, and the reason
for its ending boundary.

Running recognition again or changing chunk boundaries creates new evidence.
It does not change an existing `RecognitionRun`.

### Visible document

`Document` contains ordered `Paragraph` values. A paragraph contains ordered,
indivisible visible tokens made from one or more complete replay chunks. Normal
accepted recognition tokens are used when their exact concatenation reproduces
the chunk text. Otherwise the whole chunk text becomes one pseudo-token with
unavailable alignment. A `ChunkBoundaryMarker` stores the complete-token
position after its chunk; the last marker is also the paragraph-end marker.

Each marker refers to its `RecognitionChunk` by stable recognition identity.
The visible marker address `M@N` is derived from the current paragraph and marker
order; it is not the stable identity. `M@Ninfo` follows the marker reference to
show the immutable chunk details.

The initial document is derived from one recognition run. A `long_pause` or
`source_end` chunk ends a paragraph. Every chunk belongs to exactly one initial
paragraph, and every paragraph boundary is also a chunk boundary. Later
paragraph operations may regroup complete chunks but cannot silently change
recognition evidence.

The internal fields and persistent format of these types may change. The layer
boundaries and relationships above are the durable part of the model.

## Earlier experiment

We tried Silero VAD as a separate pre-recognition chunk planner. On representative
sample data it assigned very low speech probabilities to clearly audible,
continuously recognized speech and therefore proposed misleading boundaries.
We removed that implementation and decided to derive chunks simultaneously
with recognition, using Whisper timestamps and decoded text as the evidence.
