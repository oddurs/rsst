use rsst::{app, cache, cli, config, feed, generate, keys, launch, limit, opml, state, theme, ui};

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use rsst::app::App;
use rsst::cli::Action;
use rsst::config::Config;
use rsst::feed::Feed;
use rsst::state::ReadState;

// Short enough that a feed landing on the channel is drawn promptly, long
// enough that an idle reader is not busy-waiting.
const TICK: Duration = Duration::from_millis(100);

/// How many search hits to ask the index for.
///
/// More than anyone scrolls through, few enough that a one-letter query over a
/// large backlog does not build a huge list to throw away.
const SEARCH_LIMIT: usize = 500;

/// How often to look for feeds that have come due.
///
/// Not the refresh interval — the interval is per feed and measured in
/// minutes. This is only how often the question is asked, and it is cheap: a
/// few indexed lookups against the database.
const DUE_CHECK: Duration = Duration::from_secs(20);

/// A finished fetch on its way back to the event loop.
/// What a fetch task reports back: a note that it is trying again, or the end.
///
/// Retries are reported rather than waited out silently — a feed that looks
/// stalled for twenty seconds and one that is quietly on its third attempt
/// should not be the same picture.
enum Progress {
    Retrying {
        attempt: u32,
        wait: std::time::Duration,
    },
    Done(std::result::Result<feed::Outcome, feed::Failure>),
}

type Fetched = (usize, Progress);

/// A fetched article, keyed by the entry it belongs to.
type Article = (Vec<String>, Result<String>);

/// A URL that was checked before being added: its address and its own title.
type Added = Result<(String, String)>;

