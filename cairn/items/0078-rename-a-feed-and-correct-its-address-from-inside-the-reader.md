---
id: 78
title: Rename a feed and correct its address from inside the reader
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
release: v1.7
effort: m
area: ui
---

## Problem

A feed's title and address can only be changed by editing the config. The reader already knows how to write both — `set_feed_url` was added for permanent redirects and `set_feed_tags` for folders — but neither is reachable from the keyboard.

Renaming matters more than it sounds: a feed's own title is often the site's marketing name, and `0073` falls back to a bare host when there is none. Correcting an address matters when a feed moves in a way no redirect announced.

## Proposal

An edit prompt for the selected feed, pre-filled with what is there now: title first, since it is the common case, and the address behind the same prompt.

Changing the address is the interesting one. The entries are keyed on the configured URL, so the rows have to move with it rather than being abandoned and refetched — the same problem `0065` solved for a redirect, and it should use the same path.

## Acceptance criteria

- [ ] The title can be changed from the reader, and an empty title falls back to the feed's own
- [ ] The address can be changed, and the entries, read state and stars move with it
- [ ] The config keeps its comments
- [ ] A malformed address is refused with a reason rather than written
