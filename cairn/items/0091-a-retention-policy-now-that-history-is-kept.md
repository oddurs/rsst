---
id: 91
title: A retention policy, now that history is kept
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
part_of:
- 90
release: v1.9
effort: m
area: storage
---

## Problem

Once `0090` keeps everything, the database grows without limit. Somebody reading two hundred feeds for five years has a large file and no way to say what they want kept.

## Proposal

A retention policy the reader sets: keep everything, keep N per feed, keep the last N months, and never discard something starred, tagged or queued whatever else the policy says.

It should run as tidying rather than as a stall — `0075` established that a write between two frames is felt — and it should say what it removed rather than doing it silently.

## Acceptance criteria

- [ ] A policy can be set globally and per feed
- [ ] Starred, tagged and queued entries are never removed by a policy
- [ ] Tidying does not block the interface
- [ ] The database gets smaller on disk afterwards, not just emptier
- [ ] A dry run says what would go before anything does
