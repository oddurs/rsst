use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, Pane};

const HELP: &str =
    " q quit · Tab · j/k move · / search · u unread · m/a/A read · o open · r refresh ";

pub fn draw(frame: &mut Frame, app: &mut App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(rows[0]);

    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(columns[1]);

    draw_feeds(frame, app, columns[0]);
    draw_entries(frame, app, panes[0]);
    draw_detail(frame, app, panes[1]);
    draw_status(frame, app, rows[1]);
}

fn draw_feeds(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .feeds
        .iter()
        .enumerate()
        .map(|(index, feed)| {
            let unread = app.unread(index);
            let mut spans = vec![Span::raw(feed.title.clone())];
            if let Some(marker) = feed.status.marker() {
                let colour = if feed.status.error().is_some() {
                    Color::Red
                } else {
                    Color::DarkGray
                };
                spans.push(Span::styled(
                    format!("  {marker}"),
                    Style::default().fg(colour),
                ));
            }
            // Only worth the space when there is something to report.
            if unread > 0 {
                spans.push(Span::styled(
                    format!("  {unread}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut state = ListState::default();
    state.select((!app.feeds.is_empty()).then_some(app.selected_feed));

    frame.render_stateful_widget(
        List::new(items)
            .block(block("Feeds", app.focus == Pane::Feeds))
            .highlight_style(highlight())
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

fn draw_entries(frame: &mut Frame, app: &mut App, area: Rect) {
    if let Some(search) = app.search.clone() {
        draw_search_results(frame, app, &search, area);
        return;
    }
    let title = app
        .current_feed()
        .map(|feed| feed.url.clone())
        .unwrap_or_else(|| "Entries".into());
    let visible = app.visible_indices(app.selected_feed);
    let entries = app.current_feed().map(|feed| &feed.entries);
    let items: Vec<ListItem> = visible
        .iter()
        .filter_map(|index| entries.and_then(|entries| entries.get(*index)))
        .map(|entry| {
            // Unread stands out; read recedes rather than disappearing.
            let title = if app.is_read(entry) {
                Span::styled(entry.title.clone(), Style::default().fg(Color::DarkGray))
            } else {
                Span::styled(
                    entry.title.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                )
            };
            ListItem::new(Line::from(vec![
                Span::styled(entry.date_label(), Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                title,
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(visible.iter().position(|i| *i == app.selected_entry));

    let title = if app.read.unread_only {
        format!("{title}  [unread]")
    } else {
        title
    };
    let empty = items.is_empty();
    let items = if empty {
        vec![ListItem::new(Line::from(Span::styled(
            "Nothing unread here.",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        items
    };

    frame.render_stateful_widget(
        List::new(items)
            .block(block(&title, app.focus == Pane::Entries))
            .highlight_style(highlight())
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

/// The entries pane, showing matches from every feed rather than one.
fn draw_search_results(frame: &mut Frame, app: &App, search: &crate::app::Search, area: Rect) {
    let items: Vec<ListItem> = search
        .results
        .iter()
        .filter_map(|(f, e)| {
            let feed = app.feeds.get(*f)?;
            let entry = feed.entries.get(*e)?;
            Some(ListItem::new(Line::from(vec![
                // Matches span feeds, so each result has to say which one.
                Span::styled(
                    format!("{}  ", feed.title),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(entry.title.clone()),
            ])))
        })
        .collect();

    let title = if search.typing {
        format!("Search: {}_", search.query)
    } else {
        format!(
            "Search: {}  ({} matches)",
            search.query,
            search.results.len()
        )
    };

    let items = if items.is_empty() {
        let message = if search.query.is_empty() {
            "Type to search every feed."
        } else {
            "No matches."
        };
        vec![ListItem::new(Line::from(Span::styled(
            message,
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        items
    };

    let mut state = ListState::default();
    state.select((!search.results.is_empty()).then_some(search.selected));

    frame.render_stateful_widget(
        List::new(items)
            .block(block(&title, true))
            .highlight_style(highlight())
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

fn draw_detail(frame: &mut Frame, app: &mut App, area: Rect) {
    // Tell the app how much room it has, so it can wrap and clamp scrolling.
    // `block` takes one column of border on each side, and one row.
    let inner = Rect {
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
        ..area
    };
    app.detail_viewport = (inner.width, inner.height);
    // The pane may have shrunk since the last frame, stranding the offset past
    // the end of the text.
    let max = app.max_detail_scroll();
    app.detail_scroll = app.detail_scroll.min(max);

    let lines: Vec<Line> = app.detail_lines().into_iter().map(Line::raw).collect();

    let title = if max > 0 {
        format!("Detail  {}/{}", app.detail_scroll, max)
    } else {
        "Detail".to_string()
    };

    frame.render_widget(
        // Already wrapped by `App::detail_lines`, so no Wrap here — the scroll
        // offset has to mean the same thing to the app and to the renderer.
        Paragraph::new(lines)
            .block(block(&title, app.focus == Pane::Detail))
            .scroll((app.detail_scroll, 0)),
        area,
    );
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    // A transient message wins; otherwise a failed feed explains itself for as
    // long as it is selected, rather than flashing once and being lost.
    let error = app
        .current_feed()
        .and_then(|feed| feed.status.error())
        .map(|message| (format!(" {message} "), Color::Red));

    if let Some(pending) = app.pending {
        let what = match pending {
            crate::app::Bulk::Feed => "this feed",
            crate::app::Bulk::Everything => "every feed",
        };
        frame.render_widget(
            Paragraph::new(format!(
                " Mark {} unread entries in {what} as read?  y / n ",
                app.pending_count()
            ))
            .style(Style::default().fg(Color::Black).bg(Color::Yellow)),
            area,
        );
        return;
    }

    let (text, background) = match (&app.status, error) {
        (Some(status), _) => (status.clone(), Color::Cyan),
        (None, Some((message, colour))) => (message, colour),
        (None, None) => (HELP.to_string(), Color::Cyan),
    };

    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Black).bg(background)),
        area,
    );
}

fn block(title: &str, focused: bool) -> Block<'_> {
    let border = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(format!(" {title} "))
}

fn highlight() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::feed::{Entry, Feed};
    use crate::state::ReadState;

    fn render(app: &mut App) -> String {
        let mut terminal =
            Terminal::new(TestBackend::new(80, 24)).expect("test terminal should build");
        terminal
            .draw(|frame| draw(frame, app))
            .expect("draw should succeed");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn renders_the_selected_entry_across_all_panes() {
        let mut app = App::new(
            vec![Feed {
                title: "Rust Blog".into(),
                url: "https://blog.rust-lang.org/feed.xml".into(),
                status: crate::feed::Status::Idle,
                entries: vec![
                    Entry {
                        title: "Announcing Rust".into(),
                        link: Some("https://example.com/post".into()),
                        published: None,
                        summary: "A summary body.".into(),
                        keys: vec!["id:one".into()],
                    },
                    Entry {
                        title: "Second Post".into(),
                        link: None,
                        published: None,
                        summary: String::new(),
                        keys: vec!["id:two".into()],
                    },
                ],
            }],
            ReadState::default(),
        );

        let screen = render(&mut app);

        assert!(screen.contains("Rust Blog"), "feed pane lists the feed");
        assert!(
            screen.contains("Announcing Rust"),
            "entry pane lists the entry"
        );
        assert!(
            screen.contains("A summary body."),
            "detail pane shows the summary"
        );
        assert!(screen.contains("q quit"), "status bar shows the help line");
    }

    #[test]
    fn the_feed_pane_shows_an_unread_count_that_shrinks_as_you_read() {
        let mut app = App::new(
            vec![Feed {
                title: "Rust Blog".into(),
                url: "https://blog.rust-lang.org/feed.xml".into(),
                status: crate::feed::Status::Idle,
                entries: vec![
                    Entry {
                        title: "One".into(),
                        link: None,
                        published: None,
                        summary: String::new(),
                        keys: vec!["id:one".into()],
                    },
                    Entry {
                        title: "Two".into(),
                        link: None,
                        published: None,
                        summary: String::new(),
                        keys: vec!["id:two".into()],
                    },
                ],
            }],
            ReadState::default(),
        );

        assert!(render(&mut app).contains("Rust Blog  2"), "two unread");

        app.mark_current_read();
        assert!(render(&mut app).contains("Rust Blog  1"), "one unread");
    }

    #[test]
    fn a_fully_read_feed_shows_no_count_at_all() {
        let mut app = App::new(
            vec![Feed {
                title: "Rust Blog".into(),
                url: "https://blog.rust-lang.org/feed.xml".into(),
                status: crate::feed::Status::Idle,
                entries: vec![Entry {
                    title: "One".into(),
                    link: None,
                    published: None,
                    summary: String::new(),
                    keys: vec!["id:one".into()],
                }],
            }],
            ReadState::default(),
        );
        app.mark_current_read();

        let screen = render(&mut app);
        assert!(screen.contains("Rust Blog"));
        assert!(!screen.contains("Rust Blog  1"), "no count when all read");
    }

    /// Renders, then reports the lines of the detail pane only.
    fn detail_region(app: &mut App) -> String {
        let mut terminal =
            Terminal::new(TestBackend::new(80, 24)).expect("test terminal should build");
        terminal
            .draw(|frame| draw(frame, app))
            .expect("draw should succeed");
        let buffer = terminal.backend().buffer().clone();
        // The detail pane occupies the lower 40% of the right-hand 75%.
        let mut out = String::new();
        for y in 15..23 {
            for x in 21..79 {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn long_entry_app() -> App {
        App::new(
            vec![Feed {
                title: "Feed".into(),
                url: "https://a.example".into(),
                status: crate::feed::Status::Idle,
                entries: vec![Entry {
                    title: "Title".into(),
                    link: None,
                    published: None,
                    summary: (1..=200)
                        .map(|i| format!("word{i}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                    keys: vec!["id:x".into()],
                }],
            }],
            ReadState::default(),
        )
    }

    #[test]
    fn scrolling_the_detail_pane_changes_what_is_shown() {
        let mut app = long_entry_app();
        let before = detail_region(&mut app);

        app.focus = Pane::Detail;
        app.select_next(); // scrolls down one line
        let after = detail_region(&mut app);

        assert_ne!(before, after, "the detail pane did not scroll");
        assert!(
            before.contains("word1 "),
            "first frame starts at the beginning"
        );
        assert!(!after.contains("Title"), "the title scrolled out of view");
    }

    #[test]
    fn the_detail_pane_stops_at_the_last_line() {
        let mut app = long_entry_app();
        detail_region(&mut app); // establishes the viewport

        app.focus = Pane::Detail;
        for _ in 0..500 {
            app.select_next();
        }
        let at_end = detail_region(&mut app);
        let max = app.max_detail_scroll();

        assert_eq!(app.detail_scroll, max, "clamped to the last line");
        for _ in 0..10 {
            app.select_next();
        }
        assert_eq!(
            detail_region(&mut app),
            at_end,
            "scrolling past the end changed the view"
        );
        assert!(at_end.contains("word200"), "the final line is reachable");
    }

    #[test]
    fn a_feed_still_fetching_is_marked_and_stops_being_marked_once_it_lands() {
        let mut app = App::new(
            vec![Feed {
                title: "Slow Feed".into(),
                url: "https://slow.example/feed.xml".into(),
                status: crate::feed::Status::Fetching,
                entries: Vec::new(),
            }],
            ReadState::default(),
        );

        assert!(
            render(&mut app).contains("Slow Feed  …"),
            "shows a loading mark"
        );

        app.feeds[0].status = crate::feed::Status::Idle;
        app.feeds[0].entries = vec![Entry {
            title: "Arrived".into(),
            link: None,
            published: None,
            summary: String::new(),
            keys: vec!["id:a".into()],
        }];

        let screen = render(&mut app);
        assert!(!screen.contains("Slow Feed  …"), "mark cleared once loaded");
        assert!(screen.contains("Arrived"));
    }

    #[test]
    fn a_failed_feed_is_marked_and_explains_itself_without_inventing_entries() {
        let mut app = App::new(
            vec![Feed {
                title: "Broken".into(),
                url: "https://broken.example/feed.xml".into(),
                status: crate::feed::Status::Failed("dns error: no such host".into()),
                entries: Vec::new(),
            }],
            ReadState::default(),
        );

        let screen = render(&mut app);
        assert!(screen.contains("Broken  !"), "marked in the feed list");
        assert!(screen.contains("dns error: no such host"), "error is shown");
        assert!(
            !screen.contains("Failed to load"),
            "no fake entry was invented"
        );
    }

    #[test]
    fn a_fetching_feed_looks_different_from_a_failed_one() {
        let fetching = Feed {
            title: "Feed".into(),
            url: "https://a.example/feed.xml".into(),
            status: crate::feed::Status::Fetching,
            entries: Vec::new(),
        };
        let mut failed = fetching.clone();
        failed.status = crate::feed::Status::Failed("boom".into());

        let mut a = App::new(vec![fetching], ReadState::default());
        let mut b = App::new(vec![failed], ReadState::default());
        assert!(render(&mut a).contains("Feed  …"));
        assert!(render(&mut b).contains("Feed  !"));
    }

    #[test]
    fn a_failed_feed_keeps_showing_its_cached_entries() {
        let mut app = App::new(
            vec![Feed {
                title: "Broken".into(),
                url: "https://broken.example/feed.xml".into(),
                status: crate::feed::Status::Failed("offline".into()),
                entries: vec![Entry {
                    title: "From The Cache".into(),
                    link: None,
                    published: None,
                    summary: String::new(),
                    keys: vec!["id:c".into()],
                }],
            }],
            ReadState::default(),
        );
        assert!(render(&mut app).contains("From The Cache"));
    }

    #[test]
    fn renders_without_panicking_when_there_are_no_feeds() {
        let mut app = App::new(Vec::new(), ReadState::default());
        let screen = render(&mut app);
        assert!(screen.contains("No entry selected."));
    }
}
