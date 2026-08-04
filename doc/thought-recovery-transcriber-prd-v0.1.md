# Running Drafts Editor: Product Requirements Document

**Version:** 0.2 (draft)

**Product:** Running Drafts

**Component:** Transcript review and correction editor

## 1. Product summary

Running Drafts is for people who record ideas while walking, running,
commuting, or otherwise away from a keyboard. The broader product captures
speech, transcribes it, and helps the user continue writing elsewhere.

This editor covers only the middle of that journey:

```text
Existing audio and transcript
        ↓
Quick review
        ↓
Replay uncertain parts
        ↓
Correct mistakes
        ↓
Usable text for another writing tool
```

It is a text-first thought-recovery tool, not a complete writing environment or
professional transcription workstation. Its purpose is to recover the user's
meaning, not to produce a perfect verbatim transcript.

## 2. Problem statement

Automatic transcription usually captures enough of a spoken draft to be useful,
but unclear speech, motion, noise, and recognition boundaries introduce errors.
Listening to the entire recording and rewriting it wastes the advantage of
speaking the draft in the first place. Conventional transcription and audio
tools also invite exhaustive proofreading or expose timelines and controls that
do not help this user resume writing.

The editor must make uncertainty cheap to resolve: read the text, hear only the
audio needed to recover an idea, fix errors that block understanding, and move
on.

## 3. Target user

The primary user:

- records rough personal drafts while moving or away from a keyboard;
- may speak unclearly or in noisy surroundings;
- expects imperfect automatic recognition;
- wants to correct obvious or meaning-changing mistakes quickly;
- does not want to polish every sentence in this editor; and
- will continue editing or publishing in another tool.

Interviews, multi-speaker meetings, subtitle production, and linguistic research
are not the primary use cases.

## 4. Jobs to be done

When I return to a rough voice draft, I want to:

- understand what I was trying to say without replaying the whole recording;
- move directly between likely problems;
- replay a questionable word or passage with enough context to remember it;
- correct visible text by whichever method is quickest: typing, speaking,
  deleting, inserting, or choosing a suggestion;
- repair paragraph boundaries without understanding recognition chunks; and
- leave with usable text that I can continue shaping elsewhere.

Emotionally, the experience should keep me in writing mode, make imperfection
feel expected, and give me permission to stop when the text is good enough.

## 5. User research insights and hypotheses

The current direction rests on early observations and hypotheses that still need
validation:

- The transcript is an intermediate artifact; preserving the thought matters
  more than verbatim accuracy.
- Reading is faster than listening, so audio should answer local questions rather
  than lead the workflow.
- Uncertainty is sparse enough that issue-first navigation can save time.
- A mobile user often has only one hand available; repeated taps, precise
  dragging, and tiny targets create disproportionate friction.
- Suggested alternatives cannot cover every valid correction, making direct
  typing and voice replacement necessary.
- Voice replacement is valuable only where it is faster or easier than typing.
- Paragraphs are familiar document concepts and can conceal recognition-window
  constraints without teaching those constraints to the user.

These are product hypotheses, not validated conclusions. Usability research
should compare free review with issue-first review, quick replay windows, and
typing versus voice replacement.

## 6. Product principles

### Text is primary

The user reads and edits visible text. Audio and recognition metadata support
that work but do not define the document the user sees.

### Audio is contextual

Audio answers “What did I say?” or “What did I mean here?” It should appear on
demand and recede when the question is answered. This is why replay begins from
text rather than from a separate media screen.

### No waveform by default

The initial product has no waveform, timeline, playhead manipulation, waveform
zoom, or scrubbing workflow. These mechanisms optimize for audio editing rather
than rapid thought recovery. An optional advanced view may be considered later.

### Good enough is the goal

The product should emphasize mistakes that interfere with understanding and
avoid signals that imply every low-confidence word must be perfected.

### One-handed mobile use matters

Important actions must use large targets, few taps, and no dependence on precise
dragging. Standard platform selection handles may remain available for arbitrary
ranges, but common replay and issue navigation should not require them.

### Recognition internals remain hidden

Tokens, confidence calculations, recognition runs, model revisions, decoder
context, overlap, and time bases belong in the backing model and developer
tooling. User language is text, paragraph, issue, replay, and correction.

### Progressive disclosure preserves focus

Quick replay and normal editing remain immediate. Less frequent actions—slower
replay, paragraph operations, alternatives, and details—live behind a compact
menu or action sheet.

## 7. Primary workflow

1. The user opens an existing recording and transcript.
2. The user scans the transcript or jumps to the next issue.
3. When text is unclear, the user taps it for quick replay. Spoken text is
   highlighted during playback.
