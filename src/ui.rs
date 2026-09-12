use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::app::{App, Pane};

pub fn draw(frame: &mut Frame, app: &mut App, keymap: &crate::keys::Keymap) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    // A column of gap rather than two adjacent borders, which would put a
    // two-wide band of chrome down the full height of the screen.
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_width(app, rows[0].width)),
            Constraint::Length(1),
            Constraint::Min(20),
        ])
        .split(rows[0]);

    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints(entry_and_article(app, columns[2].height))
        .split(columns[2]);

    // Reset the hit map each frame: it describes this frame, not the last one.
    app.hits = crate::mouse::Hits {
        feeds_pane: inner(columns[0]),
        entries_pane: inner(panes[0]),
        detail_pane: inner(panes[1]),
        status_bar: rows[1],
        help_open: app.help_open,
        ..Default::default()
    };

    draw_feeds(frame, app, columns[0]);
    draw_entries(frame, app, panes[0]);
    draw_detail(frame, app, panes[1]);
    app.hits.buttons = draw_status(frame, app, rows[1]);

    // Last, so they cover everything else.
    if let Some(moving) = app.moving.clone() {
        draw_move(frame, app, &moving, frame.area());
    }
    if app.help_open {
        draw_help(frame, keymap, &app.theme, app.help_scroll, frame.area());
    }
}

fn draw_feeds(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = app.feed_rows();
    // No cursor gutter in this pane, so the full inner width is the row's.
    let width = inner(area).width;
    let items: Vec<ListItem> = rows.iter().map(|row| feed_row(app, row, width)).collect();

    let mut state = ListState::default();
    state.select(rows.iter().position(
        |row| matches!(row, crate::tree::Row::Feed { index, .. } if *index == app.selected_feed),
    ));

    let unread: usize = (0..app.feeds.len()).map(|index| app.unread(index)).sum();
    let title = if unread > 0 {
        format!("Feeds  {unread} unread")
    } else {
        "Feeds".to_string()
    };

    frame.render_stateful_widget(
        List::new(items)
            .block(block(&title, app.focus == Pane::Feeds, &app.theme))
            // No cursor glyph here: it would sit between the border and the
            // guides, breaking the one column the tree depends on. The
            // selection is shown by reversing the row instead.
            .highlight_style(
                app.theme
                    .accent
                    .add_modifier(Modifier::REVERSED | Modifier::BOLD),
            ),
        area,
        &mut state,
    );

    // Read the offset back out of the widget rather than recomputing it. The
    // list decides how far it scrolled; a second implementation here would be
    // right until it was not.
    let body = inner(area);
    app.hits.feed_rows = rows
        .into_iter()
        .enumerate()
        .skip(state.offset())
        .take(body.height as usize)
        .map(|(index, row)| ((index - state.offset()) as u16 + body.y, row))
        .collect();
}

/// The indent for a row, drawn as guides down the levels above it.
///
/// A guide is drawn where an ancestor still has siblings below, and blank
/// where it does not — so the lines stop at the last item of each branch
/// rather than running to the bottom of the pane.
fn guides(theme: &crate::theme::Theme, levels: &[bool]) -> Vec<Span<'static>> {
    let bar = if theme.ascii { "| " } else { "│ " };
    levels
        .iter()
        .map(|continues| {
            if *continues {
                Span::styled(bar, theme.dim)
            } else {
                Span::raw("  ")
            }
        })
        .collect()
}

/// One row of the sidebar: a folder and its total, or a feed and its count.
fn feed_row<'a>(app: &App, row: &'a crate::tree::Row, width: u16) -> ListItem<'a> {
    let mut spans;
    let indent;
    let (label, right, right_style);

    match row {
        crate::tree::Row::Folder {
            path,
            collapsed,
            unread,
            guides: levels,
        } => {
            spans = guides(&app.theme, levels);
            indent = levels.len() * 2 + 2;
            spans.push(Span::styled(
                match (*collapsed, app.theme.ascii) {
                    (true, false) => "▸ ",
                    (false, false) => "▾ ",
                    (true, true) => "> ",
                    (false, true) => "v ",
                },
                app.theme.dim,
            ));
            label = path.last().cloned().unwrap_or_default();
            // A folded folder still says how much is inside, or folding it
            // would hide the only reason to open it again.
            right = if *unread > 0 {
                unread.to_string()
            } else {
                String::new()
            };
            right_style = app.theme.group;
        }
        crate::tree::Row::Feed {
            index,
            guides: levels,
        } => {
            let feed = &app.feeds[*index];
            let unread = app.unread(*index);
            spans = guides(&app.theme, levels);
            indent = levels.len() * 2 + 2;
            spans.push(Span::raw("  "));
            label = feed.title.clone();

            let marker = feed
                .status
                .marker()
                .map(|marker| match (marker, app.theme.ascii) {
                    ("…", true) => "..",
                    (other, _) => other,
                });
            let count = if unread > 0 {
                unread.to_string()
            } else {
                String::new()
            };
            right = match marker {
                Some(marker) => format!("{marker} {count}"),
                None => count,
            };
            right_style = if feed.status.error().is_some() {
                app.theme.error
            } else if unread > 0 {
                app.theme.accent.add_modifier(Modifier::BOLD)
            } else {
                app.theme.dim
            };
        }
    }

    let room = (width as usize).saturating_sub(indent + right.chars().count() + 1);
    let text = truncate(&label, room, app.theme.ascii);
    let padding = room.saturating_sub(text.chars().count()) + 1;

    let is_folder = matches!(row, crate::tree::Row::Folder { .. });
    let has_unread = !right.trim().is_empty();
    spans.push(if is_folder {
        Span::styled(text, app.theme.group)
    } else if has_unread {
        Span::raw(text)
    } else {
        Span::styled(text, app.theme.dim)
    });
    spans.push(Span::raw(" ".repeat(padding)));
    spans.push(Span::styled(right, right_style));

    ListItem::new(Line::from(spans))
}

