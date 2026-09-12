---
id: 65
title: Remember a permanent redirect
type: feature
status: backlog
created: 2026-09-12
updated: 2026-09-12
priority: p2
release: v1.5
effort: s
area: net
---

## Problem

`reqwest` follows redirects, so a feed that has moved still works — and that is the whole problem. A permanent redirect is a server saying "stop asking here", and rsst asks again every refresh, forever, paying the extra round trip each time.

`http://blog.rust-lang.org/feed.xml` fetches ten entries today and will still be redirecting in a year.

## Proposal

Notice when the response came from somewhere else, and when the redirect was permanent (301 or 308), record the new URL so later fetches go straight there. Tell the reader it happened; offer to update the config, since that is where the stale URL actually lives.

## Acceptance criteria

- [ ] A 301 or 308 updates the stored URL
- [ ] A 302 or 307 does not, being temporary by definition
- [ ] Read and starred state survives the move
- [ ] The reader is told, and the config can be updated from the offer
