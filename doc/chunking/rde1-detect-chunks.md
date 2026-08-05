# Investigate Recognition and Paragraph Segmentation

## Purpose

Investigate the state of the art for dividing existing recordings into:

- **recognition chunks:** internal audio ranges constrained by a recognizer's
  input limits; and
- **paragraphs:** readable, editable units of authoritative visible text.

This discovery must determine how the two structures relate without assuming
that they are identical. A paragraph may span multiple recognition chunks, and
a recognition chunk may contribute to multiple paragraphs.

The outcome is an evidence-based recommendation and a refined implementation
ticket. This task does not ship the production chunker.

## Research questions

- What current approaches are available for offline audio segmentation,
  including fixed windows, silence detection, voice-activity detection,
  ASR-aware segmentation, transcript-assisted segmentation, semantic
  segmentation, and hybrids?
- Which approaches are suitable for a simple, inspectable, line-oriented CLI?
- How does each approach affect recognition quality near boundaries?
- When does overlap help, and what duplication or reconciliation burden does
  it introduce?
- Is audio-only segmentation sufficient for initial recognition?
- When a transcript is available, can its text or paragraph boundaries provide
  useful optional evidence?
- How can imperfect or user-edited paragraph boundaries inform segmentation
  without controlling recognition-chunk identity?
- How do candidate approaches behave with continuous speech, hesitation,
  breaths, long silence, background noise, music, and missing metadata?
- Which constraints and parameters are recognizer-specific or source-specific?
- What boundary evidence and configuration must be retained to reproduce or
  explain a recognition plan?
- Which libraries, models, media tools, and runtime dependencies are actively
  maintained and appropriate for the prototype?

## Investigation

1. Survey current research, maintained tools, and primary documentation.
2. Distinguish offline segmentation from streaming endpoint detection. Include
   streaming techniques only when their ideas transfer to existing audio.
3. Compare the major segmentation families in a decision matrix.
4. Assemble a small, documented, non-sensitive evaluation corpus containing:
   - speech with clear pauses;
   - continuous speech near and across recognizer limits;
   - hesitation, filler words, and breaths;
   - short and long silence;
   - noisy audio or music;
   - very short, near-limit, and long recordings; and
   - imperfect transcripts with correct, incorrect, and absent paragraph
     breaks.
5. Exercise at least two materially different approaches on the same corpus,
   including an audio-only baseline.
6. Where feasible, compare downstream recognition results while preserving raw
   experiment output.
7. Compare audio-only, transcript-assisted, and paragraph-assisted variants
   without treating text boundaries as mandatory.
8. Examine variants with and without overlap without selecting a default before
   collecting evidence.
9. Record runtime, determinism, dependency, licensing, portability, and
   integration characteristics.

Experimental code may be disposable or isolated. It must not establish a
public command grammar, become the production chunker by accident, or expand
the project into a waveform or full-screen TUI.

## Evaluation criteria

- **Legality:** submitted ranges respect the intended recognizer's actual input
  constraints, including overlap and time rounding.
- **Coverage:** spoken content is not omitted, and gaps and overlaps are
  measurable and explicit.
- **Boundary quality:** words or phonemes are not unnecessarily cut and useful
  acoustic context is retained.
- **Recognition outcome:** boundary-local omissions, duplication,
  substitutions, and overall transcript impact are measured where possible.
- **Reconciliation burden:** duplicated recognized text and ambiguity caused by
  overlap are understood.
- **Robustness:** behavior is understood for continuous speech, silence, noise,
  short final ranges, missing evidence, and detector failure.
- **Paragraph independence:** segmentation works without a transcript, and
  paragraph evidence remains an optional hint rather than hard ownership.
- **Reproducibility:** boundaries can be explained and reproduced from recorded
  evidence and configuration.
- **CLI feasibility:** runtime, memory, startup cost, dependencies, platform
  availability, and inspectability suit the technical prototype.
- **Complexity:** implementation and maintenance cost are proportional to the
  demonstrated quality benefit.

## Deliverables

1. A concise research note with dated sources that distinguishes established
   findings from project inference.
2. A comparison matrix of candidate approaches and tools.
3. A corpus manifest and reproducible experiment instructions. Fixtures must be
   redistributable or locally generated and must not contain user recordings.
4. Machine-readable experiment results and a short, seam-focused review.
5. A written distinction between recognition segmentation and paragraph
   formation, including permissible and impermissible coupling.
6. Example recognition-plan output showing explicit ranges, overlap, boundary
   evidence, source identity, configuration identity, and failures. Treat this
   as a schema proposal rather than a durable format commitment.
7. A decision record recommending an approach, documenting rejected
   alternatives, risks, and unresolved parameters.
8. A follow-up implementation ticket whose behavior and tests derive from the
   findings.

Generated experiment output must follow the repository rule against committing
generated artifacts unless a small fixture or result is intentionally reviewed
and required for reproducibility.

## Exit criteria

Discovery is complete when:

- the major approach families have been assessed, or excluded with recorded
  reasons;
- at least two viable strategies have been exercised on the common corpus;
- recognition-chunk and paragraph responsibilities are explicit and consistent
  with the project invariants;
- the recommended approach demonstrably creates complete, recognizer-legal
  ranges and has a known fallback when preferred boundary evidence is absent;
- the effect and reconciliation cost of overlap are understood well enough to
  scope implementation;
- dependency, licensing, timing, rounding, and reproducibility risks are
  recorded;
- remaining choices are named rather than silently resolved; and
- the implementation ticket can define its input/output contract, selected
  strategy, configuration, acceptance tests, and revision behavior without
  inventing product policy.

## Non-goals

- Shipping or integrating the final segmentation algorithm.
- Selecting numeric thresholds or defaults without experimental evidence.
- Making paragraphs and recognition chunks one-to-one.
- Solving recognition execution or overlapping-text reconciliation.
- Designing paragraph editing, replay, or boundary-symbol interaction.
- Building a waveform, full-screen TUI, diarization, or multi-speaker UI.
- Treating developer-visible diagnostics as a final end-user UI commitment.

## Follow-up implementation task

After discovery, refine and implement:

> **Detect pauses and create recognition chunks**
>
> Implement the selected approach to divide long audio into legal, revisioned
> recognition ranges. Store actual boundaries, overlap, configuration, and
> relevant boundary evidence explicitly.

The discovery determines the selected approach, the recognizer-specific meaning
of legal ranges, appropriate overlap behavior, and the focused test corpus.
Recognition execution, overlap-text reconciliation, visible paragraph
construction, and re-chunking after edits remain separate tasks unless the
investigation demonstrates that a minimal interface is required.
