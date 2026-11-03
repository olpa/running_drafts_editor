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
Storybook laboratory use the same seams. Requirements and acceptance remain
in [issue #111](https://github.com/olpa/running_drafts_editor/issues/111).

## Ownership hierarchy

Use Application -> Project(s) -> Section(s) -> `CaptureAndDelivery`. Only the
lowest module coordinates Capture and delivery:

- Application owns application lifetime, Account and Application Session rules,
  and the separate global stop alarm.
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
user-requested stop and does not raise a new unrequested-stop alarm. It also
does not acknowledge an already active alarm.

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
10 seconds runs at 10.4 seconds, that actual execution becomes `T1`, and the next
boundary is 20.4 seconds. Do not create a short catch-up frame. Scheduler
lateness before actual `T1` is neither a user-visible failure nor part of the
securing deadline; when JavaScript cannot run, the application cannot reliably
report the delay in real time. Capture duration remains based on its independent
monotonic start time.

A requested frame whose Blob has not arrived is still an unsecured frame. It
uses the same deadline and backlog rules as a frame stalled in HTTP delivery;
there is no separate browser-frame timeout or user cue. Production initially
uses a 10-second interval and a 1000 ms securing deadline, with supported
deadline configuration limited to 500–1500 ms. Production timing is explicit
validated configuration, never a persistent `localStorage` override.

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
The timing module knows nothing about Blobs, HTTP delivery, or alarms.

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
application alarm. In particular, only `unrequestedCaptureStopped` activates the
global alarm. A frame deadline is not itself a global-alarm event.

Vue renders reactive module state. Local Vue child-to-parent events carry UI
actions such as retry requests; Capture lifecycle events do not need to pass
through the Vue component tree. These known connections do not need dynamic
subscriptions or a message bus. Frame delivery, completion feedback, and the
application alarm remain independent processes.

## Semantic feedback

This section records the production baseline established by #111. The repeating
Problem experiment is tracked in [#110](https://github.com/olpa/running_drafts_editor/issues/110),
and its production implementation is tracked in
[#112](https://github.com/olpa/running_drafts_editor/issues/112). Its temporary
Storybook feedback logic does not replace the production decision below. Update
this ADR when that behavior is promoted, and replace the temporary laboratory
logic with production code and wiring as required by ADR 0002.

Production logic reports domain events rather than oscillator or audio-file
choices. Its meaningful outcomes include Capture starting, a Transport frame
being secured, a Transport frame becoming overdue, Capture finalization
failing, an Audio Asset close failing, an Audio Asset being sealed, and an
Unrequested Capture stop. A feedback adapter maps those outcomes to the smaller
audible vocabulary of start, healthy heartbeat, problem, successful completion,
and terminal alarm.

For a frame requested at `T1`:

- Securing it by `T1 + secureDeadlineMs` while its Section is actively capturing
  emits the healthy heartbeat immediately.
- If it remains unsecured at the deadline, that individual frame emits one
  problem sound and immediately exposes `Saving audio is delayed.`
- If another deadline interval passes, its visible line becomes
  `Saving audio is delayed by 2 seconds.` and then updates visually once per
  second with correct singular and plural grammar.
- Each overdue frame owns its line and clears it immediately when secured. Two
  similar lines are acceptable because two frames can be overdue independently.
- Each newly overdue frame is announced politely once, including when another
  frame is already overdue, but routine healthy
  acknowledgements and once-per-second counter changes are not live-region
  announcements.

Do not collapse overdue frames into one delay episode. Each frame that misses
its deadline emits its own problem sound. Acknowledgements also emit immediately;
several recovering frames may therefore create overlapping healthy sounds. This
burst is useful evidence of abnormal recovery, and adding a sequencing queue
would complicate the timing model while concealing the independent outcomes.

Healthy heartbeats describe the actively capturing Section. Retained frames from
an older stopped Section update its state and diagnostics when secured but do
not emit healthy sounds. A final partial frame created by a user-requested stop
still receives a securing deadline and problem feedback if overdue, but its
successful acknowledgement is not a heartbeat.
Final partial frames produced after an Unrequested Capture stop follow the same
deadline and problem-feedback rules. Delivery feedback remains independent of
the application alarm, so a delayed final frame can sound a problem cue while
the alarm is sounding.

User-requested stop itself has no cue. Successful completion is reported only
when the backend confirms that the Section's Audio Asset is sealed. Audio Asset
sealing emits completion independently of how Capture stopped, whether an alarm
is active, whether it followed recovery, or whether a newer Section is now
capturing. Independent completion and alarm sounds may overlap.

Report confirmed Audio Asset sealing before publishing terminal Section state.
That state can synchronously release a former Account's finished work; cleanup
must not suppress its completion event or sound.

Capture finalization failure after a user-requested stop and Audio Asset closing
failure remain distinct domain outcomes with their own visible and retryable
details. They map once to the shared problem sound rather than to the terminal
alarm. Silence after a user stop is therefore not mistaken for confirmed
durability, while a routine stop does not create an alarm whose purpose is to
warn that the user may still be speaking.
Each failed Audio Asset sealing attempt, including a failed user retry, is a new
failure event and emits the problem sound once.

## Application-owned alarm

An **Unrequested Capture stop** is any Capture end not explicitly requested by
the user, including automatic limits and failures. Section Capture and Delivery
emits that fact upward. The application owns alarm activation, repeated sound,
presentation, and acknowledgement because the alert must remain present outside
the recording view and without an active Application Session.
Microphone permission or recorder setup failure before Capture begins is a
local start error, not an Unrequested Capture stop, and does not raise the alarm.

There is one application alarm, not an alarm queue. A further Unrequested
Capture stop while it is active is recorded diagnostically but does not restart
or enqueue another alarm. The application renders a thick, prominent,
non-modal bar at its top with `role="alert"` and fixed copy:

> Capture stopped. Up to 30 seconds of recent audio may not be saved.

The event carries neither a duration bound nor formatted presentation text. The
message remains fixed and is deliberately not coupled to timing or backlog
configuration through runtime validation or a refactoring guard.

The baseline alarm sound loops continuously until the user
clicks its explicit **Acknowledge** button. Navigation, unrelated interaction,
delivery recovery, sign-in or sign-out, and starting another Capture do not
acknowledge it. The button receives no automatic focus, performs no recovery,
and produces no confirmation sound; acknowledgement stops playback and removes
the bar. The alarm remains active across application views, background tabs,
and sign-out while the page lives. Application disposal stops playback as a
safety consequence, not as domain acknowledgement.

If alarm playback is blocked or fails, the bar also shows
`Alarm sound is unavailable.` The alarm remains active until explicit
acknowledgement. Do not add an enable-sound or playback-retry button: the visible
warning keeps the failure understandable without another recovery interaction.

The browser sound adapter uses a looping WebAudio source rather than repeated
JavaScript timers or visibility checks. Playback failure is visible; application
disposal stops the source. A late audio-failure callback from a previous alarm
activation must not change a later activation's state.

The final sounds and experimental alarm patterns are product-design variables.
Initial production behavior maps existing start, health, delayed, falling-stop, and
interrupted sounds to start, heartbeat, problem, Audio Asset sealed, and
alarm respectively. The baseline interrupted cue loops continuously with no
added pause between repetitions until acknowledgement. A Storybook sound
laboratory may replace the feedback adapter and timing values for deterministic
exploration, but it uses the same production module and semantic events.
Production sound preferences,
ratings, and persisted experimental overrides are separate concerns.

## Consequences

- Capture and delivery are not independent workflows joined by Vue components
  or a shallow forwarding layer. Their shared timing and safety invariants live
  behind one module interface.
- Tests exercise the same interface as production callers with fake browser,
  clock, delivery, and sound adapters. They can deterministically cover drift,
  missing Blobs, acknowledgement races, backlog, independent Sections, and
  alarm lifecycle without patched browser globals.
- Transport frame boundaries become application-requested events rather than a
  health model inferred from HTTP timing. Browser event timing remains an
  implementation fact behind the recorder adapter.
- Sounds report independent semantic outcomes and may overlap. Avoiding overlap
  is not an invariant and must not motivate a cue queue.
- The application alarm is deliberately specific to Unrequested Capture stops.
  It is not a generic notification framework.
- Storybook remains a review and exploration workbench. Production tests own
  behavioral acceptance, and promotion of an approved sound palette is separate
  work.

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
- **Add a separate local-heartbeat failure cue:** rejected because the frame
  cycle's end-to-end securing deadline already covers every actionable stage
  after actual `T1`, while pre-`T1` event-loop delay cannot be reported reliably.
- **Warn once per delay episode and sequence recovery sounds:** rejected because
  frames and Sections have independent outcomes, risk signals should not be
  suppressed, and sequencing adds state without improving truthfulness.
- **Use the normal completion cue immediately on user stop:** rejected because
  accepting a stop command does not prove that final audio is secured or the
  Audio Asset is sealed.
- **Suppress completion while a terminal alarm is active:** rejected because
  Audio Asset sealing and acknowledgement of an Unrequested Capture stop are
  independent facts.
- **Put the terminal alarm in the recording component:** rejected because view
  changes and sign-out must not remove it, and Vue components do not own
  application behavior.
- **Acknowledge on navigation, interaction, recovery, or new Capture:** rejected
  because those actions do not prove that the user understood Capture had
  stopped. Only the explicit button acknowledges it.
- **Queue alarms or include technical cause and risk duration in the event:**
  rejected because one active alarm already fulfils the attention goal, while
  recording state and diagnostics own technical and recovery detail.
- **Derive or guard the fixed 30-second copy from configuration:** deliberately
  rejected in favor of stable application wording and a minimal stop event.
- **Expose production timing and sound experiments through `localStorage`:**
  rejected because hidden persistent state makes safety behavior difficult to
  reproduce and diagnose; explicit configuration and Storybook Controls own
  those variations.
