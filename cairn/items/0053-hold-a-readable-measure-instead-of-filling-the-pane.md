---
id: 53
title: Hold a readable measure instead of filling the pane
type: feature
status: backlog
created: 2026-09-12
updated: 2026-09-12
priority: p0
effort: m
area: ui
release: v1.3
---

## Problem

Body text is wrapped to whatever the pane happens to be. On a 150-column terminal that is a 126-character line:

```
This is sentence 1 of a long paragraph that will show how the measure behaves on a wide terminal. This is sentence 2 of a long
paragraph that will show how the measure behaves on a wide terminal. This is sentence 3 of a long paragraph that will show how
```

Long lines are hard to read for a mechanical reason: at the end of one the eye has to travel back and find the start of the next, and the further it travels the more often it lands on the wrong line. Typography has converged on roughly 45–75 characters for this, and every reading surface that cares — books, Instapaper, reader modes — holds a measure rather than filling the window.

Buying a wider monitor currently makes rsst harder to read, which is the wrong way round.

## Proposal

Cap the measure for prose and centre it in the pane. Code keeps the full width, because its line breaks are the author's and truncating them loses meaning. Configurable, since the right measure depends on the font someone reads in.

## Acceptance criteria

- [ ] Prose is wrapped to a readable measure rather than the pane width
- [ ] The measure is centred, so a wide pane does not leave text against one edge
- [ ] Code blocks still use the full width available
- [ ] The measure is configurable, and can be turned off
- [ ] A narrow pane is unaffected — nothing is indented off the screen
