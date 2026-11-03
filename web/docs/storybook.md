# Run the Storybook workbench

From `web/`, install dependencies and start Storybook:

```sh
npm install
npm run storybook
```

Storybook opens `http://localhost:6006` in the local browser. Use its Controls
panel to change a story's environmental inputs and its Accessibility panel for
an interactive accessibility inspection. The upstream
[Controls](https://storybook.js.org/docs/essentials/controls) and
[accessibility](https://storybook.js.org/docs/writing-tests/accessibility-testing)
documentation covers the general interface.

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

Press Ctrl+C in the terminal running Storybook to stop it.

## Capture sound laboratory: Manual review

The current exploration is complete. Retain the entire current
laboratory, including all candidates and experimental behavior. The current
palette remains a pre-selection; production promotion and replacement of
temporary Storybook product logic with production code and wiring belong to
[#112](https://github.com/olpa/running_drafts_editor/issues/112). Keep useful review
infrastructure when doing that cleanup. ADR 0002 records this boundary.

Run `npm run storybook` from `web/`, then open **Capture → Sound laboratory →
Sound Palette**. The direct story path is
`/?path=/story/capture-sound-laboratory--sound-palette`.
In **Sound Palette**, click **Enable laboratory sound** before auditioning.
In each named scenario, click **Play scenario**: it enables sound and runs the
arranged sequence without Advance, recovery, Stop, or sealing clicks. Browser
audio restrictions or missing local audio appear visibly; Capture and its visible
feedback still work.
The laboratory requests no microphone and uses no backend, credentials, or saved
Project data. Downloaded sound files are bundled with the story.

The default **Production candidate** palette pre-selects the current proposal:

| Role | Candidate | Relative volume |
| --- | --- | --- |
| Healthy heartbeat | Gentle rise / Triangle cadence | 100% |
| Problem | Morse Stop (the production interrupted cue) | 100% |
| Capture completed | Triangle finish / Four-note triangle cadence | 100% |
| Unrequested-stop alarm | Morse Stop | 100% |

This is a listening proposal, not a production change. Master volume starts at
100%; the requested 100% relative values remain independently adjustable.

Select **Production baseline** to compare with the current application. Audition any candidate directly,
then compare **Soft and tactile**, **Glass and bells**, **Minimal signals**, and
**Kenney interface**. Role selections override the palette independently. The master
volume applies to all cues, including the fixed production start gesture; relative
volume applies to each tunable role. Pitch and duration affect synthesized
alternatives only. Reset restores the selected palette, its relative levels and
alarm gap, and the laboratory master default of 100%.

Every role selector offers all candidates, grouped by their initially proposed
role. For example, under **Healthy heartbeat**, open **Candidate for healthy
heartbeat** and select **Closing bell** in the **Capture completed** group. Click
**Audition selected sound** to hear it immediately, or Start Capture to hear it
when a Transport frame secures. Catalog audition buttons use the initially
proposed role; use the role selectors to try a sound elsewhere.

### Repeating Problem experiment

**Production candidate** enables **Repeat Problem until a healthy heartbeat or
Acknowledge**. Problem events start one continuous loop, without stacking or
restarting it for further events. A healthy heartbeat clears it before playing;
the global **Acknowledge** action also clears it. Completion and ordinary Start or
Stop do not clear it. A secured final frame or an older stopped Section produces
no healthy heartbeat, so its recovery alone does not stop Problem repetition.

The workbench shows an explicit experiment notice while Problem repetition is
active, with **Acknowledge** available even when there is no unrequested-stop
alarm. If both are active, the production alarm bar's **Acknowledge** button clears
both. Healthy feedback still cannot acknowledge an unrequested-stop alarm.
The stop alarm takes sound priority when different candidates are selected; when
both use the same sound and settings, playback is shared without doubling or
restarting it. This is a preview choice for listening review.

The **Repeating sound gap** control applies to either loop. A zero gap preserves
continuous Morse Stop playback. Both kinds of loop use real audio time even when
the simulation clock advances. Timing rebuilds retain their active state until
healthy feedback or acknowledgement clears the appropriate state.

This experiment deliberately changes the sound adapter's response to production
events. It does not change production modules, per-frame delay UI, delivery,
sealing, or the production alarm. ADR 0003 still describes production's one-shot
Problem feedback. Full production behavior and its ADR/test changes belong to
follow-up #112. Other palette defaults, including **Production baseline**, disable
the experiment; the checkbox can enable it for any palette.

### Listen to an arranged scenario

Open **Single Overdue Frame** and click **Play scenario**. Time advances on its
own. The first frame becomes overdue, Morse Stop sounds, and healthy recovery
stops it and plays Triangle cadence. The arranged user Stop and sealing then
complete the sequence. **Final Frame Delayed** automatically performs Stop,
delayed final delivery, recovery, and sealing; completion leaves the experimental
Problem loop active until **Acknowledge**.

Named scenarios default to **automatic** review. **Accelerated Healthy Dictation**
defaults to 5× simulated speed; other scenarios start at 1×. **All Frames Secured**
defaults to **real-time**, which always uses 1× speed. Change **Automatic playback
speed** in Storybook Controls to compare faster scenarios while retaining their
configured frame interval and securing deadline. Sound durations and repetitions
always keep real audio time.

**Pause scenario** freezes the simulated clock and arranged actions; repeating
alerts continue until **Acknowledge**. **Resume scenario** continues from the same
point. At the end, the clock pauses and the visible final state remains for
review. **Replay scenario** creates a fresh run and preserves the palette and
numbered history. If an alert is active, the button explicitly reads
**Acknowledge and replay scenario**, clearing the alert as part of that action.

The arranged sequence invokes real production actions through the same module
interface as the Start/Stop buttons. This is explicit workbench playback initiated
by the reviewer; production does not automatically perform these user actions.
Manual UI actions and environmental failure controls remain available for
exploration, though they may change the arranged outcome.

Choose **manual** in **Review clock** for step-by-step inspection. Then use the
production **Start**, **Stop current recording to start new**, and **Start new
recording** controls with the Advance, recovery, and sealing buttons. The clock
stays still between advances. This is an inspection path, not the default
listening experience.

Timing or review-clock changes create a fresh, stopped scenario while preserving
sound choices and numbered history. Active alerts remain until acknowledgement
or appropriate healthy feedback. Playback-speed changes apply to the next clock
step without rebuilding the Section. Changing a repeating candidate replaces its
sound immediately. A zero repeating gap means no added pause.

The real-time frame interval of 1–20 seconds is a local exception to ADR 0002's
three-second wait limit. The default and visibly marked production interval is
10 seconds; the production securing deadline is 1000 ms. Manual advancement moves
Capture and delivery through the injected clock, while audio plays in real time.
Fast advancement may overlap cues. It does not replay skipped alarm repetitions.
Speak continuously during real-time review, comparing reassurance, urgency,
distinguishability, and distraction. Try speakers and headphones when available.

Each named scenario includes a description at the top. Click **Play scenario**
once, then listen for its arranged outcome:

| Story | Arranged listening sequence |
| --- | --- |
| Sound Palette | Direct auditions; automatic review can play the healthy Capture sequence. |
| All Frames Secured | Three frames secure at real production cadence, followed by Stop and sealing. |
| Accelerated Healthy Dictation | The same healthy sequence at 5× simulated speed by default. |
| Single Overdue Frame | One delay warning, healthy recovery, then Stop and completion. |
| Two Overdue Frames | Two independent delay lines and the actual backlog stop/alarm, then recovery. |
| Recovery Burst | Recovery just before the next request produces healthy cues 40 simulated ms apart, then Stop and sealing. |
| User Stop and Successful Seal | A short Capture stops; final audio secures before explicit simulated sealing confirmation. |
| Final Frame Delayed | Stop, overdue final frame, delivery recovery, and independent completion. |
| Unrequested Duration Stop | Capture reaches its duration limit; the global alarm remains for acknowledgement. |
| Capture Failure | Recorder failure, Section cause, independent global alarm, and delivery recovery. |
| Completion During Alarm | The backend confirms sealing after the duration stop while the alarm remains active. |
| Older Section Completes | Section 1 stops; Section 2 starts before the older Section seals, then the newer Section stops and seals. |
| Repeated Stop Events | Two successive Sections reach their duration limits without acknowledging the first alarm. |
| Audio Playback Unavailable | Capture reaches its duration limit while the visible alarm reports unavailable sound. |

Use **Fail next Capture finalization** before Stop, or **Fail next sealing attempt**
before confirming a seal, to review their distinct visible failure details and
shared problem cue. Production retry buttons remain available. **Recover simulated
delivery** releases held responses and permits subsequent simulated delivery;
**Confirm simulated sealing** releases held and future sealing confirmations.
These are environmental controls, not additional product recovery actions.

Record findings in [issue #110](https://github.com/olpa/running_drafts_editor/issues/110)
using candidate IDs from the log, palette/tuning, story, clock mode, and playback
equipment. The workbench saves no findings. The initial broad catalog remains
until listening review identifies the shortlist and alternatives. Production
palette promotion remains #112. Storybook is review infrastructure; focused
production tests remain the behavioral acceptance evidence.

### Candidate maintenance and provenance

The laboratory is co-located under `src/recording/sound-lab/`. Add a candidate in
`candidates.ts` using a small note definition or a bundled asset, its stable ID,
role, acoustic family, source, author, exact license, and attribution requirement.
Optionally refer to that ID in a palette. No registry, upload system, or persistent
preferences are needed. Production imports none of these files.

Synthesized alternatives and decoded assets share an RMS target of 0.028 with a
0.08 sample-peak cap before bounded master and relative gain. Silence is not
amplified. Baseline definitions preserve production tone envelopes and frequencies;
the interrupted buffer remains 1.18 seconds with no added gap. The fixed start
uses the production rising gesture. Downloaded audio is mixed to mono and normalized
in memory without changing the source files. Loudness matching is a starting point
for listening review, not a claim of identical perceived loudness.

The twelve downloaded candidates come from Kenney's
[Interface Sounds](https://kenney.nl/assets/interface-sounds), licensed under
[CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/).
Attribution is optional; suggested credit is **Kenney (www.kenney.nl)**. Each
candidate displays this provenance. The pack's original notice is retained in
`assets/KENNEY-LICENSE.txt`; `assets/SOURCE.txt` records the pinned download mirror.
The original site denied direct downloads during implementation, so the source
files were copied unchanged from that mirror after checking the author's license.
