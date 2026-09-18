---
id: 79
title: Enclosures are dropped, so podcasts and attachments vanish
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-13
updated: 2026-09-17
priority: p1
release: v1.7
effort: m
area: feed
---

## Problem

`feed-rs` parses enclosures into `entry.media` — a `Vec<MediaObject>` with URLs, media types and sizes. `feed.rs` never looks at it. Every podcast episode, every video, every PDF attached to an entry is dropped on the floor without a word.

This is the same class of fault as the images and tables fixed in `0054`: the data arrives, the model has nowhere to put it, and the reader shows a page with a hole in it. A podcast feed in rsst today is a list of titles and show notes with the episode missing.

## Proposal

Carry enclosures on the entry, show them at the foot of the article the way the reference list is shown, and let them be opened — the system opener already handles a media URL, and `0071` already made middle click open things.

Worth showing what it is and how big: "Episode 412 — audio/mpeg, 48 MB" tells a reader on a train something they want to know.

## Acceptance criteria

- [x] Enclosures are parsed and stored with the entry
- [x] They are listed in the article with type and size where the feed gives them
- [x] One can be opened from the keyboard and by clicking it
- [x] An entry with no enclosures looks exactly as it does now
- [x] A feed that lists the same URL as both a link and an enclosure does not show it twice
- [x] Tested against a real podcast feed's markup, not an invented one

Stored in a table rather than a JSON column: JSON would have meant adding
`serde_json` as a direct dependency for one field, and the dependency policy in
`CLAUDE.md` asks for a better reason than convenience. A table also leaves them
queryable, which `0084` will want.

Numbering continues from the article's links rather than starting again, so
there is one list of things a number opens. On a podcast entry with an episode
and a transcript, `1` and `2` open the two files.