/// Everything the event loop needs besides the app state itself.
struct Session {
    keymap: keys::Keymap,
    config_path: PathBuf,
    client: reqwest::Client,
    limiter: std::sync::Arc<limit::Gate>,
    config: Config,
    db: rsst::db::Db,
    tx: tokio::sync::mpsc::UnboundedSender<Fetched>,
    rx: tokio::sync::mpsc::UnboundedReceiver<Fetched>,
    articles_tx: tokio::sync::mpsc::UnboundedSender<Article>,
    articles_rx: tokio::sync::mpsc::UnboundedReceiver<Article>,
    added_tx: tokio::sync::mpsc::UnboundedSender<Added>,
    added_rx: tokio::sync::mpsc::UnboundedReceiver<Added>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = match cli::parse(std::env::args().skip(1))? {
        Action::Help => {
            // Help is printed before the config is read, so it shows the
            // built-in bindings rather than failing on a broken config.
            println!("{}", cli::help(&keys::Keymap::default()));
            return Ok(());
        }
        Action::Version => {
            println!("rsst {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Action::Man => {
            print!("{}", generate::man(&keys::Keymap::default()));
            return Ok(());
        }
        Action::Completions(shell) => {
            print!("{}", generate::completions(&shell)?);
            return Ok(());
        }
        Action::Screenshot { size, config } => return screenshot(&size, config).await,
        Action::Import { path, config } => return import(&path, config),
        Action::Export { config } => return export(config),
        Action::Run { config } => config,
    };

    let config_override = config_path.clone();
    let config = Config::load_or_init(config_path)?;
    let keymap = keys::Keymap::from_config(&config.keys)?;
    // https://no-color.org — set to anything, it means no colour.
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let theme = theme::Theme::resolve(&config.theme, no_color)?;
    if config.feeds.is_empty() {
        let path = config::config_path()?;
        eprintln!("No feeds configured. Add some to {}.", path.display());
        return Ok(());
    }

    let client = http_client()?;
    let state_path = state::state_path()?;

    // The feed list takes its final shape before a single request is made, so
    // the reader is on screen and usable while the network is still working.
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Fetched>();
    let mut db = rsst::db::Db::open(&rsst::db::db_path()?)?;
    // One-time: bring the old TOML files across rather than starting empty.
    rsst::db::migrate_from_toml(&mut db, &cache::cache_path()?, &state_path)?;
    let limiter = limit::Gate::new(config.fetch_limit(), config.per_host());
    spawn_fetches(&client, &config, &db, &limiter, &tx);

    // Last known contents stand in until the fetch lands, so a second launch
    // has something to read immediately and an offline one still works.
    let feeds = config
        .feeds
        .iter()
        .map(|source| {
            db.feed(source)
                .ok()
                .flatten()
                .unwrap_or_else(|| Feed::pending(source))
        })
        .collect();
    let mut app = App::new(feeds, ReadState::from_db(&db)?)
        .with_tags(&config.feeds)
        .with_theme(theme)
        .with_measure(config.measure(theme.ascii));

    let (articles_tx, articles_rx) = tokio::sync::mpsc::unbounded_channel::<Article>();
    let (added_tx, added_rx) = tokio::sync::mpsc::unbounded_channel::<Added>();
    let mut session = Session {
        keymap,
        config_path: config::config_path_or(config_override)?,
        db,
        client,
        limiter,
        config,
        tx,
        rx,
        articles_tx,
        articles_rx,
        added_tx,
        added_rx,
    };

    install_panic_hook();
    let mut terminal = enter(session.config.mouse)?;
    let result = run(&mut terminal, &mut app, &mut session).await;
    restore()?;

    // Save even when the loop failed: the user still read those entries, and
    // losing that is more annoying than whatever went wrong.
    if let Err(err) = app.read.persist(&mut session.db) {
        eprintln!("rsst: could not save read state: {err:#}");
    }
    if let Err(err) = session.db.retain_configured(&session.config.feeds) {
        eprintln!("rsst: could not tidy the database: {err:#}");
    }
    result
}

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

async fn run(terminal: &mut Tui, app: &mut App, session: &mut Session) -> Result<()> {
    let mut clicks = Clicks::default();
    // The entry the article pane is currently showing, so the lookup happens
    // when the selection changes rather than on every frame.
    let mut showing: Option<Vec<String>> = None;
    // Checked immediately on the first pass, so a feed that was due while the
    // reader was closed is fetched on launch rather than twenty seconds in.
    let mut last_due_check = std::time::Instant::now() - DUE_CHECK;
    // Nothing draws unless something has changed. The reader used to render a
    // whole frame ten times a second whether or not anything had moved, which
    // on a laptop is a background drain with nothing to show for it.
    //
    // Set wherever something arrives or the reader does something, and
    // deliberately generous: an event that turns out to change nothing costs
    // one frame, while an event that changes something and is missed would
    // leave the screen lying.
    let mut dirty = true;
    while !app.should_quit {
        // Take whatever has arrived since the last frame. Never blocks, so a
        // slow feed cannot hold up the redraw.
        while let Ok((index, result)) = session.rx.try_recv() {
            dirty = true;
            let Some(slot) = app.feeds.get_mut(index) else {
                continue;
            };
            let url = slot.url.clone();

            // A retry is not an answer: say so and wait for the real one.
            let result = match result {
                Progress::Retrying { attempt, wait } => {
                    slot.status = feed::Status::Fetching;
                    app.status = Some(format!(
                        " {}: trying again in {:.1}s (attempt {}) ",
                        slot.title,
                        wait.as_secs_f32(),
                        attempt + 1
                    ));
                    continue;
                }
                Progress::Done(result) => result,
            };

            // Reached, whatever the answer — a 304 means the server was asked
            // and replied, which is what the timer needs to know.
            if result.is_ok() {
                let _ = session.db.mark_fetched(&url, chrono::Utc::now());
            }
            match result {
                Ok(feed::Outcome::Updated {
                    feed,
                    etag,
                    last_modified,
                    found_at,
                }) => {
                    // Only this feed's rows — the whole reason for the database.
                    let _ = session.db.put_feed(&feed);
                    let _ = session.db.set_validators(&url, etag, last_modified);
                    // Somebody gave the address of a site; the site named its
                    // feed. Say which one, so the config can be corrected.
                    if let Some(found) = found_at {
                        // Remembered either way, so the extra hop is paid once
                        // rather than on every refresh from here on.
                        let _ = session.db.set_resolved(&url, Some(&found));
                        // And written back, because a config that still names
                        // the old address is a file that says something untrue.
                        let fixed =
                            config::set_feed_url(&session.config_path, &url, &found).is_ok();
                        app.status = Some(match fixed {
                            true => {
                                format!(" {} has moved to {found} — config updated ", slot.title)
                            }
                            false => format!(" {} is really at {found} ", slot.title),
                        });
                    }
                    *slot = *feed;
                }
                // Nothing was downloaded or reparsed; what is on screen stands.
                Ok(feed::Outcome::NotModified) => slot.status = feed::Status::Idle,
                Ok(feed::Outcome::RateLimited { retry_after }) => {
                    let until = chrono::Utc::now()
                        + chrono::Duration::from_std(retry_after)
                            .unwrap_or_else(|_| chrono::Duration::seconds(300));
                    let _ = session.db.defer_until(&url, until);
                    slot.status = feed::Status::Idle;
                    app.status = Some(format!(
                        " {}: rate limited, waiting {}s ",
                        slot.title,
                        retry_after.as_secs()
                    ));
                }
                // Keep whatever is already on screen — when that came from the
                // cache it is what makes the reader usable offline. The failure
                // is recorded on the feed rather than invented as an entry.
                Err(failure) => {
                    slot.status = feed::Status::Failed {
                        trouble: failure.trouble,
                        // The sentence leads: "is no longer there" is what a
                        // reader needs; the detail is there for the curious.
                        message: format!(
                            "{} {} — {}",
                            slot.title,
                            failure.trouble.sentence(),
                            failure.detail
                        ),
                    }
                }
            }
        }

        // Feeds that have come due, fetched without being asked. Detached, so
        // the interface never waits for the network.
        if last_due_check.elapsed() >= DUE_CHECK {
            last_due_check = std::time::Instant::now();
            // Marks a feed as fetching, which is a change worth showing.
            dirty = true;
            let now = chrono::Utc::now();
            let due: Vec<usize> = session
                .config
                .feeds
                .iter()
                .enumerate()
                .filter(|(index, source)| {
                    let idle = app
                        .feeds
                        .get(*index)
                        .is_some_and(|feed| feed.status != feed::Status::Fetching);
                    idle && session.db.due(
                        &source.url,
                        session.config.refresh_interval(source),
                        now,
                    )
                })
                .map(|(index, _)| index)
                .collect();

            if !due.is_empty() {
                for index in &due {
                    if let Some(feed) = app.feeds.get_mut(*index) {
                        feed.status = feed::Status::Fetching;
                    }
                }
                spawn_some(
                    &session.client,
                    &session.config,
                    &session.db,
                    &session.limiter,
                    &session.tx,
                    due,
                );
            }
        }

        // A checked feed coming back, ready to be written to the config.
        while let Ok(result) = session.added_rx.try_recv() {
            dirty = true;
            app.status = Some(
                match result.and_then(|(url, title)| {
                    config::add_feed(&session.config_path, &url, Some(&title))?;
                    Ok((url, title))
                }) {
                    Ok((url, title)) => match reload(app, session) {
                        Ok(_) => {
                            app.selected_feed = app
                                .feeds
                                .iter()
                                .position(|feed| feed.url == url)
                                .unwrap_or(app.selected_feed);
                            format!(" Added {title}. ")
                        }
                        Err(err) => format!(" Added, but could not reload: {err} "),
                    },
                    Err(err) => format!(" Not added: {err} "),
                },
            );
        }

        // A fetched article arriving is the only other thing worth a redraw.
        while let Ok((keys, result)) = session.articles_rx.try_recv() {
            dirty = true;
            let current = app.current_entry().map(|entry| entry.keys.clone());
            match result {
                Ok(html) => {
                    if let Some(entry) = app.current_entry().cloned() {
                        let _ = session.db.put_article(&entry, &html);
                    }
                    if current.as_deref() == Some(keys.as_slice()) {
                        app.article = Some(html);
                        app.detail_scroll = 0;
                        app.status = Some(" Full article. ".into());
                    }
                }
                Err(err) => app.status = Some(format!(" Could not fetch: {err} ")),
            }
        }

        // Whatever is selected, show the article that was fetched for it.
        let keys = app.current_entry().map(|entry| entry.keys.clone());
        if keys != showing {
            app.article = app
                .current_entry()
                .and_then(|entry| session.db.article(entry));
            showing = keys;
        }

        app.keep_place();
        if dirty {
            terminal.draw(|frame| ui::draw(frame, app, &session.keymap))?;
            dirty = false;
        }

        if !event::poll(TICK)? {
            continue;
        }
        let event = event::read()?;
        // Anything the terminal sends may have changed something — including a
        // resize, which changes everything. Cheaper to draw one frame that was
        // not needed than to work out which events matter and be wrong.
        dirty = true;
        if let Event::Mouse(mouse) = event {
            let double = clicks.is_double(&mouse);
            let hit = rsst::mouse::resolve(&app.hits, mouse, double);
            if let Some(action) = rsst::mouse::apply(app, hit) {
                dispatch(action, app, session)?;
            }
            continue;
        }
        let Event::Key(key) = event else {
            continue;
        };
        if !handles(key.kind) {
            continue;
        }

        app.status = None;

        // The overlay can be longer than the screen, so movement scrolls it
        // and anything else dismisses it.
        if app.help_open {
            match session.keymap.action(key.code, key.modifiers) {
                Some(keys::Action::Next) => app.help_scroll = app.help_scroll.saturating_add(1),
                Some(keys::Action::Previous) => app.help_scroll = app.help_scroll.saturating_sub(1),
                _ => {
                    app.help_open = false;
                    app.help_scroll = 0;
                }
            }
            continue;
        }

        // The add prompt owns the keyboard while it is open.
        if app.adding.is_some() {
            match key.code {
                KeyCode::Esc => app.cancel_add(),
                KeyCode::Backspace => app.backspace_add(),
                KeyCode::Enter => match app.add_url() {
                    Some(url) => {
                        app.cancel_add();
                        app.status = Some(" Checking that feed… ".into());
                        let client = session.client.clone();
                        let tx = session.added_tx.clone();
                        let limits = session.config.limits();
                        tokio::spawn(async move {
                            let _ = tx.send(check_feed(&client, &url, limits).await);
                        });
                    }
                    None => app.status = Some(" That needs to be an http:// URL. ".into()),
                },
                KeyCode::Char(ch) => app.type_add(ch),
                _ => {}
            }
            continue;
        }

        // The move picker owns the keyboard while it is open.
        if app.moving.is_some() {
            match key.code {
                KeyCode::Esc => app.cancel_move(),
                KeyCode::Up => app.step_move(-1),
                KeyCode::Down => app.step_move(1),
                KeyCode::Backspace => app.backspace_move(),
                KeyCode::Enter => match app.move_destination() {
                    Some((feed, path)) => {
                        app.cancel_move();
                        match move_feed(app, session, feed, &path) {
                            Ok(where_to) => app.status = Some(format!(" Moved to {where_to}. ")),
                            Err(err) => app.status = Some(format!(" Could not move: {err} ")),
                        }
                    }
                    // A new folder with no name is not a destination.
                    None => app.status = Some(" Name the folder first. ".into()),
                },
                KeyCode::Char(ch) => app.type_move(ch),
                _ => {}
            }
            continue;
        }

        // A queued bulk mark owns the keyboard until it is answered.
        if app.pending.is_some() {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y')
                    if app.pending == Some(app::Bulk::Unsubscribe) =>
                {
                    app.cancel_bulk();
                    app.status = Some(match unsubscribe(app, session) {
                        Ok(title) => format!(" Unsubscribed from {title}. "),
                        Err(err) => format!(" Could not unsubscribe: {err:#} "),
                    });
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let marked = app.confirm_bulk();
                    let _ = app.read.persist(&mut session.db);
                    app.status = Some(format!(" Marked {marked} entries read. "));
                }
                _ => {
                    app.cancel_bulk();
                    app.status = Some(" Cancelled. ".into());
                }
            }
            continue;
        }

        // While the query is being typed the keyboard belongs to the text
        // field, or every letter would also be a command.
        if app.search.as_ref().is_some_and(|s| s.typing) {
            match key.code {
                KeyCode::Esc => app.cancel_search(),
                KeyCode::Enter => app.confirm_search(),
                KeyCode::Backspace => app.pop_search(),
                KeyCode::Char(ch) => app.push_search(ch),
                _ => {}
            }
            // The full-text index answers instead of a scan over memory. A
            // query it cannot parse leaves the in-memory result standing.
            if let Some(query) = app.search_query()
                && let Ok(hits) = session.db.search(query, SEARCH_LIMIT)
            {
                app.apply_search_hits(&hits);
            }
            continue;
        }

        // Esc leaves a search before anything else can claim it.
        if key.code == KeyCode::Esc {
            if app.following.is_some() {
                app.cancel_following();
            } else if app.search.is_some() {
                app.cancel_search();
            } else if app.reading {
                app.toggle_reading();
            } else {
                app.should_quit = true;
            }
            continue;
        }
        // A digit names one of the article's numbered links. Checked before
        // the keymap so a binding cannot quietly swallow it mid-number.
        match key.code {
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                if let Some(link) = app.type_link_digit(digit) {
                    open_link(app, &link);
                }
                continue;
            }
            KeyCode::Enter if app.following.is_some() => {
                match app.take_typed_link() {
                    Some(link) => open_link(app, &link),
                    None => app.status = Some(" No such link. ".into()),
                }
                continue;
            }
            KeyCode::Backspace if app.following.is_some() => {
                app.cancel_following();
                continue;
            }
            _ => {}
        }

