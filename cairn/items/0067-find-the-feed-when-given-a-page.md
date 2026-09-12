---
id: 67
title: Find the feed when given a page
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

Adding `https://simonwillison.net/` adds a feed with no entries. The site has a feed; the page says where in a `<link rel="alternate">` tag. rsst does not look, because the bytes are not a feed and that is the end of it.

Pasting the address of the site you want to read is the obvious thing to do, and it fails.

## Proposal

When a response is HTML rather than a feed, read its `<link rel="alternate">` tags, take the first feed-shaped one, resolve it against the page URL and fetch that. Show which feed was found rather than silently substituting one.

## Acceptance criteria

- [x] A site URL with an alternate link resolves to that feed
- [x] A relative href resolves against the page it came from
- [x] A page with several alternates prefers Atom, then RSS
- [x] A page with none fails with a message saying so, not "not a feed"
- [x] Discovery happens once and the found URL is remembered

## 2026-09-12

The first attempt recursed: the hop called fetch, which discovered again, which hopped again. A test that served a page pointing at a page caught it — the fix is a private fetch_inner with a may_discover flag, false on the second request. Also found that the discovered feed was storing entries under the discovered URL, so retain_configured deleted them on the next prune; the configured address is the identity now, and the discovered one is only where to fetch.
