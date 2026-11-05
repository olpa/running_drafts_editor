---
status: accepted
---

# Let Section Capture and Delivery own frame cycles and feedback events

Browser Capture, Transport frame production, durable delivery, and user feedback
form one safety-sensitive workflow. Implement that workflow as one deep **Section
Capture and Delivery** module. Its interface exposes Section actions, observable
state, and semantic outcomes while hiding frame scheduling, browser recording,
delivery deadlines, retry coordination, and feedback timing. This keeps the
decision about when captured audio should become durable in one place and makes
the module's interface the production and test seam.

This ADR deliberately includes the agreed internal interfaces as an exception
to keeping implementation detail in code. They preserve the ownership and
recovery decisions made before implementation, so the refactor and the later
Storybook laboratory use the same seams. The original ownership requirements
remain in [issue #111](https://github.com/olpa/running_drafts_editor/issues/111),
and Capture-alert and final-palette acceptance remains in
[issue #112](https://github.com/olpa/running_drafts_editor/issues/112).

## Ownership hierarchy

Use Application -> Project(s) -> Section(s) -> `CaptureAndDelivery`. Only the
lowest module coordinates Capture and delivery:

- Application owns application lifetime, Account and Application Session rules,
  and the global Capture-alert collection.
- A Project owns its identity and ordered Sections.
- A Section holds its identity, position, Audio Asset identity, and one
  `CaptureAndDelivery` instance. It need not wrap that instance in forwarding
  methods.
- `CaptureAndDelivery` owns microphone lifecycle, Transport frame cycles,
  delivery deadlines, retained audio, retry coordination, sealing, and semantic
  feedback for its Section.

The former `ProjectRecorder` and `Recording` names misused Recording for
Capture coordination. Their responsibilities are now split between `Project`,
`ApplicationCapture`, and the Section's `CaptureAndDelivery`, rather than three
modules named Capture and Delivery. Upper levels manage ownership and access, not the steps
of frame production and delivery. This hierarchy does not introduce simultaneous
Capture or expand the existing Project workflow.

## MicrophoneAccess and user takeover

One shared `MicrophoneAccess` module controls exclusive microphone use within
the application. Application creates and supplies it; `CaptureAndDelivery`
acquires access before starting and releases it when microphone Capture ends.
Delivery may continue after release. Browser microphone permission remains the
browser recorder adapter's responsibility.

A stale reservation must not permanently prevent Capture. If another holder
exists, show a modal offering **Cancel** and **Stop previous Capture and start
new**. Cancellation leaves the current holder unchanged. Explicit takeover
revokes the old reservation, tells its recorder to stop and release the
microphone, and grants a new reservation for the new Capture. Merely clearing a
busy flag is insufficient. Keep microphone cleanup close to this module so
takeover does not rely entirely on the old workflow finishing correctly.

Reservations have distinct identities. Late release by a revoked holder must
not release its successor's reservation. Preserve the previous Section's
retained audio and delivery state. If it was capturing, takeover is an explicit
user-requested stop and does not raise a new Capture alert. It also does not
acknowledge already active Capture alerts.

Starting a new Section remains available while an older Section is finalizing.
If its reservation is still held, the same modal offers takeover. Forced cleanup
also releases a stalled finalization wait; it must not depend on that promise
settling by itself. If Section creation finishes after its reservation was
revoked, keep the Section identity and close its unused Audio Asset without
starting Capture or suspending delivery as if authentication had ended.

Scoped automatic release alone is insufficient: bugs can leave a reservation
held, so explicit user takeover remains necessary. Application does not inspect
Projects or Sections to prevent duplicate Capture; `MicrophoneAccess` enforces
exclusive access at acquisition.

## Frame-cycle clock

The Section Capture and Delivery module is the actor that starts each Transport
frame cycle. At actual monotonic time `T1` it requests the current frame from the
browser adapter, starts that frame's securing deadline, and schedules the next
boundary for actual `T1 + intervalMs`. `TransportFrameTiming` carries the frame
interval and securing deadline as validated data.

`T1` is the moment the application requests a frame. It is not the eventual
Blob arrival, HTTP request start, or an ideal wall-clock boundary. Time spent
waiting for the browser to produce the Blob, preparing the request, transferring
it, and receiving acknowledgement all consumes the same securing budget. This
measures the complete path the user cares about: how long audio remains
unsecured after the application decided it should be delivered.

Scheduling uses fixed delay rather than a fixed grid. If a boundary intended for
5 seconds runs at 5.4 seconds, that actual execution becomes `T1`, and the next
boundary is 10.4 seconds. Do not create a short catch-up frame. Scheduler
lateness before actual `T1` is neither a user-visible failure nor part of the
securing deadline; when JavaScript cannot run, the application cannot reliably
report the delay in real time. Capture duration remains based on its independent
monotonic start time.

A requested frame whose Blob has not arrived is still an unsecured frame. It
uses the same deadline and backlog rules as a frame stalled in HTTP delivery;
there is no separate browser-frame timeout or user cue. Production uses a fixed
5-second interval and a 2000 ms securing deadline, with supported deadline
configuration limited to 500–2000 ms. Interval experiments belong to Storybook;
the deployed application rejects a different interval so its risk copy and
backlog bound cannot drift from production behavior. Production timing is explicit
validated configuration, never a persistent `localStorage` override.

The oldest pending Transport frame retries 3 seconds after its first attempt.
If the original and retry remain unresolved, Capture continues through the second
5-second frame and stops at that frame boundary when the two-frame backlog limit
is reached. This gives the attentive user one warned frame in which to decide
whether to keep speaking while bounding retained risk. Later retries wait 10 and
17 seconds from their preceding attempts; they primarily support delivery after
user Stop or explicit recovery, when no new frame boundary forces interruption.

The browser recorder, monotonic clock and scheduler, backend gateway, and sound
player are adapters at internal seams. They execute the module's decisions; they
do not direct the workflow. Vue components render module state and invoke its
actions. They never coordinate frame cycles or call browser and HTTP adapters.

### FrameTiming

Use one internal `FrameTiming` module for regular frame ticks and independent
per-frame deadlines. Construction supplies the frame interval, securing
deadline, replaceable clock/scheduler, and `onFrameTick` and `onFrameDeadline`
callbacks. Its agreed interface is:

```ts
interface FrameTiming {
  markFrameRequested(frameId: number, atMs: number): void;
  stopFrameTicks(): void;
  dispose(): void;
}
```

At an actual frame request, `CaptureAndDelivery` marks `T1`. `FrameTiming`
schedules the next regular tick for `T1 + intervalMs` and this frame's deadline
for `T1 + secureDeadlineMs`. It reports timing facts; `CaptureAndDelivery`
decides whether the frame is still unsecured and what feedback is needed.
The timing module knows nothing about Blobs, HTTP delivery, or Capture alerts.

Do not cancel individual frame deadlines when audio becomes secured. Let those
events fire; `CaptureAndDelivery` silently ignores events that no longer match
pending work, with optional diagnostic logging. `stopFrameTicks()` stops future
regular ticks while preserving existing deadline events. Final partial frames
still need a marked request and deadline after regular ticks stop; they must
not restart regular ticks. `dispose()` cancels all scheduled work when the
module is permanently released. A small clock/scheduler seam underneath uses
browser timers in production and a controllable clock in tests and Storybook.

Acknowledgement at exactly the securing deadline wins over the deadline event,
independently of callback registration order. Deadline handling gives callbacks
due at that same moment a chance to settle before checking the frame. This does
not add sound sequencing or extend the securing budget. A stop that finishes
without a non-empty final Blob removes only its optional final-frame request;
missing regular-frame requests remain unsecured and prevent sealing.

## Event wiring and Vue state

Use typed callbacks rather than a component-level or application-level message
bus. `FrameTiming` reports ticks and deadlines to `CaptureAndDelivery`. The
Section workflow exposes semantic events through `onEvent(event: SectionEvent)`;
explicit application wiring connects them to feedback, diagnostics, and the
application's Capture alerts. The wiring associates Section identity with
events before raising or resolving an alert; Capture-alert identity does not
belong to timing or sound adapters.

Vue renders reactive module state. Local Vue child-to-parent events carry UI
actions such as retry requests; Capture lifecycle events do not need to pass
through the Vue component tree. These known connections do not need dynamic
subscriptions or a message bus. Frame delivery, normal progress cues, and
Capture-alert state remain independently observable processes.

## Semantic feedback

Production logic reports domain events rather than oscillator or audio-file
choices. Its meaningful outcomes include Capture starting, a Transport frame
being secured, a Transport frame becoming overdue, Capture finalization
failing, an Audio Asset sealing attempt failing, an Audio Asset being sealed,
and an Unrequested Capture stop. Capture-started and Healthy heartbeat are
independent one-shot cues; Capture-completed additionally requires a clean
Section outcome. The four unattended-mode failure outcomes described below raise
Capture alerts instead of playing local one-shot Problem cues. Completion is not
merely a report that an Audio Asset reached a sealed state.

For a frame requested at `T1`:

- Securing it by `T1 + secureDeadlineMs` while its Section is actively capturing
  emits the healthy heartbeat immediately.
- If it remains unsecured at the deadline, that individual frame raises one
  `transport-frame-overdue` Capture alert and immediately exposes
  `Saving audio is delayed by 2 seconds.`
- Its visible elapsed time then updates once per second with correct singular
  and plural grammar.
- Each overdue frame owns its line and clears it immediately when secured. Two
  similar lines are acceptable because two frames can be overdue independently.
- Each newly overdue frame is announced politely once, including when another
  frame is already overdue, but routine healthy
  acknowledgements and once-per-second counter changes are not live-region
  announcements.

Do not collapse overdue frames into one delay episode. Each frame that misses
its deadline owns a distinct alert identity. Securing that frame resolves only
its alert. Acknowledgements also emit immediately; several recovering frames
may therefore create overlapping healthy sounds. This burst is useful evidence
of abnormal recovery, and adding a sequencing queue would complicate the timing
model while concealing the independent outcomes.

Healthy heartbeats describe the actively capturing Section. Retained frames from
an older stopped Section update its state and diagnostics when secured but do
not emit healthy sounds. A final partial frame created by a user-requested stop
still receives a securing deadline and raises a Capture alert if overdue, but
its successful acknowledgement is not a heartbeat. Securing either kind of
frame nevertheless resolves that frame's alert by identity. A Healthy heartbeat
never resolves unrelated alerts.

User-requested stop itself has no cue. Successful completion is reported only
when the backend confirms that the Section's Audio Asset is sealed and that
Section has no unresolved Capture integrity failure. Sealing first resolves that
Section's sealing-failure alert, if one is active. Acknowledging an unrequested
stop or finalization failure does not make that Section eligible for Completion;
the condition records that its terminal outcome was not clean until sealing ends
the workflow. An alert from another Section does not suppress a clean completion;
Section outcomes remain independent.

Report confirmed Audio Asset sealing before publishing terminal Section state.
That state can synchronously release a former Account's finished work; cleanup
must not suppress its sealing event or an eligible Completion sound.

Capture finalization failure and Audio Asset sealing failure remain distinct
outcomes with their own visible and retryable details.
They raise `capture-finalization-failed` and `audio-asset-sealing-failed` alerts.
Each failed sealing attempt, including a failed user retry after acknowledgement,
raises the sealing alert again. A later successful seal resolves only the
sealing-failure alert; it does not prove that all recent audio was retained.

## Application-owned Capture alerts

A **Capture alert** exists to bring the user from unattended Capture into an
attentive state when something may be wrong. Its purpose is to make Capture
interruption, unsecured audio, or failed finalization or sealing difficult to
miss so the user looks at the screen and decides what to do. It is not a general
application notification system, and acknowledgement does not perform recovery.

The application owns one collection of Capture alerts. Only these four
conditions enter it:

- `transport-frame-overdue`;
- `capture-finalization-failed`;
- `audio-asset-sealing-failed`; and
- `unrequested-capture-stop`.

Microphone permission or recorder setup failure before Capture begins remains a
local start error. Authentication, health, project-loading, and other application
errors do not become Capture alerts.

Each alert has an identity derived from its Section and kind, additionally using
the frame identity for an overdue frame. Raising an already active identity does
not duplicate it or restart sound. Several identities, including several causes
from one Section, may coexist. Resolving a condition removes only its identity.
An overdue frame resolves when that frame is secured; a sealing failure resolves
when that Audio Asset seals. Capture finalization failure and Unrequested Capture
stop remain until acknowledgement because later sealing cannot prove that all
recent audio was retained.

One application-level Problem sound plays whenever the collection is nonempty.
It starts on the transition from zero alerts to one, continues unchanged while
alerts are added or individually resolved, and stops when the last alert resolves,
the user acknowledges, or the application is disposed. **Acknowledge** clears all
current alert identities without delivery recovery, confirmation sound, or
history. A later failure, including another failed retry with the same logical
identity, raises an alert normally. The application keeps no acknowledgement
tombstones.

Navigation, unrelated interaction, delivery recovery, sign-in or sign-out,
ordinary user Stop, completion, and starting another Capture do not acknowledge
alerts. Alerts remain across application views and Sessions while the page lives.
Application disposal stops playback as a safety consequence, not as domain
acknowledgement. The global surface may list each alert or show a generic summary
while the workflow UI retains specific details; either presentation must make
acknowledgement and playback failure visible.

The unrequested-stop alert says **Capture stopped. Up to 15 seconds of recent
audio may not be saved.** The fixed bound follows the production 5-second interval,
two-frame backlog, and final partial frame; this is why production rejects a
different interval while Storybook remains free to experiment.

The browser sound adapter uses one looping WebAudio source rather than repeated
JavaScript timers or visibility checks. Playback failure remains visible; do not
add production enable-sound, playback-retry, preference, rating, or persistent
override controls. A late audio-failure callback from an earlier activation must
not change a later activation's state.

The production sound vocabulary is Capture-started, Healthy heartbeat, Problem,
and Capture-completed. Problem is the only repeating role. The approved sounds
are Triangle cadence for Healthy heartbeat, Morse Stop for Problem, and Four-note
triangle cadence for Capture-completed, all at 100% relative volume; the
Capture-started cue remains unchanged. Morse Stop preserves the selected
Storybook definition and repeats with no added gap. The Storybook laboratory may
replace sound and environmental adapters for deterministic exploration, but it
uses the same production Capture-alert module, components, and application wiring.

## Consequences

- Capture and delivery are not independent workflows joined by Vue components
  or a shallow forwarding layer. Their shared timing and safety invariants live
  behind one module interface.
- Tests exercise the same interface as production callers with fake browser,
  clock, delivery, and sound adapters. They can deterministically cover drift,
  missing Blobs, acknowledgement races, backlog, independent Sections, and
  Capture-alert lifecycle without patched browser globals.
- Transport frame boundaries become application-requested events rather than a
  health model inferred from HTTP timing. Browser event timing remains an
  implementation fact behind the recorder adapter.
- Five-second frames halve the ordinary unframed-audio window and per-frame
  payload while roughly doubling PUT requests, backend frame rows, hashing, and
  transaction overhead relative to the former 10-second production interval.
  Browser acceptance must retain Chromium and Firefox continuity evidence at
  the production cadence.
- Normal cues and the repeating Problem sound report independent semantic
  outcomes and may overlap. Avoiding overlap is not an invariant and must not
  motivate a cue queue.
- Capture alerts are deliberately limited to four failures that can arise during
  unattended Capture. They are not a generic notification framework.
- Storybook remains a review and exploration workbench. Production tests own
  behavioral acceptance.

## Considered alternatives

- **Three Capture and Delivery modules at application, Project, and Section
  levels:** rejected because only the Section workflow should coordinate
  Capture and delivery. Upper levels own domain objects and access rules.
- **Cancel every deadline after successful delivery:** rejected in favor of
  letting scheduled events fire and ignoring obsolete events. This removes
  cancellation bookkeeping from the frame-securing path while keeping disposal
  cleanup explicit.
- **A component-level or application-level message bus:** rejected because all
  producers and consumers are known. Typed callbacks and explicit wiring make
  event flow and ownership easier to follow.
- **Application tracks the active Capture or relies only on scoped release:**
  rejected because a reference does not prevent concurrent startup, and bugs
  can leave reservations held. MicrophoneAccess enforces exclusive acquisition
  and provides explicit user takeover with reservation identities.
- **Let `MediaRecorder.start(timeslice)` own frame cadence:** rejected because
  browser timeslices are not exact and provide no application timestamp for the
  complete time-to-durability budget.
- **Start the deadline at Blob arrival or HTTP request:** rejected because that
  hides browser, scheduling, queueing, and request-setup delay while the user's
  audio remains unsecured.
- **Keep Capture and delivery as separately coordinated public modules:**
  rejected because callers would need to understand and preserve their shared
  frame-cycle invariants, producing a shallow interface and scattered tests.
- **Maintain an ideal fixed grid after scheduler drift:** rejected because it
  creates short catch-up frames and makes later cycles describe an intended
  timestamp instead of a real request.
- **Retain the 10-second production interval:** rejected after listening review;
  5 seconds is the better Healthy-heartbeat cadence and reduces the amount of
  audio not yet handed to delivery.
- **Scale the backlog threshold to preserve the former wall-clock tolerance:**
  rejected because the user hears Problem after 2 seconds and can decide whether
  to speak through the second frame. Stopping at that boundary bounds additional
  audio at risk.
- **Keep the first retry at 5 seconds:** rejected because it would collide with
  the next frame boundary and lose its useful recovery window. Three seconds
  leaves two seconds for the retry before backlog interruption.
- **Add a separate local-heartbeat failure cue:** rejected because the frame
  cycle's end-to-end securing deadline already covers every actionable stage
  after actual `T1`, while pre-`T1` event-loop delay cannot be reported reliably.
- **Warn once per delay episode and sequence recovery sounds:** rejected because
  frames and Sections have independent outcomes, risk signals should not be
  suppressed, and sequencing adds state without improving truthfulness.
- **Use the normal completion cue immediately on user stop:** rejected because
  accepting a stop command does not prove that final audio is secured or the
  Audio Asset is sealed.
- **Play Completion for every successful seal:** rejected because sealing is a
  technical storage event, while Completion communicates a clean Section outcome.
  A same-Section Capture alert makes that success claim misleading; alerts from
  other Sections remain independent.
- **Put alert ownership in the recording component:** rejected because view
  changes and sign-out must not remove alerts, and Vue components do not own
  application behavior.
- **Acknowledge on navigation, interaction, recovery, or new Capture:** rejected
  because those actions do not prove that the user noticed the Capture problem.
  Only the explicit button acknowledges it.
- **Play one loop per cause or retain separate Problem and stop-alarm systems:**
  rejected because the shared goal is attention. One ID-based collection keeps
  causes independently resolvable without producing competing Problem loops.
- **Expose production timing and sound experiments through `localStorage`:**
  rejected because hidden persistent state makes safety behavior difficult to
  reproduce and diagnose; explicit configuration and Storybook Controls own
  those variations.