        // While a search has results, n and N walk them instead of the backlog.
        if app.search.is_some() {
            match key.code {
                KeyCode::Char('n') => {
                    app.step_match(1);
                    continue;
                }
                KeyCode::Char('N') => {
                    app.step_match(-1);
                    continue;
                }
                _ => {}
            }
        }

        let Some(action) = session.keymap.action(key.code, key.modifiers) else {
            continue;
        };
        dispatch(action, app, session)?;
    }
    Ok(())
}

/// Starts one fetch per configured feed, reporting each back as it finishes.
///
/// Detached on purpose: the event loop owns the receiver and drains it, so no
/// caller ever waits on the network.
fn spawn_fetches(
    client: &reqwest::Client,
    config: &Config,
    db: &rsst::db::Db,
    limiter: &std::sync::Arc<limit::Gate>,
    tx: &tokio::sync::mpsc::UnboundedSender<Fetched>,
) {
    spawn_some(client, config, db, limiter, tx, 0..config.feeds.len());
}

/// Starts a fetch for each of `indices`.
fn spawn_some(
    client: &reqwest::Client,
    config: &Config,
    db: &rsst::db::Db,
    limiter: &std::sync::Arc<limit::Gate>,
    tx: &tokio::sync::mpsc::UnboundedSender<Fetched>,
    indices: impl IntoIterator<Item = usize>,
) {
    let now = chrono::Utc::now();
    let limits = config.limits();
    let retry = config.retry();
    for index in indices {
        let Some(source) = config.feeds.get(index).cloned() else {
            continue;
        };
        // Honour a server that asked us to wait rather than hammering it.
        if !db.may_fetch(&source.url, now) {
            let _ = tx.send((index, Progress::Done(Ok(feed::Outcome::NotModified))));
            continue;
        }
        let meta = db.meta(&source.url);
        // A feed found behind a web page is fetched straight from where it
        // was found; the page is only visited once, ever.
        let source = match db.resolved(&source.url) {
            Some(found) => config::FeedSource {
                url: found,
                ..source
            },
            None => source,
        };
        let client = client.clone();
        let tx = tx.clone();
        let limiter = limiter.clone();
        tokio::spawn(async move {
            let started = std::time::Instant::now();
            let mut attempt = 0u32;
            loop {
                // Every task is spawned at once, but only a few hold a permit
                // and are actually talking to the network at any moment.
                let result = limiter
                    .run(
                        &source.url,
                        feed::fetch(
                            &client,
                            &source,
                            meta.etag.as_deref(),
                            meta.last_modified.as_deref(),
                            limits,
                        ),
                    )
                    .await;

                // Only what could plausibly succeed next time. A 404 is a
                // decision someone made, not a network that will be back.
                let worth_retrying = result
                    .as_ref()
                    .err()
                    .is_some_and(|failure| failure.trouble.transient());
                if worth_retrying
                    && let Some(wait) = retry.delay(attempt + 1, &source.url, started.elapsed())
                {
                    attempt += 1;
                    let _ = tx.send((index, Progress::Retrying { attempt, wait }));
                    tokio::time::sleep(wait).await;
                    continue;
                }

                // A closed channel means the reader has already quit.
                let _ = tx.send((index, Progress::Done(result)));
                break;
            }
        });
    }
}

