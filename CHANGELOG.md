# Changelog

Notable changes to rsst. The format follows [Keep a Changelog], and the project
follows [semantic versioning] — what that covers is written down in
[docs/stability.md](docs/stability.md).

[Keep a Changelog]: https://keepachangelog.com/en/1.1.0/
[semantic versioning]: https://semver.org/spec/v2.0.0.html

## Unreleased

Everything so far. Nothing has been released yet; `0031` tracks what publishing
still needs.

### Added

- A three-pane reader: feeds, that feed's entries, and the selected entry's text
- **Mouse-first interaction** — click a feed, entry or group heading; wheel over
  any pane to scroll it; click a link to open it; the status hints are buttons.
  `mouse = false` gives the terminal its selection back
- Read and starred state that survives a restart, recognising an entry by its
  guid, its link, and its title-and-date together, so a feed that regenerates
  identifiers does not come back looking unread
- OPML import and export, with folders mapping to tags in both directions
- A SQLite store for feeds, entries and read state, with FTS5 search
- Search across every feed, an unread-only filter, a starred view, an all-feeds
  view, and oldest-first sorting
- Feed groups that fold away, and per-feed fetch status and errors
- Conditional requests (`ETag` / `Last-Modified`), honouring `Retry-After`
- Configurable key bindings, themes that follow the terminal's own palette, and
  an ASCII fallback for terminals that cannot draw box characters
- `--help`, `--version`, `--config`, `--man`, `--completions` and `--screenshot`
- A generated man page and bash/zsh/fish completions

### Fixed

- A panic no longer leaves the terminal in raw mode with the alternate screen
  active
- Every capital-letter binding (`A`, `G`, `S`, `N`, `R`) was dead: terminals
  report Shift+key with the Shift modifier set, and the keymap compared
  modifiers literally
- Colours no longer pin absolute values that ignore the terminal's theme
- HTML entities in summaries are decoded rather than shown raw
