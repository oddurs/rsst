---
id: 52
title: Fuzz the feed parser
type: chore
status: backlog
created: 2026-09-12
updated: 2026-09-12
priority: p2
effort: m
area: net
release: v1.2
---

## Problem

`SECURITY.md` says a malicious feed should not be able to crash the reader, hang it, or write outside its data directory. Nothing tests that. There is no fuzzing and no corpus of malformed feeds, so the claim is aspirational.