/// Renders one frame of the real interface as SVG, then exits.
///
/// Fetches first so the frame has real content; a screenshot of an empty
/// reader would be honest about nothing.
async fn screenshot(size: &str, config_override: Option<PathBuf>) -> Result<()> {
    let (width, height) = rsst::screenshot::parse_size(size)
        .context("--screenshot wants WIDTHxHEIGHT, like 100x30")?;

    let config = Config::load_or_init(config_override)?;
    let keymap = keys::Keymap::from_config(&config.keys)?;
    let theme = theme::Theme::resolve(&config.theme, false)?;
    let client = http_client()?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Fetched>();
    let db = rsst::db::Db::open(&rsst::db::db_path()?)?;
    let limiter = limit::Gate::new(config.fetch_limit(), config.per_host());
    spawn_fetches(&client, &config, &db, &limiter, &tx);
    drop(tx);

    let feeds = config
        .feeds
        .iter()
        .map(|source| {
            db.feed(source)
                .ok()
                .flatten()
                .unwrap_or_else(|| feed::Feed::pending(source))
        })
        .collect();
    let mut app = App::new(feeds, ReadState::from_db(&db)?)
        .with_tags(&config.feeds)
        .with_theme(theme)
        .with_measure(config.measure(theme.ascii));

    // Wait for what arrives promptly; a slow feed should not hold up a
    // screenshot, it should just show as still loading.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while let Ok(Some((index, progress))) = tokio::time::timeout_at(deadline, rx.recv()).await
        && let Some(slot) = app.feeds.get_mut(index)
    {
        // A retry still counts as fetching, which is what the placeholder says.
        let Progress::Done(result) = progress else {
            continue;
        };
        match result {
            Ok(feed::Outcome::Updated { feed, .. }) => *slot = *feed,
            Ok(_) => slot.status = feed::Status::Idle,
            // A screenshot that quietly showed a dead feed as idle would be a
            // picture of an interface nobody has.
            Err(failure) => {
                slot.status = feed::Status::Failed {
                    trouble: failure.trouble,
                    message: format!("{} {}", slot.title, failure.trouble.sentence()),
                }
            }
        }
    }

    app.focus = app::Pane::Entries;
    app.mark_current_read();
    print!(
        "{}",
        rsst::screenshot::svg(&mut app, &keymap, width, height)
    );
    Ok(())
}

