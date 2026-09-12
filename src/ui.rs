use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::app::{App, Pane};

const HELP: &str =
    " q quit · Tab · j/k move · / search · u unread · m/a/A read · o open · r refresh ";

pub fn draw(frame: &mut Frame, app: &mut App, keymap: &crate::keys::Keymap) {
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

    // Last, so it covers everything else.
    if app.help_open {
        draw_help(frame, keymap, frame.area());
    }
}

fn draw_feeds(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = app.feed_rows();
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            crate::app::FeedRow::Group { name, collapsed } => ListItem::new(Line::from(vec![
                Span::styled(
                    if *collapsed { "▸ " } else { "▾ " },
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    name.clone(),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            crate::app::FeedRow::Feed(index) => {
                let feed = &app.feeds[*index];
                let unread = app.unread(*index);
                // Indent feeds that sit under a heading.
                let indent = if app.tag_of(*index).is_some() {
                    "  "
                } else {
                    ""
                };
                let mut spans = vec![Span::raw(format!("{indent}{}", feed.title))];
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
            }
        })
        .collect();

    let mut state = ListState::default();
    state
        .select(rows.iter().position(
            |row| matches!(row, crate::app::FeedRow::Feed(i) if *i == app.selected_feed),
        ));

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
        draw_cross_feed(
            frame,
            app,
            &search.results,
            search.selected,
            &if search.typing {
                format!("Search: {}_", search.query)
            } else {
                format!(
                    "Search: {}  ({} matches)",
                    search.query,
                    search.results.len()
                )
            },
            if search.query.is_empty() {
                "Type to search every feed."
            } else {
                "No matches."
            },
            area,
        );
        return;
    }
    if app.starred_view {
        let results = app.starred_results();
        let title = format!("Starred  ({})", results.len());
        draw_cross_feed(
            frame,
            app,
            &results,
            0,
            &title,
            "Nothing starred yet.",
            area,
        );
        return;
    }
    let title = app
        .current_feed()
        .map(|feed| feed.url.clone())
        .unwrap_or_else(|| "Entries".into());
    app.entries_viewport = (area.width.saturating_sub(2), area.height.saturating_sub(2));
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
            let star = if app.is_starred(entry) {
                Span::styled("★ ", Style::default().fg(Color::Yellow))
            } else {
                Span::raw("  ")
            };
            ListItem::new(Line::from(vec![
                Span::styled(entry.date_label(), Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                star,
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

/// The key reference, generated from [`crate::keys`] so it cannot drift.
///
/// Section headings cost four rows plus the blank lines between them, which is
/// more than an 80x24 terminal can spare once every binding is listed. So they
/// are shown when there is room and dropped when there is not: on a small
/// terminal the bindings themselves matter more than the grouping.
fn draw_help(frame: &mut Frame, keymap: &crate::keys::Keymap, area: Rect) {
    let width = crate::keys::key_column_width(keymap);
    let row = |keys: &str, description: &str| {
        Line::from(vec![
            Span::styled(
                format!("  {keys:width$}  "),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(description.to_string()),
        ])
    };

    let sections = keymap.sections();

    let mut roomy = Vec::new();
    for (name, rows) in &sections {
        if !roomy.is_empty() {
            roomy.push(Line::raw(""));
        }
        roomy.push(Line::from(Span::styled(
            *name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        roomy.extend(rows.iter().map(|(k, d)| row(k, d)));
    }

    let compact: Vec<Line> = sections
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .map(|(k, d)| row(k, d))
        .collect();

    let available = area.height.saturating_sub(2) as usize;
    let lines = if roomy.len() <= available {
        roomy
    } else {
        compact
    };

    let content_width = sections
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .map(|(_, description)| 2 + width + 2 + description.len() + 2)
        .max()
        .unwrap_or(40) as u16;

    let height = (lines.len() as u16 + 2).min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(content_width.min(area.width)) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: content_width.min(area.width),
        height,
    };

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                // In the title rather than a row of its own: with every action
                // listed, an 80x24 terminal has no spare line to give it.
                .title(" Keys — any key to dismiss "),
        ),
        popup,
    );
}

/// The entries pane, listing entries drawn from every feed rather than one.
fn draw_cross_feed(
    frame: &mut Frame,
    app: &App,
    results: &[(usize, usize)],
    selected: usize,
    title: &str,
    empty_message: &str,
    area: Rect,
) {
    let items: Vec<ListItem> = results
        .iter()
        .filter_map(|(f, e)| {
            let feed = app.feeds.get(*f)?;
            let entry = feed.entries.get(*e)?;
            Some(ListItem::new(Line::from(vec![
                // Results span feeds, so each row has to say which one.
                Span::styled(
                    format!("{}  ", feed.title),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(entry.title.clone()),
            ])))
        })
        .collect();

    let empty = items.is_empty();
    let items = if empty {
        vec![ListItem::new(Line::from(Span::styled(
            empty_message,
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        items
    };

    let mut state = ListState::default();
    state.select((!empty).then_some(selected));

    frame.render_stateful_widget(
        List::new(items)
            .block(block(title, true))
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
            .draw(|frame| draw(frame, app, &crate::keys::Keymap::default()))
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
            .draw(|frame| draw(frame, app, &crate::keys::Keymap::default()))
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

    /// Renders at a given terminal size and returns the whole screen.
    fn render_at(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("test terminal should build");
        terminal
            .draw(|frame| draw(frame, app, &crate::keys::Keymap::default()))
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
    fn every_binding_is_visible_on_an_eighty_by_twentyfour_terminal() {
        let mut app = App::new(Vec::new(), ReadState::default());
        app.help_open = true;
        let screen = render_at(&mut app, 80, 24);

        let keymap = crate::keys::Keymap::default();
        for (_, rows) in keymap.sections() {
            for (_, description) in rows {
                assert!(
                    screen.contains(description),
                    "the smallest supported terminal cuts off {description}"
                );
            }
        }
    }

    #[test]
    fn a_roomy_terminal_also_gets_the_section_headings() {
        let mut app = App::new(Vec::new(), ReadState::default());
        app.help_open = true;
        let screen = render_at(&mut app, 100, 40);

        let keymap = crate::keys::Keymap::default();
        for (name, rows) in keymap.sections() {
            assert!(screen.contains(name), "missing {name}");
            for (_, description) in rows {
                assert!(screen.contains(description), "missing {description}");
            }
        }
    }

    #[test]
    fn the_help_overlay_says_how_to_dismiss_itself() {
        let mut app = App::new(Vec::new(), ReadState::default());
        app.help_open = true;
        assert!(render_at(&mut app, 80, 24).contains("any key to dismiss"));
        assert!(render_at(&mut app, 120, 44).contains("any key to dismiss"));
    }

    #[test]
    fn the_help_overlay_is_absent_until_asked_for() {
        let mut app = App::new(Vec::new(), ReadState::default());
        assert!(!render(&mut app).contains("any key to dismiss"));
    }

    #[test]
    fn renders_without_panicking_when_there_are_no_feeds() {
        let mut app = App::new(Vec::new(), ReadState::default());
        let screen = render(&mut app);
        assert!(screen.contains("No entry selected."));
    }
}
