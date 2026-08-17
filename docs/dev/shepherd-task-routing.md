# Routing work to a shepherd: threads are tasks, comments are context

A coordinator-level fact learned the hard way during the ROB01 epic, recorded
here so the next coordinator finds it before repeating it.

## The finding

**A shepherd's work queue is the PR's open review threads.** A shepherd
resuming on a PR asks one question — what threads are open? — and triages
those. Coordinator instructions posted as PR-level *comments* are not in that
queue: a shepherd resuming on a PR that reports `threads: 0 open` correctly
concludes it has nothing to triage and exits.

This happened three times during ROB01 — WS02's ADR renumber, and twice on
WS05 — and each time it read as an agent ignoring instructions when it was
nothing of the kind. The shepherd did exactly its job; the task had been
posted somewhere the job never looks.

## The rule

- **Coordinator work a shepherd must *perform* has to arrive as a review
  thread.** If it is not a thread, it is not in the queue, and it will not be
  done — silently, and correctly so from the shepherd's side.
- **PR comments are the right vehicle for context**: framing, boundaries,
  what not to "fix", why the round is scoped the way it is. A shepherd reads
  them *while* triaging threads. They are the wrong vehicle for a task,
  because reading is all that ever happens to them.

## Why not "fix" the shepherd instead

Making shepherds also scan PR comments for embedded instructions would turn
every piece of prose on a PR into a possible work item, and the shepherd into
a parser of intent. The thread/comment split is the interface working as
designed: threads carry resolvable units of work with a place to answer
fix-or-pushback; comments carry everything that is not that. Route work
accordingly instead of widening the queue.
