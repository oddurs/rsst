# Contributing

## Setup

```sh
git clone git@github.com:oddurs/rsst.git
cd rsst
cargo test
ln -sf ../../scripts/pre-commit .git/hooks/pre-commit   # optional but recommended
```

## Branches

`main` is always releasable. Everything else lands through a pull request.

Name branches `<type>/<short-description>`, matching the commit types below:

```
feat/opml-import
fix/wrap-long-titles
chore/bump-ratatui
```

## Commits

[Conventional Commits](https://www.conventionalcommits.org). The type drives the
release notes, so pick it deliberately:

| Type       | Use for                                           |
| ---------- | ------------------------------------------------- |
| `feat`     | A user-visible capability                         |
| `fix`      | A bug fix                                         |
| `perf`     | A speed or memory improvement                     |
| `refactor` | A change with no behaviour difference             |
| `test`     | Tests only                                        |
| `docs`     | Documentation only                                |
| `chore`    | Tooling, dependencies, CI                         |

```
feat(ui): wrap entry summaries at the pane width

Long lines used to run off the right edge of the detail pane. Paragraph
now wraps with trim so the text reflows when the terminal resizes.
```

Breaking changes get a `!` (`feat!: …`) and a `BREAKING CHANGE:` footer.

## Pull requests

1. Branch from an up-to-date `main`.
2. Keep the diff focused — one concern per PR.
3. Cover behaviour changes with a test. `src/app.rs` and `src/feed.rs` are pure
   and easy to test; prefer putting logic there over inside `src/ui.rs`.
4. Green CI: fmt, clippy, tests on Linux/macOS/Windows, MSRV check, audit.
5. Squash-merge. The PR title becomes the commit message, so it follows the
   Conventional Commits format too.

## Stability

`docs/stability.md` says what semver covers, how the on-disk formats are
versioned, and when the MSRV may be raised. Read it before changing the config
schema, a default key binding, or either state file's format.

## Releasing

```sh
cargo set-version 0.2.0      # or edit Cargo.toml by hand
git commit -am "chore(release): v0.2.0"
git tag -a v0.2.0 -m "v0.2.0"
git push origin main --follow-tags
```

The tag triggers `.github/workflows/release.yml`, which builds binaries for
Linux, macOS, and Windows and attaches them to a GitHub release.
