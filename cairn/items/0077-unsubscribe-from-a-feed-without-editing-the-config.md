---
id: 77
title: Unsubscribe from a feed without editing the config
type: feature
status: backlog
created: 2026-09-13
updated: 2026-09-13
priority: p2
---

## Problem

`+` adds a feed and `M` moves it to a folder. Nothing removes one. `keys.rs` has `AddFeed` and `MoveFeed` and no counterpart, so unsubscribing means quitting, opening the config in an editor, finding the right `[[feeds]]` table and deleting it by hand.

It is the most visible asymmetry in the interface: every other thing the reader can do to a feed, it can do from the keyboard.

## Proposal

An `unsubscribe` action that asks first — removing a subscription is not something to do on a stray keypress — and then edits the config the way `set_feed_url` and `set_feed_tags` already do, preserving comments and formatting.

What happens to the entries is the real question. Deleting them loses read and starred state for a feed the reader may resubscribe to; keeping them leaves rows nothing refers to. `retain_configured` already deletes rows for feeds no longer in the config, which answers it: the entries go, and starred ones go with them, so the confirmation has to say so.

## Acceptance criteria

- [ ] An action removes the selected feed from the config, after confirming
- [ ] The confirmation says how many entries and how many starred entries go with it
- [ ] Comments and formatting in the config survive, like every other edit rsst makes
- [ ] The feed disappears from the sidebar without a restart
- [ ] Removing the last feed in a folder removes the folder from the tree
- [ ] Cancelling changes nothing at all