/// Carries out one action, whatever asked for it.
///
/// The single place an action becomes a change, so the keyboard and the pointer
/// cannot disagree about what `refresh` means.
fn dispatch(action: keys::Action, app: &mut App, session: &mut Session) -> Result<()> {
    use keys::Action;
    match action {
        Action::Quit => app.should_quit = true,
        Action::Help => app.help_open = true,
        Action::CyclePane => app.toggle_focus(),
        Action::Next => app.select_next(),
        Action::Previous => app.select_previous(),
        Action::First => app.select_first(),
        Action::Last => app.select_last(),
        Action::HalfPageDown => app.half_page(1),
        Action::HalfPageUp => app.half_page(-1),
        Action::ToggleReading => app.toggle_reading(),
        // With the article alone on screen, space is a page turn — the one
        // key every other reader in the world already binds to that.
        Action::ToggleGroup if app.reading => app.page(1),
        Action::ToggleGroup if app.focus == app::Pane::Feeds => app.toggle_group(),
        Action::ToggleGroup => {}
        Action::MoveFeed => app.start_move(),
        Action::Unsubscribe => match app.current_feed() {
            Some(_) => app.request_bulk(app::Bulk::Unsubscribe),
            None => app.status = Some(" No feed to unsubscribe from. ".into()),
        },
        Action::AddFeed => app.start_add(),
        Action::NextUnread => {
            if !app.next_unread(true) {
                app.status = Some(" No unread entries. ".into());
            }
        }
        Action::PreviousUnread => {
            if !app.next_unread(false) {
                app.status = Some(" No unread entries. ".into());
            }
        }
        Action::Search => app.start_search(),
        Action::ToggleRead => {
            app.toggle_current_read();
            let _ = app.read.persist(&mut session.db);
        }
        Action::MarkFeedRead => app.request_bulk(app::Bulk::Feed),
        Action::MarkAllRead => app.request_bulk(app::Bulk::Everything),
        Action::ToggleUnreadOnly => {
            app.toggle_unread_only();
            let _ = app.read.persist(&mut session.db);
        }
        Action::ToggleStar => {
            let starred = app.toggle_star();
            let _ = app.read.persist(&mut session.db);
            app.status = Some(if starred {
                " Starred. ".into()
            } else {
                " Unstarred. ".into()
            });
        }
        Action::ToggleStarredView => app.toggle_starred_view(),
        Action::ToggleAllFeeds => app.toggle_all_feeds_view(),
        Action::ReloadConfig => match reload(app, session) {
            // A broken config must leave the running reader exactly as it
            // was: the feeds on screen are still perfectly readable.
            Err(err) => app.status = Some(format!(" Config not reloaded: {err} ")),
            Ok(added) => {
                app.status = Some(if added == 0 {
                    " Config reloaded. ".into()
                } else {
                    format!(" Config reloaded; fetching {added} new feed(s)… ")
                });
            }
        },
        Action::ToggleSort => {
            app.toggle_sort();
            let _ = app.read.persist(&mut session.db);
            app.status = Some(format!(" Sorted by {}. ", app.sort_description()));
        }
        Action::CycleSort => {
            app.cycle_sort();
            let _ = app.read.persist(&mut session.db);
            app.status = Some(format!(" Sorted by {}. ", app.sort_description()));
        }
        Action::ToggleFeedSort => {
            let own = app.toggle_feed_sort();
            // A feed with its own order needs its pref removed when it gives
            // it back, or the old choice would return on the next launch.
            if own.is_none()
                && let Some(feed) = app.current_feed()
            {
                session.db.clear_pref(&format!("sort:{}", feed.url));
            }
            let _ = app.read.persist(&mut session.db);
            app.status = Some(match own {
                Some(_) => format!(
                    " This feed sorts by {} on its own. ",
                    app.sort_description()
                ),
                None => " This feed follows the usual order again. ".into(),
            });
        }
        Action::FetchArticle => {
            app.status = Some(match fetch_article(app, session) {
                Some(message) => message,
                None => " Fetching the full article… ".into(),
            });
        }
        Action::Open => open_selected(app),
        Action::CopyLink => copy_selected(app),
        Action::Refresh => {
            let _ = app.read.persist(&mut session.db);
            let starting = app.begin_refresh();
            app.status = Some(if starting.is_empty() {
                " Already refreshing… ".into()
            } else {
                format!(" Refreshing {} feed(s)… ", starting.len())
            });
            spawn_some(
                &session.client,
                &session.config,
                &session.db,
                &session.limiter,
                &session.tx,
                starting,
            );
        }
    }
    Ok(())
}

