---
id: 95
title: A view of how each feed is behaving
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p3
release: v1.9
effort: m
area: ui
---

## Problem

A feed shows `!` when its last fetch failed and nothing otherwise. There is no way to see when a feed was last reached, how often it fails, whether it has published anything this year, or which of two hundred subscriptions are dead.

The database already knows most of it: `fetched_at`, `retry_after`, the validators, and the entries themselves.

## Proposal

A view of the subscription list as a list rather than as a tree: last fetched, last published, entries a week, failures, and whether the feed has gone quiet. Enough to answer "what should I unsubscribe from", which is the question two hundred feeds eventually produce.

Pairs with `0077`: a list of dead feeds is only useful if something can be done from it.

## Acceptance criteria

- [ ] A view listing every feed with when it was last reached and last published
- [ ] Feeds that have failed repeatedly, or gone quiet, are distinguishable at a glance
- [ ] It can be sorted by each column
- [ ] A feed can be unsubscribed from directly
- [ ] It reads from what is already stored rather than new bookkeeping where possible