4. The user leaves understandable text alone and corrects only useful targets.
5. The user types, deletes, inserts, speaks a replacement, or chooses an
   alternative.
6. If paragraph boundaries are unhelpful, the user splits or merges paragraphs.
7. The user exports or copies usable text and continues writing elsewhere.

The workflow deliberately does not require full-length playback or a pass over
every system-generated issue.

## 8. Core actions

### Navigate

The user can scroll, search text, and go to the next or previous issue. An
**issue** is a place the system believes may deserve attention, not a mandatory
error. Initial signals may include low recognition confidence, ambiguous
alternatives, missing or suspicious text, noisy audio, chunk-boundary problems,
or internal validation failures. Ranking is intentionally unresolved.

Users need a way to dismiss or intentionally ignore an issue so navigation does
not repeatedly return to accepted text.

### Replay

In the common case, one intentional gesture on a word or existing selection
starts playback immediately, with brief audio context before and after the
target. The currently spoken text is highlighted where alignment permits.

A secondary menu supports replaying a selection or paragraph, replaying again,
and slower playback. Replay is a text action, not a separate audio workspace.

### Edit

The user edits visible characters through normal text operations: type
replacement text, insert, delete, speak a replacement, or choose an AI-provided
alternative. These are input methods for the same correction job.

“Retranscribe” is not a top-level user action. Recognition may run again after
an edit or structural change, but the system does not require the user to know
when or why.

### Split and merge paragraphs

The user can split a paragraph at a text position or merge it with an adjacent
paragraph. These explicit actions repair document structure and provide a
familiar way to influence future processing. They exist in part because the
recognizer processes audio fragments of approximately 30 seconds, but the UI
does not expose inference windows or chunk boundaries.

A paragraph-level affordance may reveal replay, split, merge, delete, and detail
actions in a compact menu. It should be one primary affordance with progressive
disclosure, not a row of permanent symbols. Its glyph and placement are open.

## 9. Mobile interaction model

The primary screen is a scrollable text document with paragraph spacing,
optional issue indicators, and a compact playback state. Controls must be
reachable and forgiving on a one-handed smartphone.

Selection applies to a caret, word, phrase, arbitrary character range, or whole
paragraph. Actions operate on visible characters, never exposed token
boundaries. A tap on unselected text is intended to support quick replay, but
the exact arbitration among placing a caret, selecting a word, and replaying is
an open interaction question. Long press, a contextual action sheet, and
platform editing conventions should cover secondary actions without crowding
each paragraph.

## 10. Replay flow

### Quick replay

1. The user taps a word or an already selected phrase.
2. Playback starts without a dialog or separate screen.
3. A short contextual window surrounds the target.
4. Highlighting follows the spoken text where alignment is available.
5. A compact, accessible control indicates playback and allows stopping.

The context-window rule is not settled. Candidates are fixed time before and
after, pause-aware boundaries, sentence-aware boundaries, or a hybrid. The
prototype should measure comprehension, latency, and accidental activation.

### Extended replay

A secondary action or context menu offers:

- replay selection;
- replay paragraph;
- replay more slowly; and
- replay again.

When alignment is incomplete, the product should offer the best honest fallback
(for example, a broader inherited paragraph range) and avoid implying
word-level precision. Exact fallback behavior remains open.

## 11. Editing and voice replacement

Normal typing, insertion, and deletion must behave like familiar text editing.
AI alternatives may be offered when useful but cannot be required for
correction.

Voice replacement is a first-class method:

```text
Select incorrect visible text
        ↓
Choose voice replacement
        ↓
Record a short correction
        ↓
Recognize it, optionally using nearby context
        ↓
Preview or apply the replacement text
```

The replacement changes the selected visible range. The product may use nearby
text or original audio as recognition context without exposing decoder details.
The interaction must be tested against typing for speed, error rate, and user
effort. Trigger design, whether confirmation is necessary, and whether to retain
correction audio remain open.

## 12. Paragraph split and merge

A paragraph is a user-facing editing unit. A recognition chunk is an internal
processing unit limited to approximately 30 seconds. They may initially align,
but they are not permanently identical: a paragraph may be backed by one or
several chunks, multiple recognition revisions, or partially user-written text.

Splitting creates two visible paragraphs at the chosen character position.
Merging creates one visible paragraph while preserving its constituent backing
and edits. Either action may schedule a new recognition plan; it must not
rewrite historical recognition output. If merged audio exceeds the recognizer's
limit, the engine must still use multiple chunks behind the single paragraph.

## 13. Non-goals

Version one is not:

