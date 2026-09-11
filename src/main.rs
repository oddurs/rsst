mod app;
mod config;
mod feed;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::task::JoinSet;

use crate::app::App;
use crate::config::{Config, FeedSource};
use crate::feed::Feed;

const TICK: Duration = Duration::from_millis(250);

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load_or_init()?;
    if config.feeds.is_empty() {
        let path = config::config_path()?;
        eprintln!("No feeds configured. Add some to {}.", path.display());
        return Ok(());
    }

    let client = http_client()?;
    let mut app = App::new(fetch_all(&client, &config).await);

    let mut terminal = enter()?;
    let result = run(&mut terminal, &mut app, &client, &config).await;
    leave(&mut terminal)?;
    result
}

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

async fn run(
    terminal: &mut Tui,
    app: &mut App,
    client: &reqwest::Client,
    config: &Config,
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
            KeyCode::Char('r') => {
                app.status = Some(" Refreshing… ".into());
                terminal.draw(|frame| ui::draw(frame, app))?;
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
        }],
    }
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

fn leave(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode().context("disabling raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("leaving the alternate screen")?;
    terminal.show_cursor().context("restoring the cursor")
}
