# Contributing

## Setup

```sh
git clone git@github.com:oddurs/rsst.git
cd rsst
cargo test
ln -sf ../../scripts/pre-commit .git/hooks/pre-commit   # optional but recommended
```

## Trying a change

`scripts/dev` brings up a reader with known content and no network:

```sh
scripts/dev                   # seed if needed, then run
scripts/dev seed              # rebuild from fixtures and real feeds
scripts/dev seed --offline    # fixtures only: no network, same frame each time
scripts/dev shot 120x40       # render one frame as SVG
scripts/dev reset             # delete it
```

It runs with `RSST_HOME` pointed at `.dev/home`, so nothing it writes can reach
the rsst you actually read with — no config of yours is edited and no entry of
yours is marked read. The feeds come from `fixtures/feeds`, served by
`scripts/fixture_server.py`, which also generates the ones better computed than
committed: a 5,000-entry feed, a 404, a 500, a truncated document, and a page
that is not a feed at all despite saying it is.

The point of the fixtures is the cases real feeds cannot be made to produce on
demand. Adding one is adding an entry to a file in `fixtures/feeds`; adding a
new *kind* of failure means a branch in `generated()` in the server.

Alongside them sit ten real, public feeds, under a `Real` folder. Fixtures are
good at the awkward cases and useless at the ordinary one: real markup is
messier than anything worth inventing, real dates come in formats nobody would
choose, and real servers have opinions about caching. Any of them may be
unreachable — being offline is normal and is reported, not fatal.

`--offline` drops them. That is the deterministic mode: fixtures alone are a
pure function of the repository, so two runs give the same database and the
same frame, which is what makes `scripts/dev shot` worth comparing against the
last one. `shot` seeds offline for exactly that reason.

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

## Dependencies

Dependabot opens one grouped pull request a week for patch and minor bumps, and
a separate one for each major — a major bump is the kind that needs reading
rather than merging. CI also runs on a weekly schedule so an advisory published
against an unchanged tree surfaces without waiting for someone to push.

## Stability

`docs/stability.md` says what semver covers, how the on-disk formats are
versioned, and when the MSRV may be raised. Read it before changing the config
schema, a default key binding, or either state file's format.

## The changelog

`CHANGELOG.md` records what a release changed, for people rather than for git.
Add to **Unreleased** in the same pull request as the change, and only for
things a reader would notice — a refactor with no behaviour difference does not
belong there.

## Releasing

```sh
cargo set-version 0.2.0      # or edit Cargo.toml by hand
# move Unreleased into a dated section in CHANGELOG.md
git commit -am "chore(release): v0.2.0"
git tag -a v0.2.0 -m "v0.2.0"
git push origin main --follow-tags
```

The tag triggers `.github/workflows/release.yml`, which builds binaries for
Linux, macOS, and Windows and attaches them to a GitHub release.
