---
id: 63
title: Say what kind of failure a fetch hit
type: feature
status: backlog
created: 2026-09-12
updated: 2026-09-12
priority: p1
release: v1.5
effort: m
area: net
---

## Problem

Every failure becomes `Status::Failed(String)`. A DNS failure, a TLS error, a timeout, a 404, a 500 and a malformed document are one flat kind carrying one flat sentence.

Nothing downstream can tell them apart, so nothing can behave differently. A 404 is retried as eagerly as a timeout — which is to say, not at all, because there is no retry either; and the reader cannot say "this feed is gone" as against "the network is down", which are the two things a person actually wants to know.

## Proposal

An error type with the distinctions that change behaviour: unreachable, timed out, refused by the server, gone, too big, not a feed. Keep the message for display; add the kind for decisions.

This is the groundwork `0064` needs.

## Acceptance criteria

- [ ] A fetch failure carries a kind as well as a message
- [ ] Transient and permanent failures are distinguishable
- [ ] The status line still reads as a sentence, not an enum name
- [ ] Tests cover the mapping from each cause to its kind
