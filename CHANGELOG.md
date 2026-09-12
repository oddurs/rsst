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
- **A reading experience, not a dump of tags** — entry HTML is laid out as
  headings, lists, quotes, code, figures and aligned tables, set to a readable
  measure rather than stretched across the terminal; links are numbered against
  a reference list; `f` fetches the full text behind a teaser
- A reading mode (`z`) that gives the article the whole screen, pages with
  `Space`, and returns to each article where you left it
- Following a link from inside an article: type its number, or click the link
  itself or its line in the reference list
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
- `RSST_HOME`, which moves the config, database and read state into one
  directory together, so a second rsst cannot touch the first one's data
- `--help`, `--version`, `--config`, `--man`, `--completions` and `--screenshot`
- A generated man page and bash/zsh/fish completions
- `scripts/dev`, a seeded reader that needs no network and cannot reach real
  data: fixture feeds covering unicode, malformed markup, failing servers and
  5,000 entries, and the same frame on every run

### Fixed

- A failed fetch says what kind of failure it was — unreachable, timed out,
  server error, refused, gone, too big, not a feed — so `!` in the sidebar now
  comes with a sentence, and a 404 is no longer mistaken for a flaky network.
  `--screenshot` shows failures instead of drawing them as idle
- A feed could send as much as it liked: the body was buffered whole, so 600 MB
  on the wire meant 1.6 GB resident and nothing stopped it going further. Bodies
  are now streamed against a limit (`max_feed_megabytes`, 8 by default) and an
  oversized `Content-Length` is refused before a byte is read

- A panic no longer leaves the terminal in raw mode with the alternate screen
  active
- Every capital-letter binding (`A`, `G`, `S`, `N`, `R`) was dead: terminals
  report Shift+key with the Shift modifier set, and the keymap compared
  modifiers literally
- Colours no longer pin absolute values that ignore the terminal's theme
- HTML entities in summaries are decoded rather than shown raw — and in titles,
  which were the one place left showing `R&amp;D` instead of `R&D`
