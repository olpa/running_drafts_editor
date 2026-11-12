---
status: accepted
---

# Route durable Project views, not Capture

Give the durable Project collection and each Project stable history URLs at
`/projects` and `/projects/{projectId}`. Keep Capture as application-lifetime
activity in a modal over the current Project route: it has no URL or history
entry, and the router must neither reconstruct it nor own its safety state.
This lets Project links survive refresh and move between authenticated browsers
without making transient, memory-only Capture look recoverable from navigation.

Vue Router owns URL matching, browser history, scroll, and navigation guards.
Authentication, Project reads, Capture, and retained audio remain owned by their
application modules; the current route is the only list/detail destination
state. A route change therefore refreshes authoritative Project data but never
creates, binds, disposes, or restores Capture state.

## Considered alternatives

- A `/record` route would give transient Capture a durable-looking address and
  allow Back, Forward, refresh, or direct load to imply restoration that the
  page-lifetime audio model cannot provide.
- Mirroring the route in a component or Project-module view selector would
  create two navigation authorities that can disagree with browser history.
- Encoding mutable Project titles in URLs would make rename affect identity;
  Project IDs are the stable opaque route identity instead.
