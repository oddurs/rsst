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
- **Mouse-first interaction** — click a feed, entry or group heading; middle
  click to open one without selecting it; wheel over
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

- Updated `rustls` for RUSTSEC-2026-0285, a TLS 1.3 handshake flaw. Every feed
  rsst fetches goes through it

- rsst drew a whole frame ten times a second whether or not anything had
  changed. It draws when something happens now: sitting idle costs nothing
- A refresh deleted every row for a feed and inserted them all again, whatever
  had changed — 654 ms for a five-thousand-entry feed, run between two frames.
  It writes only what changed now: an unchanged refresh writes nothing and costs
  6 ms. A starred entry the publisher drops is also no longer at risk of being
  deleted if it was starred during the same session
- The detail pane laid the article out from scratch twice on every frame, and
  styled every row of it to show the thirty in view. Idling on a long article
  cost 23% of a core; it is now a tenth of that, and a frame costs what the
  screen is worth rather than what the publisher wrote

- A failed feed's `!` sat a column left of where every unread count sits, so the
  right edge of the sidebar was ragged
- A click on the "add a feed" prompt or the move picker fell through to the pane
  behind it, selecting a feed or opening an entry under a prompt still waiting
  for typing. Clicking away from a prompt now cancels it, as it does elsewhere
- The mouse wheel scrolled by dragging the selection through the list, so a
  trackpad flick never reached an end — selection wraps by design — marked every
  entry it passed as read, and swapped the article on every line. It now scrolls
  the view, stops at both ends, and leaves the cursor where it is

- Fetching is limited per host as well as globally (`max_concurrent_per_host`,
  2 by default), so fifteen feeds on one site no longer arrive as one burst —
  feeds on other hosts still use the full global capacity
- Redirects are followed by rsst rather than silently by the HTTP client, so a
  permanent move (301 or 308) is remembered and written back to the config with
  its comments intact, instead of costing an extra round trip on every refresh
  forever. A temporary one is followed and forgotten, a chain is bounded, and a
  chain with one temporary hop in it counts as temporary
- Adding the address of a *site* now finds its feed: rsst reads the page's
  `<link rel="alternate">`, prefers Atom over RSS, skips comment feeds, and
  remembers where it looked so the page is only ever visited once
- A transient failure is retried with growing, spread-out delays instead of
  marking the feed dead until the next `r` — and a bare 503, which used to park
  a feed for five minutes, is now just a server restarting. A 404 or a document
  that is not a feed is still never retried, because it cannot start working
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
- A feed publishing `<title></title>` sat in the sidebar as a blank line with an
  unread count beside it. Blank now falls back the way missing always did, and
  to the host rather than the whole address — `danluu.com`, not
  `https://danluu.com/atom.…`
- HTML entities in summaries are decoded rather than shown raw — and in titles,
  which were the one place left showing `R&amp;D` instead of `R&D`
