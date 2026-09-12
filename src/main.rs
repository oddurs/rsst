mod app;
mod cache;
mod cli;
mod config;
mod feed;
mod launch;
mod opml;
mod state;
mod text;
mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::App;
use crate::cache::Cache;
use crate::cli::Action;
use crate::config::{Config, FeedSource};
use crate::feed::Feed;
use crate::state::ReadState;

// Short enough that a feed landing on the channel is drawn promptly, long
// enough that an idle reader is not busy-waiting.
const TICK: Duration = Duration::from_millis(100);

/// A finished fetch on its way back to the event loop.
type Fetched = (usize, Result<feed::Outcome>);

/// Everything the event loop needs besides the app state itself.
struct Session {
    client: reqwest::Client,
    config: Config,
    state_path: PathBuf,
    cache_path: PathBuf,
    cache: Cache,
    tx: tokio::sync::mpsc::UnboundedSender<Fetched>,
    rx: tokio::sync::mpsc::UnboundedReceiver<Fetched>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = match cli::parse(std::env::args().skip(1))? {
        Action::Help => {
            println!("{}", cli::HELP);
            return Ok(());
        }
        Action::Version => {
            println!("rsst {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Action::Import { path, config } => return import(&path, config),
        Action::Export { config } => return export(config),
        Action::Run { config } => config,
    };

    let config = Config::load_or_init(config_path)?;
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
    let cache_path = cache::cache_path()?;
    let cache = Cache::load(&cache_path);
    spawn_fetches(&client, &config, &cache, &tx);

    // Last known contents stand in until the fetch lands, so a second launch
    // has something to read immediately and an offline one still works.
    let feeds = config
        .feeds
        .iter()
        .map(|source| cache.get(source).unwrap_or_else(|| Feed::pending(source)))
        .collect();
    let mut app = App::new(feeds, ReadState::load(&state_path));

    let mut session = Session {
        client,
        config,
        state_path,
        cache_path,
        cache,
        tx,
        rx,
    };

    install_panic_hook();
    let mut terminal = enter()?;
    let result = run(&mut terminal, &mut app, &mut session).await;
    restore()?;

    // Save even when the loop failed: the user still read those entries, and
    // losing that is more annoying than whatever went wrong.
    if let Err(err) = app.read.save(&session.state_path) {
        eprintln!("rsst: could not save read state: {err:#}");
    }
    session.cache.retain_configured(&session.config.feeds);
    if let Err(err) = session.cache.save(&session.cache_path) {
        eprintln!("rsst: could not save cache: {err:#}");
    }
    result
}

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

async fn run(terminal: &mut Tui, app: &mut App, session: &mut Session) -> Result<()> {
    while !app.should_quit {
        // Take whatever has arrived since the last frame. Never blocks, so a
        // slow feed cannot hold up the redraw.
        while let Ok((index, result)) = session.rx.try_recv() {
            let Some(slot) = app.feeds.get_mut(index) else {
                continue;
            };
            let url = slot.url.clone();
            match result {
                Ok(feed::Outcome::Updated {
                    feed,
                    etag,
                    last_modified,
                }) => {
                    session.cache.put(&feed);
                    session.cache.set_validators(&url, etag, last_modified);
                    *slot = *feed;
                }
                // Nothing was downloaded or reparsed; what is on screen stands.
                Ok(feed::Outcome::NotModified) => slot.loading = false,
                Ok(feed::Outcome::RateLimited { retry_after }) => {
                    let until = chrono::Utc::now()
                        + chrono::Duration::from_std(retry_after)
                            .unwrap_or_else(|_| chrono::Duration::seconds(300));
                    session.cache.defer_until(&url, until);
                    slot.loading = false;
                    app.status = Some(format!(
                        " {}: rate limited, waiting {}s ",
                        slot.title,
                        retry_after.as_secs()
                    ));
                }
                // Keep whatever is already on screen. When that came from the
                // cache it is exactly what makes the reader usable offline;
                // replacing it with an error notice would throw it away.
                Err(err) if !slot.entries.is_empty() => {
                    slot.loading = false;
                    app.status = Some(format!(" {}: {err} ", slot.title));
                }
                Err(err) => *slot = placeholder(&session.config.feeds[index], &err),
            }
        }

        terminal.draw(|frame| ui::draw(frame, app))?;

        if !event::poll(TICK)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        app.status = None;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            KeyCode::Tab | KeyCode::BackTab => app.toggle_focus(),
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_previous(),
            KeyCode::Char('o') => open_selected(app),
            KeyCode::Char('y') => copy_selected(app),
            KeyCode::Char('r') => {
                let _ = app.read.save(&session.state_path);
                let starting = app.begin_refresh();
                app.status = Some(if starting.is_empty() {
                    " Already refreshing… ".into()
                } else {
                    format!(" Refreshing {} feed(s)… ", starting.len())
                });
                spawn_some(
                    &session.client,
                    &session.config,
                    &session.cache,
                    &session.tx,
                    starting,
                );
            }
            _ => {}
        }
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
    cache: &Cache,
    tx: &tokio::sync::mpsc::UnboundedSender<Fetched>,
) {
    spawn_some(client, config, cache, tx, 0..config.feeds.len());
}

/// Starts a fetch for each of `indices`.
fn spawn_some(
    client: &reqwest::Client,
    config: &Config,
    cache: &Cache,
    tx: &tokio::sync::mpsc::UnboundedSender<Fetched>,
    indices: impl IntoIterator<Item = usize>,
) {
    let now = chrono::Utc::now();
    for index in indices {
        let Some(source) = config.feeds.get(index).cloned() else {
            continue;
        };
        // Honour a server that asked us to wait rather than hammering it.
        if !cache.may_fetch(&source.url, now) {
            let _ = tx.send((index, Ok(feed::Outcome::NotModified)));
            continue;
        }
        let meta = cache.meta(&source.url);
        let client = client.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = feed::fetch(
                &client,
                &source,
                meta.etag.as_deref(),
                meta.last_modified.as_deref(),
            )
            .await;
            // A closed channel means the reader has already quit.
            let _ = tx.send((index, result));
        });
    }
}

