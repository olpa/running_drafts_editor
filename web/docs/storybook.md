# Run the Storybook workbench

From `web/`, install dependencies and start Storybook:

```sh
npm install
npm run storybook
```

Storybook opens `http://localhost:6006`. Use its Controls panel to change a
story's environmental inputs and its Accessibility panel for interactive
inspection.

For a remote machine or agent environment, leave browser opening disabled:

```sh
npm run storybook -- --no-open
```

Forward the remote loopback port from the computer where the browser runs:

```sh
ssh -N -L 6006:127.0.0.1:6006 user@remote-host
```

Then open `http://127.0.0.1:6006` locally. If local port 6006 is occupied, use
another local port, for example `-L 16006:127.0.0.1:6006`, and open that port.
Press Ctrl+C in the Storybook terminal to stop it.

## Capture sound laboratory: Manual review

Open **Capture → Sound laboratory → Sound Palette**. The direct story path is
`/?path=/story/capture-sound-laboratory--sound-palette`.

The laboratory requests no microphone, backend, credentials, or saved Project
data. It renders the production `RecordingPanel` and Capture-alert component over
the production application modules and event wiring. Deterministic adapters
replace microphone, delivery, sealing, clock, and sound output. Unexpected
activity fails visibly rather than reaching a live service.

Click **Enable laboratory sound** before direct audition. In an arranged story,
**Play scenario** enables sound and performs the documented production actions
automatically. Browser audio restrictions or simulated playback failure remain
visible while Capture and alert controls stay usable.

### Production palette

The default **Production** preset uses the production definitions:

| Role | Candidate | Relative volume |
| --- | --- | --- |
| Healthy heartbeat | `triangle-cadence` / Triangle cadence | 100% |
| Problem | `morse-stop` / Morse Stop | 100% |
| Capture completed | `four-note-triangle-cadence` / Four-note triangle cadence | 100% |

Capture-started remains fixed. Morse Stop uses the production 1.18-second buffer
and repeats with no added gap. Production and Storybook import the same promoted
definitions; the laboratory does not keep copies.

The retained Soft and tactile, Glass and bells, Minimal signals, and Kenney
interface presets provide alternative starting points. Every role selector
offers every candidate, grouped as Healthy, Problem, or Completion. The
laboratory keeps no historical production preset.

Master and relative volume, synthesized pitch and duration, and repeating gap
are bounded listening controls. Production fixes master and all relative levels
at 100%, pitch and duration at their definitions, and the Problem gap at zero.
Changing a candidate does not replace an active Problem loop. Click the
production **Acknowledge** action, then audition Problem again.

Direct Healthy and Completion auditions use the one-shot sound adapter. Direct
Problem audition raises a visibly labelled laboratory alert through the real
production `CaptureAlerts` module. The production component owns presentation,
unavailable-sound state, acknowledgement, and repeating playback.

### Capture-alert behavior

The application can hold several identified Capture alerts while playing one
Problem loop. Adding another alert neither stacks nor restarts sound. Resolving
one alert stops sound only when no alert remains. **Acknowledge** clears all
current alerts without performing recovery or removing unresolved workflow
details.

An overdue frame resolves only its own alert when that frame becomes secured. A
Healthy heartbeat does not clear unrelated alerts. A final frame and a frame
from an older stopped Section resolve their alerts even though they emit no
Healthy heartbeat. Successful Audio Asset sealing resolves its sealing-failure
alert. Capture finalization failure and Unrequested Capture stop require
acknowledgement.

Capture alerts outlive the recording view and Application Session while the page
lives. Storybook timing changes may rebuild its simulated Section environment;
use **Acknowledge** whenever a retained alert is no longer useful to the review.

### Arranged and manual playback

Named scenarios default to automatic review. **Accelerated Healthy Dictation**
defaults to 5× simulated speed; other automatic scenarios start at 1×. **All
Frames Secured** defaults to real time at the production ten-second frame
interval. Sound durations and repetition always use real audio time.

**Pause scenario** freezes simulated Capture and delivery time; repeating Problem
continues. **Resume scenario** continues from the same point. A finished scenario
leaves its final visible state for review. When alerts are active, replay is
labelled **Acknowledge and replay scenario** and explicitly clears them first.

