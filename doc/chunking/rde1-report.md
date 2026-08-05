# Recognition chunking and paragraph segmentation: discovery report

**Status:** research/design recommendation; not production implementation  
**Research date:** 2026-08-04 UTC  
**Source brief:** `rde1-detect-chunks.md`

## Executive summary

Recognition chunks and paragraphs must have separate identities. A recognition
chunk is a reproducible, recognizer-legal audio request. A paragraph is a
revisioned, readable unit of authoritative text. Either may cross the other's
boundaries; aligned word/time spans create the many-to-many bridge.

Recommend a **recognizer-aware audio-only hybrid**: decode onto an integer sample
clock; obtain speech probabilities with a pinned small VAD (prototype Silero
ONNX behind an interface); choose a low-activity valley in a bounded look-back
window before the hard recognizer limit; and deterministically hard-cut when no
candidate exists or detection fails. Persist source/config/model identities,
core and submitted ranges, overlap, evidence, and fallback reasons.

Do not enable overlap by default until recognizer-specific seam experiments and
text reconciliation demonstrate a net benefit. Overlap adds context but creates
duplicate or conflicting text. Transcript and paragraph boundaries may later be
soft evidence, never requirements or owners of chunks.

WhisperX is the strongest directly relevant precedent: VAD cut-and-merge
improved long-form recognition and enabled batching, while forced alignment was
used afterward for word timing ([Bain et al., 2023](https://www.isca-archive.org/interspeech_2023/bain23_interspeech.html)).

## Definitions and invariants

- **Recognition plan:** revisioned declaration of ranges submitted to one
  identified recognizer/configuration.
- **Recognition chunk:** half-open source range `[start,end)` plus optional
  submitted padding. Its concerns are legality, acoustics, retries, provenance.
- **Paragraph:** editable visible-text span formed from punctuation, semantics,
  pauses, markup, speaker evidence, and user intent.
- **Boundary evidence:** silence, VAD valley, aligned gap, paragraph hint, or
  hard-limit fallback explaining why a legal point was selected.

Required invariants:

1. Core ranges exactly cover the intended source, or excluded ranges are explicit.
2. Submitted ranges are legal after overlap, rounding, and encoding.
3. Canonical coordinates are integer samples/time-base ticks, never float seconds.
4. Source, adapter, config, detector/model, and evidence reproduce the plan.
5. Paragraph edits never silently mutate recognition plans.
6. Detector failure yields a legal deterministic fixed-window fallback.

## Intermediate findings

### Limits are recognizer contracts, not one duration

Whisper pads/trims model input to 30 seconds while long-form transcription loops
over internal windows ([README](https://github.com/openai/whisper/blob/main/README.md),
[implementation](https://github.com/openai/whisper/blob/main/whisper/transcribe.py)).
Hosted APIs independently constrain formats and request transport
([OpenAI Audio API](https://platform.openai.com/docs/api-reference/audio/createTranscription)).
Therefore a versioned adapter must expose decoded-sample, duration, byte,
encoding, and rounding limits. A global `max_seconds=30` is incorrect design.

### Silence is not speech activity

FFmpeg `silencedetect` identifies samples below an amplitude/noise threshold for
a duration ([official manual](https://ffmpeg.org/ffmpeg-filters.html#silencedetect)).
It is deterministic and inspectable, but quiet speech, fans, noise, and music
break the equation “quiet = not speech.” Keep it as baseline/diagnostic.

The common WebRTC binding accepts mono 16-bit PCM at 8/16/32/48 kHz in
10/20/30 ms frames and needs collector/hangover logic
([py-webrtcvad](https://github.com/wiseman/py-webrtcvad)). Silero is an
MIT-licensed neural VAD with ONNX/PyTorch paths and 8/16 kHz support
([repository](https://github.com/snakers4/silero-vad)). Its history lists v6.2
in December 2025 ([history](https://github.com/snakers4/silero-vad/wiki/Version-history-and-Available-Models)).
A 2026 Google-authored benchmark reports Silero ahead of WebRTC/RMS and a
hysteresis benefit for WebRTC, subject to local corpus confirmation
([McKinnon et al.](https://research.google/pubs/window-size-versus-accuracy-experiments-in-voice-activity-detectors/)).

### VAD is evidence, not a planner

VAD returns regions/probabilities. Planning must still merge nearby speech,
avoid tiny requests, honor hard limits, cover continuous speech, retain context,
and account for silence. Silero's timestamp utility exposes duration, padding,
and maximum-region controls
([utility](https://github.com/snakers4/silero-vad/blob/master/src/silero_vad/utils_vad.py));
`faster-whisper` constrains VAD chunks to model chunk length
([implementation](https://github.com/SYSTRAN/faster-whisper/blob/master/faster_whisper/transcribe.py)).
Separate detector from deterministic constrained planner and record both outputs.

### Overlap is model-dependent insurance

For CTC, overlapping inference with discarded edge logits gives boundary sounds
central context ([CTC chunking](https://huggingface.co/blog/asr-chunking)). For
autoregressive text ASR, overlap can produce different repetitions, punctuation,
and timestamps. Naive long-form sliding windows also risk drift, repetition, and
hallucination ([WhisperX paper](https://arxiv.org/abs/2303.00747)). Represent
overlap now; default to zero until seam quality and deterministic reconciliation
are measured.

### Audio-only bootstrap is sufficient

Fixed windows always work; VAD improves boundary preference without text.
Forced alignment can later map known text to time, but imperfect text may omit,
insert, normalize, or stale-edit speech. Montreal Forced Aligner explicitly
requires audio/text/language resources
([documentation](https://montreal-forced-aligner.readthedocs.io/)); long-audio
CTC alignment tolerates degraded transcripts but still has error
([Kürzinger et al., 2023](https://doi.org/10.3390/app13031854)). Text evidence
may adjust scores only within a legal search interval and never suppress coverage.

### Paragraph formation is downstream

Pauses may mean breath, hesitation, emphasis, or waiting, while topic shifts can
occur without pauses. TextTiling's lexical-cohesion approach illustrates that
semantic units are a text problem, not an ASR-request constraint
([Hearst, 1997](https://aclanthology.org/J97-1003/)). Form paragraphs after
recognition/alignment and keep user/imported boundaries as presentation evidence.

## Decision matrix

| Family | Boundary behavior | Noise/continuous speech | Cost/inspectability | Decision |
|---|---|---|---|---|
| Fixed | Predictable word cuts | Noise-proof; always terminates | Minimal/excellent | Mandatory fallback |
| RMS/silence | Good clean pauses | Weak with noise/music/quiet speech | Minimal/excellent | Baseline only |
| WebRTC VAD | Speech-aware; needs hangover | Domain-sensitive; hard fallback | Native/light/good | Comparator |
| Silero VAD | Probability valleys | Reportedly robust; still domain-sensitive | Small ONNX/good if logged | Preferred prototype |
| pyannote | Rich speech/speaker evidence | Model/domain-sensitive | Heavy/moderate | Defer unless diarization |
| ASR-aware | Closest to recognizer | Inherits ASR drift/hallucination | Coupled/moderate | Later evidence |
| Forced alignment | Avoids aligned words | Fails with missing text/models | Heavy/moderate | Optional evidence |
| Paragraph hint | Preserves author intent | May be wrong/stale | Needs alignment/high | Soft score only |
| Semantic | Readable topic units | Acoustic-independent | NLP/variable | Paragraph subsystem |
| Constrained hybrid | Combines legal in-window evidence | Deterministic fallback | Proportional/high | Target architecture |

## Difficult input behavior

| Input | Required behavior |
|---|---|
| Clear pauses | choose stable low-speech valley; log extent/score/sample |
| Continuous speech | hard-cut last legal sample; `hard_limit_no_candidate` |
| Hesitation/breath | minimum gap/hysteresis prevents micro-fragmentation; pad edges |
| Long silence | deterministic tie-break; do not issue many empty requests |
| Noise/music | prefer VAD to RMS; fall back when evidence is uninformative |
| Missing metadata | trust decoded sample count; reject undecodable source explicitly |
| Very short/near-limit | exact legal range; preflight after transform/encoding |
| Short final tail | preserve; merge backward only if still legal |
| Detector failure | complete fixed-window plan plus recorded error |
| Edited paragraph | leave plan unchanged; explicit new revision only |

## Pilot experiment and intermediate results

The local workspace has Node.js but no FFmpeg, Python, VAD, or ASR runtime. A
deterministic 16 kHz synthetic planner pilot therefore compared:

- fixed 10-second windows;
- RMS-pause search over the final two seconds before the limit, requiring 200 ms
  below RMS 0.025, otherwise fixed fallback.

```json
{
  "experiment":"synthetic-planner-pilot-2026-08-04",
  "results":[
    {"fixture":"clear-pauses","fixed":{"ranges_s":[[0,10],[10,20],[20,25]],"speech_cuts":2},"energy":{"ranges_s":[[0,8.94],[8.94,18.36],[18.36,25]],"speech_cuts":0}},
    {"fixture":"continuous","fixed":{"ranges_s":[[0,10],[10,20],[20,25]],"speech_cuts":2},"energy":{"ranges_s":[[0,10],[10,20],[20,25]],"speech_cuts":2}},
    {"fixture":"noisy-pauses","fixed":{"ranges_s":[[0,10],[10,20],[20,25]],"speech_cuts":2},"energy":{"ranges_s":[[0,10],[10,20],[20,25]],"speech_cuts":2}}
  ]
}
```

Every result covered exactly 25 seconds without gaps/overlap and stayed at or
below 10 seconds. Clean-pause cuts landed in known silence; noise defeated the
absolute threshold; continuous speech required hard cuts. This validates planner
mechanics only, not speech detection or WER, and selects no numeric defaults.

## Real-speech corpus manifest

| ID | Source/construction | Phenomena | Text variants/license |
|---|---|---|---|
| `gen-clear-pauses` | pinned local TTS, authored sentences, inserted 0.2/0.8/2.5 s gaps | pause lengths/near-limit | correct/wrong/no paragraph; audit voice terms |
| `gen-continuous` | clauses without inserted gaps across limit | unavoidable seam | exact authored text |
| `hesitation` | explicitly consented redistributable fixture | fillers/restarts/breath | verbatim and cleaned |
| `cv-clean` | pinned Common Voice release/item | voices/accents/short | catalog labels CC0 ([catalog](https://commonvoice.mozilla.org/en/datasets)) |
| `libri-chain` | pinned LibriSpeech items + gaps | clean long read speech | CC BY 4.0/attribution ([OpenSLR](https://www.openslr.org/12/)) |
| `noise-mix` | speech + generated noise at recorded SNR | stationary noise | inherited text |
| `music-bed` | explicitly CC0 instrumental + speech | music false positives | item-specific license |
| `edge-*` | 0.1 s, limit ± one sample, long with short tail | legality/rounding/tail | generated |

MUSAN may supply research noise/music, but constituent provenance must be
checked rather than assuming a blanket license
([paper](https://arxiv.org/abs/1510.08484)). Each manifest record must contain
release/item IDs, URLs, licenses, original/output SHA-256, transformation/tool
versions, decoded sample facts, phenomena, and transcript variants.

## Reproducible experiment protocol

1. Pin OS/architecture, decoder, detector/model hash, recognizer/model/API, and config.
2. Decode once to canonical mono PCM; record decoded samples.
3. Run fixed, RMS, Silero, and if feasible WebRTC; test zero and candidate overlap.
4. Assert exact core coverage, explicit intersections, legality, order, termination,
   and byte-identical deterministic canonical output.
5. Encode using the adapter and preflight duration/bytes afterward.
6. Recognize independently; preserve raw responses, timings, errors, and cost.
7. Reconcile separately and preserve raw/reconciled hypotheses.
8. Report raw/normalized global and seam WER/CER, omissions, duplicates,
   substitutions, and manual ambiguity.
9. Measure detector/planner/ASR time, startup, peak RSS, submitted audio/bytes/calls.
10. Compare absent transcript, aligned transcript, and correct/wrong/no paragraph.
11. Emit JSONL and a seam review; ignore bulk artifacts unless intentionally reviewed.

Required metrics include coverage gap/overlap samples, legality failures,
speech-cut count, distance to reference nonspeech, seam omissions/duplicates/
substitutions, WER/CER, reconciliation ambiguities, and real-time factor.

## Recognition-plan schema proposal

```json
{
  "schema":"recognition-plan-proposal/v1","plan_id":"sha256:...","revision":1,
  "source":{"content_sha256":"...","decoded":{"sample_rate_hz":16000,"channels":1,"samples":400000}},
  "recognizer_contract":{"adapter":"...","version":"...","model":"...","max_decoded_samples":160000,"max_request_bytes":null,"encodings":["pcm_s16le"],"rounding":"integer-samples-half-open"},
  "planner":{"name":"vad-valley","version":"...","config_sha256":"...","config":{"search_back_samples":32000,"minimum_core_samples":80000,"left_padding_samples":0,"right_padding_samples":0,"candidate_threshold":"experiment-selected"}},
  "detector":{"name":"silero-vad-onnx","package_version":"...","model_sha256":"...","provider":"cpu","evidence_sha256":"..."},
  "chunks":[
    {"id":"chunk:...","ordinal":0,"core":{"start_sample":0,"end_sample":143040},"submitted":{"start_sample":0,"end_sample":143040},"overlap":{"left_samples":0,"right_samples":0},"boundary":{"kind":"vad_valley","selected_sample":143040,"candidate_interval":{"start_sample":136000,"end_sample":150400},"probability":{"min":0.01,"mean":0.04,"max":0.12},"hints":[],"fallback":null},"preflight":{"legal":true,"decoded_samples":143040,"encoded_bytes":286124}},
    {"id":"chunk:...","ordinal":1,"core":{"start_sample":143040,"end_sample":303040},"submitted":{"start_sample":143040,"end_sample":303040},"overlap":{"left_samples":0,"right_samples":0},"boundary":{"kind":"hard_limit","selected_sample":303040,"hints":[{"kind":"paragraph","revision":7,"accepted":false}],"fallback":{"code":"hard_limit_no_candidate"}},"preflight":{"legal":true,"decoded_samples":160000,"encoded_bytes":320044}}
  ],
  "failures":[]
}
```

Core ranges partition intended coverage; submitted ranges may overlap. Seconds
are rendered views, not stored canonical coordinates.

## Coupling rules

Permissible: score aligned gaps/breaks within the legal window; provenance-link
paragraphs to all contributing chunks; explicitly create a new plan revision;
form paragraphs from text/acoustic/user evidence; support many-to-many mapping.

Impermissible: derive chunk IDs from paragraphs; force paragraph breaks into
audio cuts or seams into visible breaks; skip untranscribed audio; re-chunk on a
newline; require text for recognition; use paragraph ownership to reconcile overlap.

## ADR-RDE1-001

**Decision:** constrained recognizer-aware planner, Silero ONNX audio evidence,
fixed fallback, detector abstraction, integer coordinates, explicit schema.
Overlap is represented but zero until experiments; text hints are deferred/soft.

**Rejected/deferred:** fixed-only (cuts speech), silence-only (noise failure),
WebRTC default (retain comparator), pyannote (disproportionately heavy),
ASR-owned segmentation (circular/coupled), forced alignment first (requires
text/language assets), semantic chunking (paragraph concern), unconditional
overlap (unmeasured duplication).

**Risks/gates:** missed quiet speech (never delete coverage; stratify); music
false positives (fixtures/fallback); drift (pin hashes/provider); ONNX footprint
(benchmark WebRTC/RMS); byte illegality (post-encoding preflight); rounding
(integer half-open ranges); continuous speech (explicit fallback, test padding);
duplicates (zero overlap until reconciliation); edit instability (immutable
separate IDs); artifact creep (commit only reviewed manifests/scripts/results).

Intentionally unresolved: actual recognizer contract; model/version; VAD
threshold/hysteresis/durations; search width/minimum range/scoring; silence
accounting; overlap/reconciliation; context prompting; scoring normalization;
source-class profiles. Evidence, not this report, must select values.

## Refined implementation ticket

### Detect low-speech boundaries and create legal revisioned recognition plans

Given immutable audio, a recognizer contract, and configuration, emit a
deterministic plan whose core covers the intended source, submitted ranges are
legal, and each boundary explains evidence/fallback. Do not recognize or form
paragraphs.

Before numeric defaults/nonzero overlap, run the real corpus with fixed, RMS,
Silero and a materially different VAD if feasible; measure global/seam ASR with
the actual target; show a quality benefit at acceptable cost; and demonstrate
reconciliation separately for overlap.

Algorithm:

1. Decode to integer samples/ticks; obtain adapter constraints.
2. Get detector evidence; failure switches to fixed fallback.
3. Form the final legal look-back interval after considering padding.
4. Deterministically score low-speech candidates and optional in-window hints.
5. If none qualifies, use the adapter's last legal integer boundary.
6. Repeat to end; preserve short tail; derive clipped submitted padding.
7. Encode/preflight bytes where needed; validate the entire plan atomically.

Acceptance tests:

- zero/short/exact-limit/limit±one-sample behavior;
- clear pause selection; continuous/noisy/music fallback reason;
- detector failure equals fixed core plan;
- exact core union and no core intersection;
- submitted intersection equals explicit overlap;
- safe edge padding and post-encoding byte checks;
- no rounding gaps/inversion/oversize across sample rates/time bases;
- deterministic plan content and identity changes for source/config/model changes;
- correct/wrong/missing paragraph hints never control coverage/identity;
- separate fixture proves many-to-many paragraph/chunk provenance;
- property tests for legality, coverage, order, termination, determinism.

Inputs are source identity/media facts, adapter contract, pinned detector,
configuration, and optional provenance-bearing hints. Outputs are a versioned
plan, JSON/JSONL boundary diagnostics, hashes, and explicit failures—never text
or paragraphs. CLI naming is deliberately unspecified. ASR execution,
reconciliation, paragraph UI, diarization, waveform UI, and automatic replan on
edits remain out of scope.

## Exit assessment

Major families, dependencies, paragraph independence, schema, risks, fallback,
and ticket contract are complete at discovery level. Two strategies were
exercised for planner mechanics. Real-speech VAD/ASR, seam WER, exact dependency
pins, and overlap value remain a mandatory implementation experiment gate. This
is an explicit evidence boundary, not a silently chosen policy.

## Dated primary source register

Accessed 2026-08-04:

1. [Whisper paper](https://cdn.openai.com/papers/whisper.pdf) and [code](https://github.com/openai/whisper).
2. [WhisperX, INTERSPEECH 2023](https://www.isca-archive.org/interspeech_2023/bain23_interspeech.html).
3. [FFmpeg filter manual](https://ffmpeg.org/ffmpeg-filters.html#silencedetect).
4. [Silero VAD](https://github.com/snakers4/silero-vad).
5. [py-webrtcvad](https://github.com/wiseman/py-webrtcvad).
6. [pyannote.audio](https://github.com/pyannote/pyannote-audio).
7. [McKinnon et al., 2026](https://research.google/pubs/window-size-versus-accuracy-experiments-in-voice-activity-detectors/).
8. [Hearst, 1997](https://aclanthology.org/J97-1003/).
9. [Common Voice catalog](https://commonvoice.mozilla.org/en/datasets).
10. [LibriSpeech/OpenSLR 12](https://www.openslr.org/12/).
11. [MUSAN paper](https://arxiv.org/abs/1510.08484).
12. [Montreal Forced Aligner](https://montreal-forced-aligner.readthedocs.io/).
13. [Long-audio CTC alignment](https://doi.org/10.3390/app13031854).
14. [OpenAI Audio API](https://platform.openai.com/docs/api-reference/audio/createTranscription).

No conclusion relies solely on a secondary comparison. Performance claims are
attributed or explicitly project inference and require local corpus confirmation.
