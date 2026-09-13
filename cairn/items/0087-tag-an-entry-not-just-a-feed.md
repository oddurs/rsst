---
id: 87
title: Tag an entry, not just a feed
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
---

## Problem

Tags belong to feeds, and become folders in the sidebar. There is no way to mark an individual entry as anything other than read or starred.

Starring is one bucket. A reader who wants "to read on the train", "for the newsletter" and "reply to this" has one star and has to remember which is which.

## Proposal

Tags on entries, stored beside the read and starred keys — the same identity-by-key machinery, which already survives a feed regenerating its identifiers.

Tags combine with `0084` (`tag:train`) and with `0085`, so a tag is a sidebar entry without being a separate idea.

## Acceptance criteria

- [ ] An entry can be tagged and untagged from the keyboard
- [ ] Tags survive a refresh that rewrites the entry
- [ ] Tags survive an entry rolling out of the feed, the way a star does
- [ ] Tagged entries are findable through the query language
- [ ] The tags in use are discoverable rather than something to remember
