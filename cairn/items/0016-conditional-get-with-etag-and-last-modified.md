---
id: 16
title: Conditional GET with ETag and Last-Modified
type: feature
status: backlog
depends_on:
- 15
created: 2026-09-11
updated: 2026-09-11
priority: p0
effort: m
area: net
release: v0.2
---

## Problem

Every refresh downloads every feed in full. On a 50-feed list that is megabytes per refresh for content that usually has not changed, and it is discourteous to the servers involved.

## Proposal

Store each feed's ETag and Last-Modified, send If-None-Match / If-Modified-Since, and treat 304 as 'nothing changed'. Honour Retry-After on 429 and 503.

## Acceptance criteria

- [ ] An unchanged feed returns 304 and is not reparsed.
- [ ] A 429 with Retry-After defers the next attempt rather than hammering.
- [ ] Validators survive a restart.
