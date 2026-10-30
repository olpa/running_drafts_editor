---
status: accepted
---

# Use Storybook as a local review workbench

Use Storybook as the local, review-first workbench for user-visible Vue states
that are costly or unreliable to reach in the ordinary application. Stories
render the smallest meaningful production component over real application
modules. They vary environmental conditions through typed, in-memory adapters
at the same injection seams as production; reviewers exercise product behavior
through the real UI. Co-locate stories, harnesses, and fake adapters with their
owning feature, and extract shared story support only after another feature
needs it.

Storybook is development tooling rather than a product surface or acceptance
framework. Keep it out of the production application, build, image, deployment,
and navigation. Focused production tests remain with their owning features, and
external browser acceptance remains outside Storybook. Provide the accessibility
addon for interactive inspection without making its panel output required review
evidence.

Fake adapters fail visibly on unexpected requests and never fall through to a
live service. Controls change environmental inputs; real UI actions trigger
product behavior. A story may declare a necessary live browser resource when it
is visibly marked and its prerequisites are documented, but it does not replace
a reasonable deterministic fake. Real waits may last at most three seconds;
longer behavior advances through an injected clock or scheduler rather than
patched browser globals.

User-visible frontend work should normally include a reproducible **Manual
review** handoff when ready for human review. Give ordinary application steps
when the state is quick and repeatable to reach. Add or update a story when the
state or transition depends on timing, failure, concurrency, permissions,
destructive setup, or substantial data or manual preparation. A Storybook
handoff identifies the startup command, exact story, reviewer actions, and
expected visible outcome. This is a review reminder rather than a completion
gate; omit it when it would add little review value.

## Considered alternatives

- A custom scenario registry or control panel would duplicate Storybook's
  established component, Controls, and accessibility tooling.
- Stories built from replacement view models would be easier to stage but would
  bypass the application paths under review.
- Publishing Storybook or running it as an acceptance suite would add deployment
  and test surfaces without replacing focused production or external browser
  tests.