/// Remembers the last click, so a second one nearby counts as a double.
///
/// Terminals report clicks, not double-clicks, so the timing is ours to keep.
#[derive(Debug, Default)]
struct Clicks {
    last: Option<(u16, u16, std::time::Instant)>,
}

/// How close together two clicks must be to count as one gesture.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

impl Clicks {
    fn is_double(&mut self, event: &crossterm::event::MouseEvent) -> bool {
        if !matches!(
            event.kind,
            MouseEventKind::Down(crossterm::event::MouseButton::Left)
        ) {
            return false;
        }
        let now = std::time::Instant::now();
        let double = self.last.is_some_and(|(column, row, at)| {
            column == event.column && row == event.row && now.duration_since(at) < DOUBLE_CLICK
        });
        // A double-click does not seed a triple: the third click starts over.
        self.last = if double {
            None
        } else {
            Some((event.column, event.row, now))
        };
        double
    }
}

/// Moves a feed into a folder, writing the change to the config.
///
/// The config is the source of truth for where a feed lives, so the move is
/// written there rather than held in the database — otherwise the next edit by
/// hand would silently undo it.
fn move_feed(app: &mut App, session: &mut Session, feed: usize, path: &[String]) -> Result<String> {
    let url = app
        .feeds
        .get(feed)
        .map(|feed| feed.url.clone())
        .context("that feed is gone")?;

    config::set_feed_tags(&session.config_path, &url, path)?;

    // Re-read rather than patching the in-memory copy, so what is on screen is
    // what is in the file.
    let config = Config::load_from(&session.config_path)?;
    app.reconcile(&config.feeds);
    app.selected_feed = app
        .feeds
        .iter()
        .position(|feed| feed.url == url)
        .unwrap_or(app.selected_feed);
    session.config = config;

    Ok(if path.is_empty() {
        "the top level".into()
    } else {
        path.join(" / ")
    })
}

