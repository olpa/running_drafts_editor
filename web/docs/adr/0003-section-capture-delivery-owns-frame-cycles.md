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

## Semantic feedback

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
- The initial overdue state is announced politely, but routine healthy
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

User-requested stop itself has no cue. Successful completion is reported only
when the backend confirms that the Section's Audio Asset is sealed. Audio Asset
sealing emits completion independently of how Capture stopped, whether an alarm
is active, whether it followed recovery, or whether a newer Section is now
capturing. Independent completion and alarm sounds may overlap.

Capture finalization failure after a user-requested stop and Audio Asset closing
failure remain distinct domain outcomes with their own visible and retryable
details. They map once to the shared problem sound rather than to the terminal
alarm. Silence after a user stop is therefore not mistaken for confirmed
durability, while a routine stop does not create an alarm whose purpose is to
warn that the user may still be speaking.

## Application-owned alarm

An **Unrequested Capture stop** is any Capture end not explicitly requested by
the user, including automatic limits and failures. Section Capture and Delivery
emits that fact upward. The application owns alarm activation, repeated sound,
presentation, and acknowledgement because the alert must remain present outside
the recording view and without an active Application Session.

There is one application alarm, not an alarm queue. A further Unrequested
Capture stop while it is active is recorded diagnostically but does not restart
or enqueue another alarm. The application renders a thick, prominent,
non-modal bar at its top with `role="alert"` and fixed copy:

> Capture stopped. Up to 30 seconds of recent audio may not be saved.

The event carries neither a duration bound nor formatted presentation text. The
message remains fixed and is deliberately not coupled to timing or backlog
configuration through runtime validation or a refactoring guard.

The alarm sound continues in a repeating or continuous pattern until the user
clicks its explicit **Acknowledge** button. Navigation, unrelated interaction,
delivery recovery, sign-in or sign-out, and starting another Capture do not
acknowledge it. The button receives no automatic focus, performs no recovery,
and produces no confirmation sound; acknowledgement stops playback and removes
the bar. The alarm remains active across application views, background tabs,
and sign-out while the page lives. Application disposal stops playback as a
safety consequence, not as domain acknowledgement.

The exact alarm pattern and final sounds are product-design variables. Initial
production behavior maps existing start, health, delayed, falling-stop, and
interrupted sounds to start, heartbeat, problem, Audio Asset sealed, and
repeating alarm respectively. A Storybook sound laboratory may replace the
feedback adapter and timing values for deterministic exploration, but it uses
the same production module and semantic events. Production sound preferences,
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
