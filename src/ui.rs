use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Pane};

const HELP: &str = " q quit · Tab pane · j/k move · o open · y copy · r refresh ";

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
    let title = app
        .current_feed()
        .map(|feed| feed.url.clone())
        .unwrap_or_else(|| "Entries".into());
    let items: Vec<ListItem> = app
        .current_feed()
        .map(|feed| &feed.entries)
        .into_iter()
        .flatten()
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
    state.select((!items.is_empty()).then_some(app.selected_entry));

    frame.render_stateful_widget(
        List::new(items)
            .block(block(&title, app.focus == Pane::Entries))
            .highlight_style(highlight())
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let body = match app.current_entry() {
        Some(entry) => {
            let mut lines = vec![
                Line::from(Span::styled(
                    entry.title.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    entry.link.clone().unwrap_or_default(),
                    Style::default().fg(Color::Blue),
                )),
                Line::raw(""),
            ];
            lines.push(Line::raw(entry.summary.clone()));
            lines
        }
        None => vec![Line::from(Span::styled(
            "No entry selected.",
            Style::default().fg(Color::DarkGray),
        ))],
    };

    frame.render_widget(
        Paragraph::new(body)
            .block(block("Detail", false))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let text = app.status.clone().unwrap_or_else(|| HELP.to_string());
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Black).bg(Color::Cyan)),
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

    #[test]
    fn renders_without_panicking_when_there_are_no_feeds() {
        let mut app = App::new(Vec::new(), ReadState::default());
        let screen = render(&mut app);
        assert!(screen.contains("No entry selected."));
    }
}
