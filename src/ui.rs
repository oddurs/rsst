use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Pane};

const HELP: &str = " q quit · Tab switch pane · j/k move · r refresh ";

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
        .map(|feed| ListItem::new(feed.title.clone()))
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
    let entries = app.current_feed().map(|feed| &feed.entries);
    let items: Vec<ListItem> = entries
        .into_iter()
        .flatten()
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::styled(entry.date_label(), Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::raw(entry.title.clone()),
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
        let mut app = App::new(vec![Feed {
            title: "Rust Blog".into(),
            url: "https://blog.rust-lang.org/feed.xml".into(),
            entries: vec![Entry {
                title: "Announcing Rust".into(),
                link: Some("https://example.com/post".into()),
                published: None,
                summary: "A summary body.".into(),
            }],
        }]);

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
    fn renders_without_panicking_when_there_are_no_feeds() {
        let mut app = App::new(Vec::new());
        let screen = render(&mut app);
        assert!(screen.contains("No entry selected."));
    }
}
