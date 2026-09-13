---
id: 96
title: Sync with a self-hosted server
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p1
release: v2.0
effort: xl
area: net
part_of:
- 90
---

## Problem

rsst's state is a file on one machine. Read something on the laptop and the desktop does not know; read it on the desktop and the laptop shows it unread forever.

This is the feature that decides whether a reader is *the* reader or a second place things are read. Everything else on the roadmap makes rsst better at being one person's reader on one machine.

## Proposal

Sync against a self-hosted server rather than a commercial one, and against an API that several implement rather than one product: Miniflux and FreshRSS both speak the Google Reader API, and Miniflux has its own cleaner one.

It changes the shape of the app. Fetching stops being rsst's job and becomes the server's; entries arrive already deduplicated; read and starred state has to be reconciled rather than owned, which means conflicts, which means the local state needs to record what the server has been told. That is the real work, and it is why this is its own release rather than a feature.

`0090` matters first: reconciling history that is thrown away on every refresh is not reconciling.

## Acceptance criteria

- [ ] Entries, read state and stars sync both ways against at least one server
- [ ] Marking read offline reaches the server on the next sync, and the reverse
- [ ] A conflict resolves predictably, and the rule is written down
- [ ] Local-only operation is unchanged for anyone not using it
- [ ] Credentials are handled the way `0093` handles them
- [ ] A sync that fails halfway leaves the local database consistent
