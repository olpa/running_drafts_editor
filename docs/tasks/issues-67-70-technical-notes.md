# Issues 67 and 70: technical decisions

These notes preserve the discussion and implementation-level refinements for
issues #67 and #70. ADR-0016 records the durable ownership decision; the GitHub
issues remain the source of planned behavior.

## Confirmed maintainer mandate

The following intent was confirmed before selecting the schema and Rust
representation.

- Work in sequence: understand the goal, agree on the top-level design, update
  issues #67 and #70 and the relevant ADRs, then implement. Do not create a new
  ticket for this work.
- Optimize first for the maintainer inspecting and debugging the project now.
  The central problem is loss of confidence: the JSON does not clearly explain
  what happened, the structure feels conceptually wrong, and the Rust model is
  difficult to explain or change.
- Treat the model as two conceptual halves:
  1. The transcription process turns uncertain audio and decoder activity into
     stable, disjoint chunks.
  2. Product work happens on top of those chunks: current text, correction,
     replay, review, and document organization.
- Treat chunks as standalone, durable product entities. They should be direct
  children of the Project or Document, should not appear subordinate to decoder
  internals, and should form a primary Rust interface. Stable chunk IDs support
  internal identity and a possible future database; the CLI continues to use
  user-facing structural addresses.
- Apply a locality principle: almost everything specifically concerning one
  chunk should be discoverable from that chunk. Apply it strictly but not
  absolutely. Shared, cross-chunk, pre-chunk, and failed-to-produce-a-chunk
  information belongs to its nearest honest owner.
- Make the raw saved JSON understandable in an ordinary text editor. It serves
  exact recovery first, diagnosis second, current-state comprehension third,
  and explanation of the model fourth. Compactness is not important.
- Present chunks prominently while also making recording-order gaps, failures,
  and other non-chunk outcomes noticeable enough for diagnosis.
- Permit any number of clearly marked human-friendly copies or projections
  when they improve inspection. Such values must be visibly non-authoritative,
  ignored during loading, and regenerated deterministically.
- Existing decision evidence is initially sufficient. The immediate problem
  is organization, not recording every rejected calculation or intermediate
  value.
- Treat the future TypeScript web application as background direction rather
  than a current specification. Preserve domain concepts and invariants first.
- Use this as a deliberate rewrite opportunity. Existing experimental schema
  shapes and accidental internal interfaces need not survive.
- Adjacent internals may change when they encode the old ownership model. Stop
  and obtain confirmation before expanding into user-visible CLI behavior,
  chunk identity or boundary rules, transcription or prompting algorithms,
  replay semantics, correction semantics, or new product capabilities.
- Accepted ADRs may be superseded when a better top-level model requires it,
  but surface the conflict and obtain confirmation instead of contradicting an
  ADR silently.
- Agree on the top structure before substantial implementation. Present a
  compact ownership tree, representative JSON, lifecycle, authority rules, and
  undo/redo behavior for approval. Resolve minor details during implementation.

This mandate conflicts with the current ownership direction in ADR-0014 if
"decode spans own chunks" makes finalized chunks subordinate to decoder
evidence. The technical design must resolve that conflict explicitly.

## Settled technical direction

- `Project` owns one authoritative `Document`. `Document.content` is an ordered
  mixed sequence whose initial variants are `Chunk` and `ParagraphBreak`.
  Persisted paragraphs, paragraph identities, paragraph revisions, and a second
  chunk-order projection are removed. Paragraphs and displayed addresses are
  derived by scanning this sequence.
- JSON document items use an internal `type` tag. A Chunk's fields share the
  item object; there is no redundant `{ "type": "chunk", "chunk": { ... } }`
  wrapper.
- A finalized Chunk is a standalone product entity owned by the Document, not
  by the decode span that produced it. Decode spans are optional process
  evidence only.
- A Chunk exposes explicit references to its immediate preceding Chunk, its
  audio source, and an immutable transcription profile. Future transcription
  prompts are built live by following the preceding-Chunk chain and reading
  current transcriptions; no persisted prompt is operational input.
- Validate `previous_chunk_id` against the preceding Chunk in Document order,
  ignoring ParagraphBreak items. Document order remains authoritative; the
  reference is a navigation edge for prompt construction and future storage.
- Group Chunk JSON into `audio`, `transcription`, and `annotations`, while
  keeping `id` and `previous_chunk_id` prominent. `audio` contains its source
  reference, canonical sample range, and alignment. `transcription` contains
  the current result and its profile reference.
- A Chunk contains one current Transcription rather than a collection plus a
  current-selection ID. Undoable and redoable earlier Transcriptions live in
  Project history. After undo, use the canonical term `current transcription`,
  not `latest transcription`.
- Introduce immutable, ID-addressed transcription profiles. Project settings
  select the active profile for the next operation; each Chunk records the
  profile of its current Transcription. History restores both together.
- Store Chunk annotations next to the current Transcription. Attention marks
  target token offsets/identities rather than byte or character offsets and
  move through history with the Transcription they annotate.
- Keep ParagraphBreak as a Document content item rather than a Chunk flag. It
  has durable sequence semantics but is not intrinsic to a standalone Chunk.
- Reserve `_inspection` for human-readable, non-authoritative material located
  beside the data it explains. A Transcription may export the exact prompt IDs
  and readable prompt text used to produce it under `_inspection`. Import
  ignores this field and no behavior consults it.
- Initial decoder/process details may be exported as `_inspection` material
  when available but are ignored on import and may disappear after a
  load/save round trip. Exact recovery applies to current document state,
  references, profiles, annotations, and reachable history—not to this process
  trace.
- Audio sources and transcription profiles live outside the Document and are
  referenced by ID. The persisted coordinate names remain `start_sample` and
  `end_sample`, following the canonical mono 16 kHz sample-offset model.
- `settings` and `history` remain Project-owned outside the Document.

This direction supersedes ADR-0014. It also deliberately narrows the evidence
round-trip requirements currently written in issues #67 and #70 and in the
initial-transcription documentation.

## Implementation refinements

- Use schema `rde-project/v4-experimental`; earlier experimental formats are
  intentionally unsupported.
- Keep `id` and `previous_chunk_id` flat because they explain identity and
  sequence. Group the remaining chunk state into `audio`, `transcription`, and
  `annotations`; this avoids both a field bag and unnecessary wrapper objects.
- Persist one current Transcription inside each Chunk. Its immutable profile is
  referenced by `profile_id`; the profile owns model, language, and the full
  decode configuration. Previous current Transcriptions remain reachable only
  through Project undo/redo history.
- Do not repeat audio-source facts or transcriber implementation identity in a
  Transcription. Its containing Chunk owns the audio-source reference and its
  profile identifies the relevant transcription configuration. Initial-process
  source facts may still appear in ignored `_inspection` evidence when they
  explain failures or audio that produced no Chunk.
- Omit the standard Whisper timestamp scale of 160 samples per timestamp unit;
  write `samples_per_unit` only for a nonstandard scale. Render each token's
  alternatives array compactly on one line so candidate evidence remains easy
  to scan.
- Keep token-to-audio mappings and resolved multi-token issues at Project scope.
  They are cross-cutting indexes or review state rather than intrinsic Chunk
  content. Keep attention marks in their target Chunk.
- Treat derived paragraph and replay structures as runtime caches. Rebuild them
  from authoritative mixed content after loading and after restoring history.
- Reject unknown authoritative fields and invalid references. `_inspection` is
  the deliberate exception: it is written for maintainers and ignored on load.
- Preserve transcription, correction, replay, and CLI behavior. This change
  reorganizes ownership and persistence without changing the algorithms or
  user workflow.