fn draw_entries(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut rows_out: Vec<(u16, (usize, usize))> = Vec::new();
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
            &mut rows_out,
        );
        app.hits.entry_rows = rows_out;
        return;
    }
    if app.all_feeds_view {
        let rows = app.all_entries();
        let order = if app.read.oldest_first {
            "oldest first"
        } else {
            "newest first"
        };
        let title = format!("All feeds  ({}, {order})", rows.len());
        let selected = rows
            .iter()
            .position(|(f, e)| *f == app.selected_feed && *e == app.selected_entry)
            .unwrap_or(0);
        draw_cross_feed(
            frame,
            app,
            &rows,
            selected,
            &title,
            "No entries yet.",
            area,
            &mut rows_out,
        );
        app.hits.entry_rows = rows_out;
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
            &mut rows_out,
        );
        app.hits.entry_rows = rows_out;
        return;
    }
    // The feed's name, not its URL. The URL is the least useful string
    // available and this is the most prominent label on screen.
    let title = match app.current_feed() {
        Some(feed) => {
            let unread = app.unread(app.selected_feed);
            let total = feed.entries.len();
            match (total, unread) {
                (0, _) => feed.title.clone(),
                (total, 0) => format!("{}  {total}", feed.title),
                (total, unread) => format!("{}  {unread} of {total} unread", feed.title),
            }
        }
        None => "Entries".into(),
    };
    app.entries_viewport = (area.width.saturating_sub(2), area.height.saturating_sub(2));
    let visible = app.visible_indices(app.selected_feed);
    let entries = app.current_feed().map(|feed| &feed.entries);
    let width = inner(area).width.saturating_sub(CURSOR_WIDTH);
    let items: Vec<ListItem> = visible
        .iter()
        .filter_map(|index| entries.and_then(|entries| entries.get(*index)))
        .map(|entry| entry_row(app, entry, width))
        .collect();

    let mut state = ListState::default();
    state.select(visible.iter().position(|i| *i == app.selected_entry));

    let title = if app.read.unread_only {
        let separator = if app.theme.ascii { "  - " } else { "  · " };
        format!("{title}{separator}unread only")
    } else {
        title
    };
    let empty = items.is_empty();
    let items = if empty {
        vec![ListItem::new(Line::from(Span::styled(
            "Nothing unread here.",
            app.theme.dim,
        )))]
    } else {
        items
    };

    frame.render_stateful_widget(
        List::new(items)
            .block(block(&title, app.focus == Pane::Entries, &app.theme))
            .highlight_style(highlight(&app.theme))
            .highlight_symbol(cursor(&app.theme)),
        area,
        &mut state,
    );

    let body = inner(area);
    let feed = app.selected_feed;
    app.hits.entry_rows = visible
        .into_iter()
        .enumerate()
        .skip(state.offset())
        .take(body.height as usize)
        .map(|(row, entry)| ((row - state.offset()) as u16 + body.y, (feed, entry)))
        .collect();
}

/// Borders drawn with characters every terminal and font has.
const ASCII_BORDER: ratatui::symbols::border::Set<'static> = ratatui::symbols::border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

/// Box-drawing characters, or ASCII where those would not render.
fn border_set(theme: &crate::theme::Theme) -> ratatui::symbols::border::Set<'static> {
    if theme.ascii {
        ASCII_BORDER
    } else {
        ratatui::symbols::border::PLAIN
    }
}

