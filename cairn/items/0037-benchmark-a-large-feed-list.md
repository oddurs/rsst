---
id: 37
title: Benchmark a large feed list
type: chore
status: done
assignee: Oddur Sigurdsson
created: 2026-09-11
updated: 2026-09-12
priority: p3
effort: m
area: net
release: v1.0
---

## Problem

Nothing has been measured. Behaviour at 500 feeds and 50,000 entries is unknown — a scroll that is smooth at ten feeds may not be.

## Proposal

A benchmark harness over a synthetic corpus, covering startup, refresh and scroll, with the numbers recorded so regressions are visible.

## Acceptance criteria

- [x] Reproducible benchmarks for startup, refresh and scrolling.
- [x] Baseline numbers committed.
- [x] CI flags a significant regression.
