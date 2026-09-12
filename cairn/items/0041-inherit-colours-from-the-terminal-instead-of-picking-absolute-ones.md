---
id: 41
title: Inherit colours from the terminal instead of picking absolute ones
type: bug
status: done
assignee: Oddur Sigurdsson
created: 2026-09-12
updated: 2026-09-12
priority: p1
effort: m
area: ui
release: v0.4
---

## What happens

Colours do not follow the terminal's theme. In Ghostty — which reports `truecolor` and sets no `COLORFGBG` — three things go wrong:

- **The status bar hardcodes `Color::Black` text on `accent` (cyan).** Whether that is readable depends entirely on how light the theme's cyan happens to be. On a theme with a dark cyan it is black on dark.
- **`dim` is `Color::DarkGray`**, which is ANSI bright-black. Many themes set that very close to their background, so read entries and dates fade to invisible rather than receding.
- **The `light` preset is hardcoded RGB**, which by definition ignores the terminal's palette entirely.

## What should happen

rsst should say *what a thing is* and let the terminal say what colour that is. The terminal already chose sixteen colours that work against its own background; picking absolute values second-guesses a decision the user already made.

## Reproduction

1. Use any Ghostty theme whose cyan is dark, or whose bright-black is near the background.
2. Run `rsst`.
3. The status bar is unreadable, or dates and read entries disappear.