/// Starts fetching the selected entry's page, unless there is nothing to fetch.
///
/// Never automatic: fetching every article of every feed is neither polite to
/// the publishers nor something anyone asked for.
fn fetch_article(app: &mut App, session: &Session) -> Option<String> {
    let entry = app.current_entry()?.clone();
    let link = entry.link.clone().filter(|link| !link.is_empty());
    let Some(link) = link else {
        return Some(" This entry has no link to fetch. ".into());
    };
    if app.article.is_some() {
        return Some(" Already showing the full article. ".into());
    }

    let client = session.client.clone();
    let tx = session.articles_tx.clone();
    tokio::spawn(async move {
        let result = download(&client, &link).await;
        let _ = tx.send((entry.keys.clone(), result));
    });
    None
}

/// Downloads a page and pulls the article out of it.
async fn download(client: &reqwest::Client, url: &str) -> Result<String> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("requesting {url}"))?
        .error_for_status()
        .with_context(|| format!("bad status from {url}"))?;

    let html = response
        .text()
        .await
        .with_context(|| format!("reading {url}"))?;

    rsst::readable::extract(&html).context("nothing on that page reads like an article")
}

/// Checks that a URL really is a feed, and reports what it calls itself.
///
/// Fetched before being written to the config rather than after: a typo added
/// and then found to be broken leaves the reader with a dead feed and the
/// person with a file to edit by hand.
async fn check_feed(client: &reqwest::Client, url: &str, limits: feed::Limits) -> Added {
    let source = config::FeedSource {
        url: url.to_string(),
        refresh_minutes: None,
        title: None,
        tags: Vec::new(),
    };

    match feed::fetch(client, &source, None, None, limits).await? {
        feed::Outcome::Updated { feed, .. } => Ok((url.to_string(), feed.title.clone())),
        feed::Outcome::NotModified => Ok((url.to_string(), url.to_string())),
        feed::Outcome::RateLimited { .. } => {
            anyhow::bail!("that server asked us to come back later")
        }
    }
}

/// Whether a key event should be acted on.
///
/// Windows reports a Release for every key as well as a Press, and on some
/// terminals a Repeat too. Acting on all of them makes every keystroke happen
/// two or three times — which looks like a rendering bug rather than an input
/// one, and is why this is a named function with a test rather than an inline
/// comparison.
fn handles(kind: KeyEventKind) -> bool {
    matches!(kind, KeyEventKind::Press)
}

/// Re-reads the config and reconciles the running reader with it.
///
/// Everything is validated before anything is changed, so a config that fails
/// to parse — or names an unknown action or colour — leaves the session alone.
/// Removes the selected feed from the config and from the database.
///
/// Writes the config and then reloads, which is the path `R` already takes —
/// `reconcile` drops the feed from the list, fixes the cursor and rebuilds the
/// folder tree, so unsubscribing needs no second implementation of any of it.
fn unsubscribe(app: &mut App, session: &mut Session) -> Result<String> {
    let feed = app.current_feed().context("no feed is selected")?;
    let (url, title) = (feed.url.clone(), feed.title.clone());

    config::remove_feed(&session.config_path, &url)?;
    reload(app, session)?;

    // The rows go with the subscription. `retain_configured` is what decides
    // that, and it is already the rule for a feed removed from the config by
    // hand — the confirmation says so before any of it happens.
    let _ = session.db.retain_configured(&session.config.feeds);
    let _ = session.db.prune();
    Ok(title)
}

