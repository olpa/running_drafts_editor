---
status: accepted
---

# Build the web UI as Vue components over application modules

The browser application is a Vue 3 application written with the Composition
API. All state and behavior live in factory-created application modules
(`createSession`, `createRecording`, `createProjects`, `createHealth`), not in
components. Each module takes its external gateways (HTTP, microphone, clock,
page location) as plain arguments. One composition root, `src/app/application.ts`,
creates the production adapters and modules and provides the finished modules to
components through typed `InjectionKey`s. Components inject modules; they never
call HTTP or browser APIs directly.

Recording state lives in an application-lifetime module outside every component.
Pending audio exists only in browser memory, so no view change, unmount, or
sign-out may drop it. Components are stateless views of module state and may be
mounted and unmounted freely. Modules are plain TypeScript with Vue refs, so they
run in tests without a Vue app, and a test or Storybook story can replace a whole
module with a stub through the same injection key.

There is no Pinia store. Reconsider it when several independent Vue branches must
change or react to the same long-lived state, when forwarding layers appear, or
when duplicated state needs synchronising.

Source is organised by feature (`session/`, `recording/`, `projects/`, `health/`,
`app/`). Code moves to `shared/` only when a second feature needs it.

## Considered alternatives

- **Keep Custom Elements:** rejected because the project would run two UI
  styles, and Storybook (#105) needs a component framework.
- **Pinia for application state:** rejected for now because the modules have one
  consumer branch each and explicit interfaces; a store would add a second place
  for state to live.
- **Recording state owned by its panel component:** rejected because unmounting
  the panel would lose undelivered audio, and data safety would depend on a
  template detail.
- **Modules that call `inject()` for their gateways:** rejected because they would
  run only inside a component, which makes the risky recording logic harder to
  test and to reuse.
- **Layered folders (`components/`, `composables/`, `gateways/`):** rejected
  because a change to one feature would touch four folders, and a feature's stories
  and stubs would not sit together.