/// The "move to folder" picker.
fn draw_move(frame: &mut Frame, app: &App, moving: &crate::app::Move, area: Rect) {
    let name = app
        .feeds
        .get(moving.feed)
        .map(|feed| feed.title.as_str())
        .unwrap_or("this feed");

    let items: Vec<ListItem> = moving
        .choices
        .iter()
        .map(|choice| {
            let label = choice.label(&moving.typed);
            let style = match choice {
                crate::app::MoveTarget::New => app.theme.accent,
                _ => Style::default(),
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let widest = moving
        .choices
        .iter()
        .map(|choice| choice.label(&moving.typed).chars().count())
        .max()
        .unwrap_or(20);
    let width = (widest as u16 + 10)
        .max(name.chars().count() as u16 + 14)
        .min(area.width.saturating_sub(4));
    let height = (moving.choices.len() as u16 + 2).min(area.height.saturating_sub(2));

    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let mut state = ListState::default();
    state.select(Some(moving.selected));

    frame.render_widget(Clear, popup);
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .border_set(border_set(&app.theme))
                    .borders(Borders::ALL)
                    .border_style(app.theme.accent)
                    .padding(ratatui::widgets::Padding::horizontal(1))
                    .title(format!(" Move {name} to ")),
            )
            .highlight_style(
                app.theme
                    .accent
                    .add_modifier(Modifier::REVERSED | Modifier::BOLD),
            ),
        popup,
        &mut state,
    );
}

/// The key reference, generated from [`crate::keys`] so it cannot drift.
///
/// Section headings cost four rows plus the blank lines between them, which is
/// more than an 80x24 terminal can spare once every binding is listed. So they
/// are shown when there is room and dropped when there is not: on a small
/// terminal the bindings themselves matter more than the grouping.
fn draw_help(
    frame: &mut Frame,
    keymap: &crate::keys::Keymap,
    theme: &crate::theme::Theme,
    scroll: u16,
    area: Rect,
) {
    let width = crate::keys::key_column_width(keymap);
    let row = |keys: &str, description: &str| {
        Line::from(vec![
            Span::styled(format!("  {keys:width$}  "), theme.star),
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
            theme.accent.add_modifier(Modifier::BOLD),
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

    // More actions than rows is normal on a small terminal, so the overlay
    // scrolls rather than silently cutting the list off.
    let overflow = lines.len().saturating_sub(available);
    let scroll = scroll.min(overflow as u16);

    let content_width = sections
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        // Two for the border and two for its padding.
        .map(|(_, description)| 2 + width + 2 + description.len() + 4)
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
        Paragraph::new(lines).scroll((scroll, 0)).block(
            Block::default()
                .border_set(border_set(theme))
                .borders(Borders::ALL)
                .border_style(theme.accent)
                .padding(ratatui::widgets::Padding::horizontal(1))
                // In the title rather than a row of its own: with every action
                // listed, an 80x24 terminal has no spare line to give it.
                .title(match (overflow > 0, theme.ascii) {
                    (true, false) => " Keys — j/k to scroll, any other key closes ",
                    (false, false) => " Keys — any key to dismiss ",
                    (true, true) => " Keys - j/k to scroll, any other key closes ",
                    (false, true) => " Keys - any key to dismiss ",
                }),
        ),
        popup,
    );
}

/// The entries pane, listing entries drawn from every feed rather than one.
#[allow(clippy::too_many_arguments)]
fn draw_cross_feed(
    frame: &mut Frame,
    app: &App,
    results: &[(usize, usize)],
    selected: usize,
    title: &str,
    empty_message: &str,
    area: Rect,
    rows_out: &mut Vec<(u16, (usize, usize))>,
) {
    let items: Vec<ListItem> = results
        .iter()
        .filter_map(|(f, e)| {
            let feed = app.feeds.get(*f)?;
            let entry = feed.entries.get(*e)?;
            Some(ListItem::new(Line::from(vec![
                // Results span feeds, so each row has to say which one.
                Span::styled(format!("{}  ", feed.title), app.theme.dim),
                Span::raw(entry.title.clone()),
            ])))
        })
        .collect();

    let empty = items.is_empty();
    let items = if empty {
        vec![ListItem::new(Line::from(Span::styled(
            empty_message,
            app.theme.dim,
        )))]
    } else {
        items
    };

    let mut state = ListState::default();
    state.select((!empty).then_some(selected));

    frame.render_stateful_widget(
        List::new(items)
            .block(block(title, true, &app.theme))
            .highlight_style(highlight(&app.theme))
            .highlight_symbol(cursor(&app.theme)),
        area,
        &mut state,
    );

    let body = inner(area);
    rows_out.extend(
        results
            .iter()
            .enumerate()
            .skip(state.offset())
            .take(body.height as usize)
            .map(|(row, target)| ((row - state.offset()) as u16 + body.y, *target)),
    );
}

fn draw_detail(frame: &mut Frame, app: &mut App, area: Rect) {
    // The same inner rect the hit map uses, so a click on the link lands on the
    // row the link was drawn on rather than one above it.
    let inner = inner(area);
    app.detail_viewport = (inner.width, inner.height);
    // The pane may have shrunk since the last frame, stranding the offset past
    // the end of the text.
    let max = app.max_detail_scroll();
    app.detail_scroll = app.detail_scroll.min(max);

    let lines: Vec<Line> = app
        .detail_rows()
        .into_iter()
        .map(|row| article_line(app, row))
        .collect();

    // The article says where it came from and when. "Detail" said neither.
    let title = match app.current_entry() {
        Some(entry) => {
            let feed = app
                .feeds
                .get(app.selected_feed)
                .map(|feed| feed.title.as_str())
                .unwrap_or("");
            let date = entry
                .published
                .map(|when| when.format("%-d %B %Y").to_string())
                .unwrap_or_default();
            let separator = if app.theme.ascii { "  -  " } else { "  ·  " };
            match (feed.is_empty(), date.is_empty()) {
                (false, false) => format!("{feed}{separator}{date}"),
                (false, true) => feed.to_string(),
                (true, false) => date,
                (true, true) => "Article".into(),
            }
        }
        None => "Article".into(),
    };
    let title = if max > 0 {
        let separator = if app.theme.ascii { "  -  " } else { "  ·  " };
        format!("{title}{separator}{}%", percent(app.detail_scroll, max))
    } else {
        title
    };

    frame.render_widget(
        // Already wrapped by `App::detail_lines`, so no Wrap here — the scroll
        // offset has to mean the same thing to the app and to the renderer.
        Paragraph::new(lines)
            .block(block(&title, app.focus == Pane::Detail, &app.theme))
            .scroll((app.detail_scroll, 0)),
        area,
    );

    // Where the link ended up on screen, so it can be clicked. Scrolled off the
    // top or past the bottom, it simply is not clickable.
    app.hits.link_rows = match (app.detail_link_lines(), app.current_entry()) {
        (Some((first, count)), Some(entry)) => {
            let link = entry.link.clone().unwrap_or_default();
            let wrapped = crate::text::wrap(&link, inner.width as usize);
            (0..count)
                .filter_map(|i| {
                    let line = (first + i) as u16;
                    let row = line.checked_sub(app.detail_scroll)? + inner.y;
                    (row < inner.y + inner.height).then(|| {
                        let width = wrapped.get(i).map_or(0, |l| l.chars().count()) as u16;
                        (row, inner.x, inner.x + width.saturating_sub(1))
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    };
}

/// Styles one laid-out line of the article.
///
/// The parser decided what each piece *is*; this decides what that looks like,
/// so the theme stays the only place colours are chosen.
fn article_line<'a>(app: &App, row: crate::article::Row) -> Line<'a> {
    use crate::article::{Inline, Kind};

    let mut spans: Vec<Span> = Vec::new();

    // A quote is marked down its left edge rather than indented silently.
    if row.kind == Kind::Quote {
        let rule = if app.theme.ascii { "| " } else { "│ " };
        spans.push(Span::styled(rule, app.theme.accent));
    } else if row.indent > 0 {
        spans.push(Span::raw(" ".repeat(row.indent)));
    }

    if row.kind == Kind::Rule {
        let width = app.detail_viewport.0.max(1) as usize;
        let dash = if app.theme.ascii { "-" } else { "─" };
        spans.push(Span::styled(dash.repeat(width), app.theme.dim));
        return Line::from(spans);
    }

    let base = match row.kind {
        Kind::Heading => Style::default().add_modifier(Modifier::BOLD),
        Kind::Code => app.theme.accent,
        Kind::Quote => app.theme.dim.add_modifier(Modifier::ITALIC),
        Kind::Reference => app.theme.link,
        _ => Style::default(),
    };

    for span in row.spans {
        let style = match &span {
            Inline::Strong(_) => base.add_modifier(Modifier::BOLD),
            Inline::Emphasis(_) => base.add_modifier(Modifier::ITALIC),
            Inline::Code(_) if row.kind != Kind::Code => app.theme.accent,
            Inline::Link(..) => app.theme.link,
            _ => base,
        };
        spans.push(Span::styled(span.text().to_string(), style));
    }
    Line::from(spans)
}

/// How far through the article we are, as a percentage.
fn percent(scroll: u16, max: u16) -> u16 {
    if max == 0 {
        return 100;
    }
    (u32::from(scroll) * 100 / u32::from(max)) as u16
}

/// Draws the status bar and returns the clickable hints it drew.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) -> Vec<(u16, u16, crate::keys::Action)> {
    let mut buttons = Vec::new();
    // A transient message wins; otherwise a failed feed explains itself for as
    // long as it is selected, rather than flashing once and being lost.
    let error = app
        .current_feed()
        .and_then(|feed| feed.status.error())
        .map(|message| (format!(" {message} "), app.theme.error));

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
            .style(app.theme.status(app.theme.warning)),
            area,
        );
        return buttons;
    }

    let (message, tone) = match (&app.status, error) {
        (Some(status), _) => (Some(status.clone()), app.theme.accent),
        (None, Some((message, tone))) => (Some(message), tone),
        (None, None) => (None, app.theme.accent),
    };

    let style = app.theme.status(tone);
    match message {
        Some(text) => frame.render_widget(Paragraph::new(text).style(style), area),
        None => {
            // The hints are drawn span by span and their columns recorded, so a
            // click lands on exactly the hint that was drawn there.
            let separator = if app.theme.ascii { " | " } else { " · " };
            let mut spans = vec![Span::raw(" ")];
            let mut column = area.x + 1;
            for (index, (label, action)) in status_hints().into_iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw(separator));
                    column += separator.chars().count() as u16;
                }
                let width = label.chars().count() as u16;
                if column + width >= area.x + area.width {
                    break;
                }
                buttons.push((column, column + width - 1, action));
                spans.push(Span::raw(label));
                column += width;
            }
            frame.render_widget(Paragraph::new(Line::from(spans)).style(style), area);
        }
    }
    buttons
}

fn block<'a>(title: &'a str, focused: bool, theme: &'a crate::theme::Theme) -> Block<'a> {
    let border = if focused { theme.accent } else { theme.dim };
    Block::default()
        .border_set(border_set(theme))
        .borders(Borders::ALL)
        .border_style(border)
        // A column either side, so text is framed by the border rather than
        // touching it. Must match `inner`, which is what the pointer uses.
        .padding(ratatui::widgets::Padding::horizontal(1))
        .title(format!(" {title} "))
}

/// The marker drawn against the selected row.
fn cursor(theme: &crate::theme::Theme) -> &'static str {
    if theme.ascii { "> " } else { "› " }
}

/// Columns the selection marker occupies, which a list item does not get.
///
/// `highlight_symbol` is drawn inside the block but outside the item, so
/// right-aligning against the full inner width pushes the last two columns off
/// the edge — which silently ate the dates and the unread counts.
const CURSOR_WIDTH: u16 = 2;

/// One row of the entry list: a star gutter, the title, then the date.
///
/// The title leads because it is the content. The date is pushed right and
/// dimmed: it is useful for orienting yourself and useless as the thing your
/// eye lands on first, which is what leading with it made it.
fn entry_row<'a>(app: &App, entry: &'a crate::feed::Entry, width: u16) -> ListItem<'a> {
    let read = app.is_read(entry);
    let date = short_date(entry, &app.theme);
    let date_width = date.chars().count();

    // Fixed-width gutter, so a star never shifts the title beside it.
    let star = if app.is_starred(entry) {
        Span::styled(if app.theme.ascii { "* " } else { "★ " }, app.theme.star)
    } else {
        Span::raw("  ")
    };

    // Two spaces of breathing room before the date column.
    let room = (width as usize).saturating_sub(2 + date_width + 2);
    let title = truncate(&entry.title, room, app.theme.ascii);
    let padding = room.saturating_sub(title.chars().count()) + 2;

    let title = if read {
        Span::styled(title, app.theme.dim)
    } else {
        Span::styled(title, Style::default().add_modifier(Modifier::BOLD))
    };

    ListItem::new(Line::from(vec![
        star,
        title,
        Span::raw(" ".repeat(padding)),
        Span::styled(date, app.theme.dim),
    ]))
}

