# rsst

A terminal RSS/Atom feed reader, built with [ratatui](https://ratatui.rs).

Three panes: your feeds on the left, that feed's entries top-right, the selected
entry's text below. Feeds are fetched concurrently on launch and on demand.

## Install

```sh
cargo install --path .
```

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
| `r`            | Refresh all feeds         |
| `q` / `Esc`    | Quit                      |

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
