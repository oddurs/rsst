---
id: 64
title: Retry a transient failure instead of giving up
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p1
release: v1.5
effort: m
area: net
part_of:
- 63
---

## Problem

One failed request marks a feed failed until the reader presses `r`. A dropped packet at the moment of launch is indistinguishable from a feed that no longer exists — both show `!` and stay that way.

On a laptop that just woke up, or a train, that is most of the feed list.

## Proposal

Retry what is worth retrying, with exponential backoff and jitter, a small number of times. Never retry what cannot succeed: a 404, a 410, a document that is not a feed.

Needs `0063` first, to know which is which.

## Acceptance criteria

- [x] A transient failure is retried with growing delays
- [x] A permanent failure is not retried at all
- [x] Retries are bounded in both count and total time
- [x] Backoff is jittered, so a hundred feeds on one host do not retry in lockstep
- [x] The reader shows that a retry is happening rather than looking stalled

## 2026-09-12

Jitter comes from an FNV hash of the feed URL rather than a random source: no new dependency, deterministic so it is testable, and it spreads feeds apart, which is the only property that matters. Also found that a bare 503 was being treated as rate limiting and costing the default five-minute backoff; it is now a retryable server error, while a 503 or 429 that names a time is still honoured.