/// A date short enough to sit in a gutter.
///
/// The year is dropped for the current one — it is the same for almost every
/// entry, so printing it costs five columns to say nothing.
fn short_date(entry: &crate::feed::Entry, theme: &crate::theme::Theme) -> String {
    use chrono::Datelike;
    match entry.published {
        Some(when) if when.year() == chrono::Utc::now().year() => {
            format!("{:>2} {}", when.day(), month(when.month()))
        }
        Some(when) => format!("{} {}", month(when.month()), when.year()),
        // A placeholder the width of a date, so the column stays a column.
        None => if theme.ascii { "     -" } else { "     —" }.to_string(),
    }
}

fn month(month: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    NAMES[(month as usize).clamp(1, 12) - 1]
}

/// Shortens text to fit, marking that it had to.
///
/// The marker is a character, not a string, so it always costs exactly one
/// column — and it is ASCII when the theme says the terminal cannot draw more.
fn truncate(text: &str, width: usize, ascii: bool) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    if width <= 1 {
        return text.chars().take(width).collect();
    }
    let mut out: String = text.chars().take(width - 1).collect();
    out.push(if ascii { '~' } else { '…' });
    out
}

/// How wide the feed list should be.
///
/// Measured from the tree rather than fixed: nesting pushes names right, and a
/// width that suited a flat list truncates every feed two levels down. Still
/// bounded, so a deep tree cannot crowd out the article.
fn sidebar_width(app: &App, total: u16) -> u16 {
    let widest = app
        .feed_rows()
        .iter()
        .map(|row| {
            let (indent, label, right) = match row {
                crate::tree::Row::Folder {
                    path,
                    guides,
                    unread,
                    ..
                } => (
                    guides.len() * 2 + 2,
                    path.last().map_or(0, |name| name.chars().count()),
                    if *unread > 0 {
                        unread.to_string().len()
                    } else {
                        0
                    },
                ),
                crate::tree::Row::Feed { index, guides } => (
                    guides.len() * 2 + 2,
                    app.feeds
                        .get(*index)
                        .map_or(0, |feed| feed.title.chars().count()),
                    // Room for a count and a status marker.
                    4,
                ),
            };
            // Borders, padding, and a column between name and count.
            (indent + label + right + 6) as u16
        })
        .max()
        .unwrap_or(20);

    widest.clamp(18, 38).min(total.saturating_sub(30).max(18))
}