- an audio editor, waveform editor, or production tool;
- a subtitle or timecode-authoring system;
- a professional verbatim transcription workstation;
- a complete writing, publishing, or collaboration environment;
- a multi-speaker interview or conference workflow;
- a general linguistic annotation platform;
- an interface for tuning recognizer models or token data; or
- a promise of perfect text or exhaustive proofreading.

Multiple channels and generic annotations may be represented minimally in the
technical model only where that does not complicate the core experience.

## 14. Success metrics

The main hypothesis is that an 8-minute recording can usually be reviewed into
usable text in approximately 3–6 minutes. This is an early target, not an SLA.

Supporting measures should include:

- median review time relative to recording duration and to manual
  listen-and-rewrite;
- percentage of sessions producing text the user considers usable;
- time and gestures from uncertainty to replay start;
- correction time and success rate by typing, voice replacement, and
  alternative selection;
- rate of accidental replay and immediate replay cancellation;
- share of sessions requiring full-length listening or timeline-like behavior;
- percentage of issues reviewed, ignored, or bypassed without treating 100% as
  inherently desirable; and
- task completion and reachability in one-handed mobile usability tests.

## 15. Risks

- A single tap competes with caret placement and selection conventions; a poor
  choice could make both replay and editing frustrating.
- Replay may feel slow, start with too little context, or overplay enough audio
  to erase the time advantage.
- Issue markers may create an exhaustive-proofreading mindset or lose trust if
  poorly ranked.
- Voice replacement may add recording and confirmation overhead that makes it
  slower than typing.
- Weak or stale alignment may replay misleading audio after substantial edits.
- Paragraph operations may expose recognizer limits if merges behave
  unpredictably.
- Background re-recognition could unexpectedly alter authoritative user text.
- Mobile accessibility, privacy expectations, and recording permissions could
  complicate voice replacement and playback.

## 16. Open questions

- What exactly happens on a single tap: replay, caret placement, or selection?
- How are accidental replays avoided?
- How much audio context should quick replay include, and should it be fixed,
  pause-aware, sentence-aware, or hybrid?
- How does the user stop or repeat playback with one hand and assistive tools?
- What fallback is shown when selected text has incomplete alignment?
- How is voice replacement triggered on mobile, and does it require preview or
  confirmation?
- Should correction audio be preserved, and under what privacy/retention rules?
- When may background re-recognition run without disrupting visible text?
- What feedback appears while recognition or replay data is unavailable?
- How should a paragraph affordance look and where should it appear?
- How are issue markers ranked and explained?
- How does the user mark an issue as intentionally ignored, and can it reappear
  after relevant edits?
- What does “usable text” mean to target users, and when do they decide to stop?
- What export or handoff actions are essential for continuing elsewhere?

## 17. Future opportunities

- improved issue ranking based on observed corrections;
- optional advanced audio visualization for users who need it;
- AI-assisted paragraph cleanup that never silently replaces user text;
- desktop integrations for Neovim, Zed, or other editors through a readable
  textual format, LSP diagnostics/actions, Tree-sitter highlighting, replay
  commands, folding, and paragraph operations;
- richer provenance and optional annotations; and
- collaboration or multi-channel support if personal-draft evidence warrants it.

Desktop tooling must remain an additional view over the document. It must not
turn the source format into a visible dump of recognition tokens or be required
to understand and edit the text.

## Appendix A: Documentation change summary

### Removed

- The implication that this editor captures recordings; its scope begins with
  existing audio and a transcript.
- A fixed two-second quick-replay context rule, which was not validated.
- General-purpose annotation, multi-speaker, audio-editing, and professional
  transcription ambitions from the initial scope.

### Simplified

- The product is organized around navigate, replay, edit, and paragraph
  split/merge.
- Recognition internals support the visible document without appearing in the
  primary product language.
- Desktop language tooling is a future enhancement, not the basis of the mobile
  experience or document format.

### Added

- Running Drafts context, research hypotheses, design rationale, mobile
  interactions, risks, measurable outcomes, and explicit non-goals.
- Voice replacement as a first-class correction method.
- Concrete quick and extended replay flows with unresolved choices identified.
- Explicit product decisions still requiring validation.

### Contradictions resolved

- The broader Running Drafts workflow includes recording, while this editor
  starts with existing audio and a transcript.
- “Not a transcription editor” now means not a professional or exhaustive
  transcription workstation; focused transcript correction remains its purpose.
- Paragraphs are user-facing document units, while chunks are internal and may
  have a many-to-many relationship with paragraphs.
- Visible text is authoritative; recognition may support or suggest changes but
  cannot silently replace user edits.
- Immediate replay remains a requirement, while the context-window algorithm
  remains an open question.

The unresolved product decisions are collected in Section 16. Unresolved data
model and implementation decisions are collected in Section 17 of the technical
specification.
