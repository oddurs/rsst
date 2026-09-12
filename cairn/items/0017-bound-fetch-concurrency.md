---
id: 17
title: Bound fetch concurrency
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p2
effort: s
area: net
release: v0.2
---

## Problem

`fetch_all` spawns one task per feed with no ceiling. A few hundred feeds means a few hundred simultaneous TLS handshakes, which trips rate limits and can exhaust file descriptors.

## Proposal

Run fetches through a semaphore with a configurable limit, defaulting to something modest like 8.

## Acceptance criteria

- [x] A 200-feed config never exceeds the configured number of in-flight requests.
- [x] The limit is configurable.
- [x] Total refresh time for a large list does not regress noticeably.