Choose **manual** under **Review clock** for step-by-step inspection. Use the
production Start and Stop controls with **Advance**, **Recover simulated
delivery**, and **Confirm simulated sealing**. Timing or review-clock changes
create a fresh simulated Section while retaining palette choices and numbered
activity history. Playback-speed changes apply to the next clock step without
rebuilding the Section.

The real-time frame interval control from 1–20 seconds is a documented local
exception to ADR 0002's three-second wait limit. Production is visibly marked at
5 seconds. The securing deadline is bounded from 500–2000 ms and defaults to the
production 2000 ms. These Storybook controls do not loosen the deployed
application's fixed 5-second interval.

### Reproducible review paths

1. Open **Sound Palette**, enable sound, and compare the three Production roles.
   For Problem, acknowledge before selecting and auditioning another candidate.
2. Open **Single Overdue Frame** and play it. Expect Morse Stop and a global
   Capture alert at the deadline. Securing that frame removes the alert and plays
   Triangle cadence; Four-note triangle cadence follows at least two seconds later.
3. Open **Two Overdue Frames**. Expect independent delay lines and alert IDs with
   one Problem loop. Recovery removes frame alerts independently while the
   Unrequested Capture stop remains for acknowledgement.
4. Open **Final Frame Delayed**. Expect its overdue alert to resolve when the
   final frame secures, without a Healthy heartbeat.
5. Open **Sealing During Capture Alert**. Expect backend sealing while the
   Unrequested Capture stop alert and Problem loop remain active, without the
   Completion cue.
6. Open **Audio Playback Unavailable**. Expect the global component to report
   **Problem sound is unavailable.** Acknowledge must remain usable.

Speak continuously during real-time review. Compare reassurance, urgency,
distinguishability, and distraction through speakers and headphones when
available. Record findings in the GitHub issue that requested the current sound
experiment; Storybook stores none.

### Scenario catalog

| Story | Arranged listening sequence |
| --- | --- |
| Sound Palette | Direct auditions; automatic mode can play a healthy Capture sequence. |
| All Frames Secured | Three frames secure at production cadence, followed by Stop and sealing. |
| Accelerated Healthy Dictation | The healthy sequence at 5× simulated speed by default. |
| Single Overdue Frame | One identified overdue alert, recovery, then Completion after a two-second listening gap. |
| Two Overdue Frames | Two delay lines, backlog stop, recovery, and persistent unrequested-stop alert. |
| Recovery Burst | Healthy cues arrive 40 simulated ms apart; Completion waits at least two seconds. |
| User Stop and Successful Seal | Final audio secures before explicit sealing confirmation. |
| Final Frame Delayed | An overdue final frame resolves without a Healthy heartbeat. |
| Unrequested Duration Stop | A simulated backend duration rejection raises a Capture alert and retains pending audio. |
| Capture Failure | Recorder failure raises unrequested-stop and finalization alerts. |
| Sealing During Capture Alert | Sealing succeeds, but the same Section's alert suppresses Completion. |
| Older Section Completes | An older Section seals during a newer Capture. |
| Repeated Stop Events | Two Sections add alerts to one Problem loop. |
| Audio Playback Unavailable | Visible alert behavior remains usable without sound. |

Use **Fail next Capture finalization** before Stop or **Fail next sealing
attempt** before sealing confirmation to inspect those identified alerts and
retry behavior. These controls vary deterministic environmental inputs; they do
not implement product decisions.

### Candidate maintenance and provenance

The laboratory is co-located under `src/recording/sound-lab/`. Add an alternative
in `candidates.ts` with a stable acoustic ID and name, role, family, source,
author, exact license, and attribution requirement. Production definitions live
in `src/recording/sound-definitions.ts` and are imported into the catalog.

Synthesized alternatives and decoded assets share an RMS target of 0.028 with a
0.08 sample-peak cap before bounded master and relative gain. Silence is not
amplified. Fixed definitions preserve their production envelopes and
frequencies. Downloaded audio is mixed to mono and normalized in memory without
changing source files.

The twelve downloaded candidates come from Kenney's
[Interface Sounds](https://kenney.nl/assets/interface-sounds), licensed under
[CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/).
Attribution is optional; suggested credit is **Kenney (www.kenney.nl)**. The
original notice remains in `assets/KENNEY-LICENSE.txt`; `assets/SOURCE.txt`
records the pinned download mirror.