/// Stands in for a feed that failed to load, so one dead URL can't hide the rest.
fn placeholder(source: &FeedSource, err: &anyhow::Error) -> Feed {
    Feed {
        title: format!("{} (error)", source.title.as_deref().unwrap_or(&source.url)),
        url: source.url.clone(),
        loading: false,
        entries: vec![feed::Entry {
            title: format!("Failed to load: {err}"),
            link: Some(source.url.clone()),
            published: None,
            summary: format!("{err:#}"),
            // No keys: a failure notice must never be remembered as read.
            keys: Vec::new(),
        }],
    }
}

/// Opens the selected entry's link in the browser, reporting the outcome.
fn open_selected(app: &mut App) {
    let Some(link) = app.current_entry().and_then(|entry| entry.link.clone()) else {
        app.status = Some(" This entry has no link. ".into());
        return;
    };
    app.status = Some(match launch::browser(&link) {
        Ok(()) => format!(" Opened {link} "),
        Err(err) => format!(" Could not open: {err:#} "),
    });
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
        .user_agent(concat!("rsst/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()
        .context("building the HTTP client")
}

fn enter() -> Result<Tui> {
    enable_raw_mode().context("enabling raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("entering the alternate screen")?;
    Terminal::new(CrosstermBackend::new(stdout)).context("creating the terminal")
}

/// Undoes everything [`enter`] did.
///
/// Deliberately takes no terminal and holds no borrow, so the panic hook can
/// call it too. Safe to call when the TUI was never entered, and safe to call
/// twice — both operations are no-ops in that case.
fn restore() -> Result<()> {
    disable_raw_mode().context("disabling raw mode")?;
    execute!(io::stdout(), LeaveAlternateScreen, Show).context("leaving the alternate screen")
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