/// How to divide the right-hand column between the entry list and the article.
///
/// The list takes what it needs rather than a fixed share: a feed with three
/// entries should not hold back two thirds of the screen while the article it
/// is showing is squeezed into a dozen rows. Bounded at both ends so a huge
/// feed cannot crowd the article out either.
fn entry_and_article(app: &App, height: u16) -> [Constraint; 2] {
    let rows = app.listed_entry_count() as u16;
    // Two for the border, and never less than three rows of list.
    let wanted = rows.saturating_add(2).max(5);
    let cap = (height * 3 / 5).max(5);
    [Constraint::Length(wanted.min(cap)), Constraint::Min(6)]
}

/// The area inside a bordered block, with a column of padding either side.
///
/// Text touching a border reads as cramped, and the border stops being a frame
/// and starts being part of the sentence.
fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    }
}

/// The hints along the bottom, each one a thing you can click.
///
/// Paired with the action it performs rather than being decorative text, so the
/// pointer can use them and a hint cannot claim a key that does something else.
fn status_hints() -> Vec<(&'static str, crate::keys::Action)> {
    use crate::keys::Action;
    vec![
        ("? keys", Action::Help),
        ("q quit", Action::Quit),
        ("Tab pane", Action::CyclePane),
        ("/ search", Action::Search),
        ("u unread", Action::ToggleUnreadOnly),
        ("s star", Action::ToggleStar),
        ("o open", Action::Open),
        ("r refresh", Action::Refresh),
    ]
}

