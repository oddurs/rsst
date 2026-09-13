---
id: 88
title: A read-later queue
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
part_of:
- 87
---

## Problem

There is no queue. "I will read this later" has to be expressed as a star, which is also "I want to keep this" — two different intentions sharing one mark, so neither list is trustworthy.

## Proposal

A read-later queue with an order the reader controls: add to it, work through it, and leave it when it is empty. Distinct from starring, which is about keeping.

Once `0087` exists this could be a tag with a blessed name rather than a third mechanism, and that is probably the right answer — one idea, one implementation, a shortcut for the common case.

## Acceptance criteria

- [ ] An entry can be queued and unqueued from the keyboard
- [ ] The queue is a view in the sidebar with a count
- [ ] Reading an entry does not silently remove it from the queue
- [ ] The queue has an order, and it is stable
- [ ] Built on entry tags rather than beside them, unless there is a reason not to
