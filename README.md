# rsst

A terminal RSS/Atom feed reader, built with [ratatui](https://ratatui.rs).

Three panes: your feeds on the left, that feed's entries top-right, the selected
entry's text below. Feeds are fetched concurrently on launch and on demand.

## Install

```sh
cargo install --path .
```

## Usage

```sh
rsst                        # read your feeds
rsst --config path.toml     # use a different config
rsst --help
rsst --version

rsst import subs.opml       # merge an OPML list into your config
rsst export > subs.opml     # write your feeds out as OPML
```

Importing merges rather than replaces, so running it twice adds nothing the
second time. Folders in the OPML are flattened — there is nowhere to put them
yet.

## Configure

The first run writes a starter config and tells you where it lives:

- macOS: `~/Library/Application Support/rsst/config.toml`
- Linux: `~/.config/rsst/config.toml`
- Windows: `%APPDATA%\rsst\config.toml`

```toml
[[feeds]]
url = "https://blog.rust-lang.org/feed.xml"
title = "Rust Blog"        # optional; defaults to the feed's own title

[[feeds]]
url = "https://this-week-in-rust.org/atom.xml"
```

## Keys

| Key            | Action                    |
| -------------- | ------------------------- |
| `j` / `↓`      | Next item                 |
| `k` / `↑`      | Previous item             |
| `Tab`          | Switch between panes      |
| `o`            | Open the entry in your browser |
| `y`            | Copy its link to the clipboard |
| `r`            | Refresh all feeds         |
| `q` / `Esc`    | Quit                      |

Entries you have read are remembered between runs, and each feed shows how many
are still unread. An entry is recognised by its guid, its link, and its title
and date together — so a feed that regenerates its identifiers does not come
back looking entirely unread.

A feed that fails to load shows up as an `(error)` entry rather than taking the
reader down with it.

## Develop

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the branch and PR workflow, and
[ROADMAP.md](ROADMAP.md) for where this is going.

## License

MIT
