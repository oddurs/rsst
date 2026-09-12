---
id: 66
title: Be polite to one host, not just to the network
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p2
release: v1.5
effort: m
area: net
---

## Problem

The limiter counts requests across the whole network — eight at a time, wherever they are going. Eight simultaneous requests to one host is not eight lanes of traffic, it is a small burst at one server, and it is how a feed reader earns a 429.

Someone subscribed to fifteen feeds on one site hits it fifteen times as fast as the limiter allows.

## Proposal

Limit per host as well as globally, with a small per-host ceiling, so a hundred feeds across fifty hosts still fetch quickly while no single host sees a burst.

## Acceptance criteria

- [x] Concurrent requests to one host are capped below the global limit
- [x] Feeds on different hosts still fetch concurrently
- [x] A test asserts the observed per-host peak