fn reload(app: &mut App, session: &mut Session) -> Result<usize> {
    let config = Config::load_from(&session.config_path)?;
    let keymap = keys::Keymap::from_config(&config.keys)?;
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let theme = theme::Theme::resolve(&config.theme, no_color)?;

    app.reconcile(&config.feeds);
    app.theme = theme;
    session.keymap = keymap;
    session.config = config;

    // Only the feeds that arrived with this reload need fetching.
    let starting = app.indices_without_entries();
    for index in &starting {
        if let Some(feed) = app.feeds.get_mut(*index) {
            feed.status = feed::Status::Fetching;
        }
    }
    let count = starting.len();
    spawn_some(
        &session.client,
        &session.config,
        &session.db,
        &session.limiter,
        &session.tx,
        starting,
    );
    Ok(count)
}

/// Opens a URL in the browser, reporting the outcome.
fn open_link(app: &mut App, link: &str) {
    app.status = Some(match launch::browser(link) {
        Ok(()) => format!(" Opened {link} "),
        Err(err) => format!(" Could not open: {err:#} "),
    });
}

/// Opens the selected entry's link in the browser, reporting the outcome.
fn open_selected(app: &mut App) {
    // A link the reader has picked out of the article wins over the entry's
    // own, which is what "open" means when they have not picked one.
    if let Some(link) = app.take_typed_link() {
        open_link(app, &link);
        return;
    }
    let Some(link) = app.current_entry().and_then(|entry| entry.link.clone()) else {
        app.status = Some(" This entry has no link. ".into());
        return;
    };
    open_link(app, &link);
}

/// Copies the selected entry's link to the clipboard.
fn copy_selected(app: &mut App) {
    let Some(link) = app.current_entry().and_then(|entry| entry.link.clone()) else {
        app.status = Some(" This entry has no link. ".into());
        return;
    };
    app.status = Some(match launch::clipboard(&link) {
        Ok(()) => format!(" Copied {link} "),
        Err(err) => format!(" Could not copy: {err:#} "),
    });
}

/// Merges an OPML file into the config, reporting what actually changed.
fn import(path: &std::path::Path, config_override: Option<PathBuf>) -> Result<()> {
    let xml =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let incoming = opml::parse(&xml)?;
    let found = incoming.len();

    let config_path = match config_override {
        Some(path) => path,
        None => config::config_path()?,
    };
    // Start from whatever is already configured, or from nothing — never from
    // the starter config, which would import someone else's feeds alongside.
    let mut config = if config_path.exists() {
        Config::load_from(&config_path)?
    } else {
        Config::default()
    };

    let added = opml::merge(&mut config.feeds, incoming);
    config.save(&config_path)?;

    println!(
        "Imported {added} new feed{} from {found} in the file. {} total in {}.",
        if added == 1 { "" } else { "s" },
        config.feeds.len(),
        config_path.display()
    );
    Ok(())
}

/// Writes the configured feeds to stdout as OPML.
fn export(config_override: Option<PathBuf>) -> Result<()> {
    let config = Config::load_or_init(config_override)?;
    print!("{}", opml::write(&config.feeds));
    Ok(())
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        // Followed by `feed::fetch` instead, which can tell a permanent move
        // from a temporary one and remember the former.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("rsst/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()
        .context("building the HTTP client")
}

fn enter(mouse: bool) -> Result<Tui> {
    enable_raw_mode().context("enabling raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("entering the alternate screen")?;
    if mouse {
        execute!(stdout, EnableMouseCapture).context("capturing the mouse")?;
    }
    Terminal::new(CrosstermBackend::new(stdout)).context("creating the terminal")
}

/// Undoes everything [`enter`] did.
///
/// Deliberately takes no terminal and holds no borrow, so the panic hook can
/// call it too. Safe to call when the TUI was never entered, and safe to call
/// twice — both operations are no-ops in that case.
fn restore() -> Result<()> {
    disable_raw_mode().context("disabling raw mode")?;
    // Releasing the mouse unconditionally: it is harmless when it was never
    // captured, and leaving a terminal in mouse-reporting mode is miserable —
    // every click prints escape codes at the shell prompt.
    execute!(
        io::stdout(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    )
    .context("leaving the alternate screen")
}

/// Restores the terminal before a panic reaches the default handler.
///
/// Without this a panic unwinds straight past [`restore`], leaving raw mode on
/// and the alternate screen active — the message lands on a screen the user is
/// about to lose, and their shell is unusable until they type a blind `reset`.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best effort: we are already panicking, so a failure here must not
        // shadow the panic the user actually needs to see.
        let _ = restore();
        default(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_key_presses_are_acted_on() {
        assert!(handles(KeyEventKind::Press));
        // Windows sends both; acting on Release would double every keystroke.
        assert!(!handles(KeyEventKind::Release));
        // A held key repeats; a reader should not scroll twice per repeat tick.
        assert!(!handles(KeyEventKind::Repeat));
    }
}
