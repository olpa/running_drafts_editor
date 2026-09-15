---
status: accepted
---

# Derive cheap values from current chunks

Persist stable transcription identities, not values that can be calculated
cheaply from the current chunk arrangement. Issue grouping, ordering, displayed
addresses, and resolved-list positions are short-lived projections calculated
when a command needs them; a projection must not survive a project mutation.
This keeps stored state minimal and prevents paragraph edits from making
derived positions stale. A future cache must be tied to an explicit project
revision and invalidated by every project mutation.
