---
id: 11
title: Open the selected entry in a browser
type: feature
status: planned
milestone: v0.1
created: 2026-09-11
updated: 2026-09-11
priority: p1
effort: s
area: ui
---

## Problem

The detail pane shows the entry's URL but nothing can be done with it. Reading the full article means copying the link out by hand.

## Proposal

Bind `o` to open the selected entry's link in the system browser (`open`/`xdg-open`/`start`), and `y` to copy it to the clipboard.

## Acceptance criteria

- [ ] `o` opens the entry in the default browser on macOS, Linux and Windows.
- [ ] An entry with no link shows a message in the status bar rather than failing silently.
- [ ] Launching the browser does not disturb the TUI or leave a zombie process.
