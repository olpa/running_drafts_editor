# Run the Storybook workbench

From `web/`, install dependencies and start Storybook:

```sh
npm install
npm run storybook
```

Storybook opens `http://localhost:6006`. Use its Controls and Accessibility
panels for interactive inspection. To run without opening a browser:

```sh
npm run storybook -- --no-open
```

For a remote machine, forward the loopback port:

```sh
ssh -N -L 6006:127.0.0.1:6006 user@remote-host
```

Open `http://127.0.0.1:6006` locally. Use another local port when 6006 is
occupied. Press Ctrl+C in the Storybook terminal to stop the workbench.

## Capture Scenario Workbench

Capture stories make production states involving timing, delivery, recovery,
and failure repeatable without a microphone, backend, credentials, or saved
Project data. They are grouped by healthy completion, delivery and recovery,
failures, and hard stops. Stories render production modules and Vue components
over typed in-memory adapters; unexpected operations fail visibly and never
reach an external service.

Opening a story does nothing. Read its review note, then activate **Play
scenario**. Play prepares browser audio, performs the fixed arranged production
actions, and advances injected time. Long setup is fast-forwarded, then the last
one or two scheduler frames play at 2× review speed so the transition remains
visible. The resulting state remains visible.
Production controls are inactive in these arranged reviews, except
**Acknowledge**, which can stop a repeating Problem sound.

After playback finishes, **Replay scenario** constructs fresh scheduler,
adapters, Session and Capture state, pending work, alerts, and sound. The
always-visible bounded activity log keeps numbered history across Replay for
comparison. Changing/remounting the story or refreshing the page starts a fresh
log.

### Reproducible review

Open **Capture → Delivery delay and recovery → Backlog Stop After Two Overdue
Frames** and activate **Play scenario**. Inspect the two identified delayed-frame
states, arranged recovery, retained Unrequested Capture stop alert, and one
Problem loop. **Acknowledge** stops the loop and clears the current alert.
Activate **Replay scenario** to reconstruct the original state and compare the
new numbered activity with the retained history.

## Sound configuration

Open **Capture → Sound configuration → Configure Sounds**. Select any retained
candidate, inspect its source and license, audition it with the local repetition
controls, and assign it independently to Capture-started, Healthy heartbeat,
Allowance warning, Problem, or Capture-completed.

Assignments affect Capture stories for the current Storybook page lifetime and
reset to production defaults on refresh. Audition repetition never changes
production cue behavior. Capture stories continue to use production
`CuePlayer`; production modules decide when cues start, repeat, overlap, stop,
or require acknowledgement. A Capture story shows a short assignment summary
only while overrides are active.

Retain exact source, author, license, and attribution data when maintaining the
candidate catalog. Bundled Kenney assets retain their notices under
`src/recording/sound-lab/assets/`.

## Authoring Capture scenarios

Use standard typed Storybook CSF stories and the shared workbench. Do not create
a scenario registry, authoring DSL, custom Storybook control panel, or a second
implementation of production behavior. Supply environmental outcomes through
the same typed injection seams as production. Keep each arranged sequence only
as general as its current scenario requires.

Every arranged story supplies four distinct semantic review-note fields:

- **Initial condition**
- **Play scenario**
- **Inspect**
- **Interaction**

Keep these fields as separate semantic markup through the shared review-note
module; do not collapse them into one prose string. This lets presentation move
without parsing or rewriting story text.

Log scenario start and Replay, arranged actions, adapter requests and outcomes,
semantic cue starts and stops, Capture events, and significant public module
outcomes. Do not log private reactive mutations. Focused production tests remain
the acceptance evidence; Storybook supports implementation and human review.
