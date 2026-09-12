mod app;
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
use tokio::task::JoinSet;

use crate::app::App;
use crate::cli::Action;
use crate::config::{Config, FeedSource};
use crate::feed::Feed;
use crate::state::ReadState;

const TICK: Duration = Duration::from_millis(250);

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
    let mut app = App::new(
        fetch_all(&client, &config).await,
        ReadState::load(&state_path),
    );

    install_panic_hook();
    let mut terminal = enter()?;
    let result = run(&mut terminal, &mut app, &client, &config, &state_path).await;
    restore()?;

    // Save even when the loop failed: the user still read those entries, and
    // losing that is more annoying than whatever went wrong.
    if let Err(err) = app.read.save(&state_path) {
        eprintln!("rsst: could not save read state: {err:#}");
    }
    result
}

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

async fn run(
    terminal: &mut Tui,
    app: &mut App,
    client: &reqwest::Client,
    config: &Config,
    state_path: &std::path::Path,
) -> Result<()> {
    while !app.should_quit {
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
                app.status = Some(" Refreshing… ".into());
                terminal.draw(|frame| ui::draw(frame, app))?;
                let _ = app.read.save(state_path);
                app.feeds = fetch_all(client, config).await;
                app.selected_feed = app.selected_feed.min(app.feeds.len().saturating_sub(1));
                app.selected_entry = 0;
                app.status = None;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Fetches every configured feed concurrently, reporting failures inline.
async fn fetch_all(client: &reqwest::Client, config: &Config) -> Vec<Feed> {
    let mut tasks = JoinSet::new();
    for (index, source) in config.feeds.iter().cloned().enumerate() {
        let client = client.clone();
        tasks.spawn(async move {
            let result = feed::fetch(&client, &source).await;
            (index, source, result)
        });
    }

    let mut slots: Vec<Option<Feed>> = vec![None; config.feeds.len()];
    while let Some(joined) = tasks.join_next().await {
        let (index, source, result) = match joined {
            Ok(output) => output,
            // Only a panic in the fetch task can land here; the feed is simply dropped.
            Err(_) => continue,
        };
        slots[index] = Some(result.unwrap_or_else(|err| placeholder(&source, &err)));
    }

    slots.into_iter().flatten().collect()
}

/// Stands in for a feed that failed to load, so one dead URL can't hide the rest.
fn placeholder(source: &FeedSource, err: &anyhow::Error) -> Feed {
    Feed {
        title: format!("{} (error)", source.title.as_deref().unwrap_or(&source.url)),
        url: source.url.clone(),
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
