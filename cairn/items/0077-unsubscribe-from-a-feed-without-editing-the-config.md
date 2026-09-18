---
id: 77
title: Unsubscribe from a feed without editing the config
type: feature
status: done
assignee: Oddur Sigurdsson
created: 2026-09-13
updated: 2026-09-17
priority: p1
release: v1.7
effort: m
area: ui
---

## Problem

`+` adds a feed and `M` moves it to a folder. Nothing removes one. `keys.rs` has `AddFeed` and `MoveFeed` and no counterpart, so unsubscribing means quitting, opening the config in an editor, finding the right `[[feeds]]` table and deleting it by hand.

It is the most visible asymmetry in the interface: every other thing the reader can do to a feed, it can do from the keyboard.

## Proposal

An `unsubscribe` action that asks first — removing a subscription is not something to do on a stray keypress — and then edits the config the way `set_feed_url` and `set_feed_tags` already do, preserving comments and formatting.

What happens to the entries is the real question. Deleting them loses read and starred state for a feed the reader may resubscribe to; keeping them leaves rows nothing refers to. `retain_configured` already deletes rows for feeds no longer in the config, which answers it: the entries go, and starred ones go with them, so the confirmation has to say so.

## Acceptance criteria

- [x] An action removes the selected feed from the config, after confirming
- [x] The confirmation says how many entries and how many starred entries go with it
- [x] Comments and formatting in the config survive, like every other edit rsst makes
- [x] The feed disappears from the sidebar without a restart
- [x] Removing the last feed in a folder removes the folder from the tree
- [x] Cancelling changes nothing at all

`M` and `+` turned out to be missing from the key reference as well — bound but
undiscoverable and unconfigurable, the same gap `f` and `z` had. All three feed
actions are in the reference now, under a Feeds section. That pushed the
one-column overlay one row past a forty-row terminal, which is `0097`.