fn highlight(theme: &crate::theme::Theme) -> Style {
    theme.accent.add_modifier(Modifier::BOLD)
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
                        content: String::new(),
                        keys: vec!["id:one".into()],
                    },
                    Entry {
                        title: "Second Post".into(),
                        link: None,
                        published: None,
                        summary: String::new(),
                        content: String::new(),
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
                        content: String::new(),
                        keys: vec!["id:one".into()],
                    },
                    Entry {
                        title: "Two".into(),
                        link: None,
                        published: None,
                        summary: String::new(),
                        content: String::new(),
                        keys: vec!["id:two".into()],
                    },
                ],
            }],
            ReadState::default(),
        );

        assert!(row_with(&mut app, "Rust Blog").contains('2'), "two unread");

        app.mark_current_read();
        assert!(row_with(&mut app, "Rust Blog").contains('1'), "one unread");
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
                    content: String::new(),
                    keys: vec!["id:one".into()],
                }],
            }],
            ReadState::default(),
        );
        app.mark_current_read();

        let row = row_with(&mut app, "Rust Blog");
        assert!(
            !row.chars().any(|c| c.is_ascii_digit()),
            "no count when all read: {row:?}"
        );
    }

    /// Renders and returns the single screen row containing `needle`.
    ///
    /// Right-aligned columns mean a name and its count are no longer adjacent,
    /// so asserting on the row is the honest question: is the count on the
    /// same line as the feed?
    fn row_with(app: &mut App, needle: &str) -> String {
        let mut terminal =
            Terminal::new(TestBackend::new(80, 24)).expect("test terminal should build");
        terminal
            .draw(|frame| draw(frame, app, &crate::keys::Keymap::default()))
            .expect("draw should succeed");
        let buffer = terminal.backend().buffer().clone();
        for y in 0..24 {
            let row: String = (0..80).map(|x| buffer[(x, y)].symbol()).collect();
            // Skip the borders, whose titles also mention feed names.
            if row.contains(needle) && !row.contains('┌') && !row.contains('└') {
                return row;
            }
        }
        panic!("no body row contains {needle:?}");
    }

    /// Renders, then reports the lines of the detail pane only.
    fn detail_region(app: &mut App) -> String {
        render_at(app, 80, 24);
        let mut terminal =
            Terminal::new(TestBackend::new(80, 24)).expect("test terminal should build");
        terminal
            .draw(|frame| draw(frame, app, &crate::keys::Keymap::default()))
            .expect("draw should succeed");
        let buffer = terminal.backend().buffer().clone();
        // Read the pane's position from the hit map rather than assuming it:
        // the layout sizes the entry list to its content, so the article does
        // not start at a fixed row.
        let pane = app.hits.detail_pane;
        let mut out = String::new();
        for y in pane.y..pane.y + pane.height {
            for x in pane.x..pane.x + pane.width {
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
                    content: String::new(),
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

        let row = row_with(&mut app, "Slow Feed");
        assert!(row.contains('…'), "shows a loading mark: {row:?}");

        app.feeds[0].status = crate::feed::Status::Idle;
        app.feeds[0].entries = vec![Entry {
            title: "Arrived".into(),
            link: None,
            published: None,
            summary: String::new(),
            content: String::new(),
            keys: vec!["id:a".into()],
        }];

        assert!(
            !row_with(&mut app, "Slow Feed").contains('…'),
            "mark cleared once loaded"
        );
        assert!(render(&mut app).contains("Arrived"));
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

        assert!(
            row_with(&mut app, "Broken").contains('!'),
            "marked in the feed list"
        );
        assert!(
            render(&mut app).contains("dns error: no such host"),
            "error is shown"
        );
        assert!(
            !render(&mut app).contains("Failed to load"),
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
        assert!(row_with(&mut a, "Feed").contains('…'));
        assert!(row_with(&mut b, "Feed").contains('!'));
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
                    content: String::new(),
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
    fn every_binding_is_reachable_on_an_eighty_by_twentyfour_terminal() {
        let keymap = crate::keys::Keymap::default();
        let mut unseen: Vec<String> = keymap
            .sections()
            .iter()
            .flat_map(|(_, rows)| rows.iter().map(|(_, d)| (*d).to_string()))
            .collect();

        let mut app = App::new(Vec::new(), ReadState::default());
        app.help_open = true;
        // Scroll the whole way, as the keyboard would.
        for step in 0..40 {
            app.help_scroll = step;
            let screen = render_at(&mut app, 80, 24);
            unseen.retain(|description| !screen.contains(description.as_str()));
            if unseen.is_empty() {
                break;
            }
        }
        assert!(
            unseen.is_empty(),
            "unreachable even by scrolling: {unseen:?}"
        );
    }

    #[test]
    fn a_short_overlay_says_any_key_dismisses_it() {
        let mut app = App::new(Vec::new(), ReadState::default());
        app.help_open = true;
        // Tall enough for every action at once.
        assert!(render_at(&mut app, 100, 60).contains("any key to dismiss"));
    }

    #[test]
    fn an_overflowing_overlay_says_how_to_scroll() {
        let mut app = App::new(Vec::new(), ReadState::default());
        app.help_open = true;
        assert!(render_at(&mut app, 80, 24).contains("j/k to scroll"));
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
        assert!(render_at(&mut app, 120, 60).contains("any key to dismiss"));
    }

    #[test]
    fn the_help_overlay_is_absent_until_asked_for() {
        let mut app = App::new(Vec::new(), ReadState::default());
        assert!(!render(&mut app).contains("any key to dismiss"));
    }

    /// The style bytes of the whole screen, for comparing themes.
    fn styles_at(app: &mut App, width: u16, height: u16) -> Vec<String> {
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
            .map(|cell| format!("{:?}/{:?}", cell.fg, cell.bg))
            .collect()
    }

    fn themed(theme: crate::theme::Theme) -> App {
        App::new(
            vec![Feed {
                title: "Feed".into(),
                url: "https://a.example".into(),
                status: crate::feed::Status::Idle,
                entries: vec![Entry {
                    title: "An Entry".into(),
                    link: Some("https://a.example/x".into()),
                    published: None,
                    summary: "Body.".into(),
                    content: String::new(),
                    keys: vec!["id:x".into()],
                }],
            }],
            ReadState::default(),
        )
        .with_theme(theme)
    }

    #[test]
    fn a_different_theme_actually_changes_the_rendering() {
        let dark = styles_at(&mut themed(crate::theme::Theme::dark()), 80, 24);
        let light = styles_at(&mut themed(crate::theme::Theme::light()), 80, 24);
        assert_ne!(dark, light, "the light theme rendered identically to dark");
    }

    #[test]
    fn the_mono_theme_uses_no_colour_anywhere() {
        let mut app = themed(crate::theme::Theme::mono());
        app.help_open = false;
        let styles = styles_at(&mut app, 80, 24);
        for style in &styles {
            assert_eq!(style, "Reset/Reset", "mono rendered a colour: {style}");
        }
    }

    #[test]
    fn ascii_mode_draws_nothing_a_plain_terminal_cannot() {
        let mut theme = crate::theme::Theme::dark();
        theme.ascii = true;
        let mut app = themed(theme);
        app.feeds[0].status = crate::feed::Status::Fetching;
        app.read.toggle_star(&app.feeds[0].entries[0].clone());
        // Long enough to be truncated, and dated, so the ellipsis and the
        // title separator are both exercised — without these the test passed
        // while the real interface leaked `…` and `·`.
        app.feeds[0].title = "A Feed With A Very Long Name Indeed".into();
        app.feeds[0].entries[0].title =
            "An entry whose title is far too long to fit in the pane it is drawn in".into();
        app.feeds[0].entries[0].published = Some(
            chrono::DateTime::parse_from_rfc3339("2026-09-07T00:00:00Z")
                .expect("fixture parses")
                .with_timezone(&chrono::Utc),
        );

        let screen = render_at(&mut app, 80, 24);
        let offenders: Vec<char> = screen.chars().filter(|c| !c.is_ascii()).collect();
        assert!(offenders.is_empty(), "ascii mode still drew: {offenders:?}");
    }

    #[test]
    fn the_default_theme_still_uses_box_drawing() {
        // Guards against the ASCII fallback quietly becoming the only mode.
        let mut app = themed(crate::theme::Theme::dark());
        let screen = render_at(&mut app, 80, 24);
        assert!(
            screen
                .chars()
                .any(|c| ('\u{2500}'..='\u{257F}').contains(&c)),
            "box drawing disappeared from the default theme"
        );
    }

    #[test]
    fn the_status_bar_reverses_instead_of_pinning_a_text_colour() {
        let mut app = themed(crate::theme::Theme::terminal());
        let mut terminal =
            Terminal::new(TestBackend::new(80, 24)).expect("test terminal should build");
        terminal
            .draw(|frame| draw(frame, &mut app, &crate::keys::Keymap::default()))
            .expect("draw should succeed");
        let buffer = terminal.backend().buffer().clone();

        // The status bar is the last row.
        let cell = &buffer[(1, 23)];
        assert!(
            cell.modifier.contains(Modifier::REVERSED),
            "the status bar is not reversed: {cell:?}"
        );
        assert_eq!(
            cell.bg,
            ratatui::style::Color::Reset,
            "the status bar pinned a background instead of reversing"
        );
    }

    /// Renders, then clicks at a screen position, as a terminal would.
    ///
    /// Goes through `ui::draw` so the hit map is the one the renderer built,
    /// which is the whole point: the click lands on what was actually drawn.
    fn click_at(app: &mut App, column: u16, row: u16) -> Option<crate::keys::Action> {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        render_at(app, 80, 24);
        let event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        let hit = crate::mouse::resolve(&app.hits, event, false);
        crate::mouse::apply(app, hit)
    }

    fn wheel_at(app: &mut App, column: u16, row: u16, down: bool) {
        use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
        render_at(app, 80, 24);
        let event = MouseEvent {
            kind: if down {
                MouseEventKind::ScrollDown
            } else {
                MouseEventKind::ScrollUp
            },
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        let hit = crate::mouse::resolve(&app.hits, event, false);
        crate::mouse::apply(app, hit);
    }

    /// Two feeds under one group, plus an ungrouped third.
    fn clickable() -> App {
        let entry = |title: &str| Entry {
            title: title.into(),
            link: Some(format!("https://example.com/{title}")),
            published: None,
            summary: "Body text.".into(),
            content: String::new(),
            keys: vec![format!("id:{title}")],
        };
        let feed = |name: &str, titles: &[&str]| Feed {
            title: name.into(),
            url: format!("https://{name}.example"),
            status: crate::feed::Status::Idle,
            entries: titles.iter().map(|t| entry(t)).collect(),
        };
        let sources = vec![
            crate::config::FeedSource {
                url: "https://alpha.example".into(),
                title: None,
                tags: vec!["News".into()],
            },
            crate::config::FeedSource {
                url: "https://beta.example".into(),
                title: None,
                tags: vec!["News".into()],
            },
            crate::config::FeedSource {
                url: "https://gamma.example".into(),
                title: None,
                tags: Vec::new(),
            },
        ];
        App::new(
            vec![
                feed("alpha", &["a1", "a2", "a3"]),
                feed("beta", &["b1"]),
                feed("gamma", &["g1"]),
            ],
            ReadState::default(),
        )
        .with_tags(&sources)
    }

    /// Where the renderer put a given feed row, from the hit map it recorded.
    fn row_of_feed(app: &mut App, index: usize) -> u16 {
        render_at(app, 80, 24);
        app.hits
            .feed_rows
            .iter()
            .find(|(_, row)| matches!(row, crate::tree::Row::Feed { index: i, .. } if *i == index))
            .map(|(y, _)| *y)
            .unwrap_or_else(|| panic!("feed {index} was not drawn"))
    }

    /// Where the renderer put a given folder.
    fn row_of_group(app: &mut App, name: &str) -> u16 {
        render_at(app, 80, 24);
        app.hits
            .feed_rows
            .iter()
            .find(|(_, row)| {
                matches!(row, crate::tree::Row::Folder { path, .. }
                    if path.last().map(String::as_str) == Some(name))
            })
            .map(|(y, _)| *y)
            .unwrap_or_else(|| panic!("folder {name} was not drawn"))
    }

    #[test]
    fn clicking_a_feed_selects_the_one_that_was_drawn_there() {
        let mut app = clickable();
        let row = row_of_feed(&mut app, 1);
        click_at(&mut app, 4, row);
        assert_eq!(app.selected_feed, 1);
        assert_eq!(app.focus, Pane::Feeds);

        let row = row_of_feed(&mut app, 2);
        click_at(&mut app, 4, row);
        assert_eq!(app.selected_feed, 2, "the ungrouped feed");
    }

    #[test]
    fn clicking_a_group_heading_folds_and_unfolds_it() {
        let mut app = clickable();
        assert_eq!(app.selectable_feeds().len(), 3);

        let row = row_of_group(&mut app, "News");
        click_at(&mut app, 4, row);
        assert_eq!(app.selectable_feeds(), vec![2], "its feeds are hidden");

        let row = row_of_group(&mut app, "News");
        click_at(&mut app, 4, row);
        assert_eq!(app.selectable_feeds().len(), 3, "and come back");
    }

    #[test]
    fn folding_a_group_shifts_the_rows_below_it_and_clicks_follow() {
        let mut app = clickable();
        let heading = row_of_group(&mut app, "News");
        click_at(&mut app, 4, heading);

        // Folding removes rows, so whatever is at the heading's row + 1 now is
        // not what was there before. A click must follow what is drawn.
        render_at(&mut app, 80, 24);
        let after: Vec<_> = app.hits.feed_rows.clone();
        assert!(
            !after
                .iter()
                .any(|(_, row)| matches!(row, crate::tree::Row::Feed { index: 0, .. })),
            "the folded feed is still drawn"
        );
    }

    #[test]
    fn clicking_an_entry_selects_it_and_marks_it_read() {
        let mut app = clickable();
        assert_eq!(app.unread(0), 3);

        render_at(&mut app, 80, 24);
        let (row, _) = app.hits.entry_rows[1];
        click_at(&mut app, 40, row);
        assert_eq!(app.focus, Pane::Entries);
        assert_eq!(app.selected_entry, 1);
        assert_eq!(app.unread(0), 2, "clicking an entry reads it");
    }

    #[test]
    fn clicking_the_link_asks_to_open_it() {
        let mut app = clickable();
        render_at(&mut app, 80, 24);
        let (row, from, _) = app.hits.link_rows[0];
        assert_eq!(
            click_at(&mut app, from, row),
            Some(crate::keys::Action::Open)
        );
    }

    #[test]
    fn clicking_beside_the_link_only_moves_focus() {
        let mut app = clickable();
        render_at(&mut app, 80, 24);
        let (row, _, to) = app.hits.link_rows[0];
        let beside = to + 5;
        assert_eq!(click_at(&mut app, beside, row), None);
        assert_eq!(app.focus, Pane::Detail);
    }

    #[test]
    fn the_wheel_over_the_entries_pane_moves_through_entries() {
        let mut app = clickable();
        render_at(&mut app, 80, 24);
        let (row, _) = app.hits.entry_rows[0];
        wheel_at(&mut app, 40, row, true);
        assert_eq!(app.selected_entry, 1);
        wheel_at(&mut app, 40, row, false);
        assert_eq!(app.selected_entry, 0);
    }

    #[test]
    fn the_wheel_does_not_steal_keyboard_focus() {
        let mut app = clickable();
        assert_eq!(app.focus, Pane::Feeds);
        render_at(&mut app, 80, 24);
        let (row, _) = app.hits.entry_rows[0];
        wheel_at(&mut app, 40, row, true);
        assert_eq!(app.focus, Pane::Feeds, "scrolling only looked at the pane");
    }

    #[test]
    fn the_wheel_over_the_detail_pane_scrolls_the_article() {
        let mut app = clickable();
        app.feeds[0].entries[0].summary = (1..=400)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        wheel_at(&mut app, 40, 18, true);
        assert!(
            app.detail_scroll > 1,
            "a wheel notch moves more than a line"
        );
    }

    #[test]
    fn a_status_bar_hint_runs_its_action() {
        let mut app = clickable();
        // "? keys" is the first hint, at the start of the last row.
        assert_eq!(
            click_at(&mut app, 2, 23),
            Some(crate::keys::Action::Help),
            "the first hint is ? keys"
        );
    }

    #[test]
    fn clicking_dismisses_the_help_overlay() {
        let mut app = clickable();
        app.help_open = true;
        click_at(&mut app, 40, 10);
        assert!(!app.help_open);
        assert_eq!(app.help_scroll, 0);
    }

    #[test]
    fn renders_without_panicking_when_there_are_no_feeds() {
        let mut app = App::new(Vec::new(), ReadState::default());
        let screen = render(&mut app);
        assert!(screen.contains("No entry selected."));
    }
}
