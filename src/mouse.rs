//! Pointer handling: what is under the cursor, and what clicking it means.
//!
//! Hit-testing is done against a record of what was actually drawn, not against
//! geometry recomputed here. A list that is scrolled, filtered, grouped or
//! folded draws different things on the same rows, and any second calculation
//! of "row 7 is the third feed" would drift from the first. The renderer notes
//! each row as it draws it; this module only looks the answer up.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::Pane;
use crate::keys::Action;
use crate::tree::Row as FeedRow;

/// How far the wheel moves a text pane per notch.
///
/// More than one line, because a wheel notch is a coarser gesture than a key
/// press and scrolling prose one line at a time feels broken.
const WHEEL_LINES: isize = 3;

/// What was drawn where, recorded during rendering.
#[derive(Debug, Default, Clone)]
pub struct Hits {
    pub feeds_pane: Rect,
    pub entries_pane: Rect,
    pub detail_pane: Rect,
    pub status_bar: Rect,
    /// Screen row to the feed-pane row drawn on it.
    pub feed_rows: Vec<(u16, FeedRow)>,
    /// Screen row to the (feed, entry) drawn on it.
    pub entry_rows: Vec<(u16, (usize, usize))>,
    /// The rows the article's link occupies, as (row, first column, last
    /// column). A long URL wraps, so clicking its tail must work too.
    pub link_rows: Vec<(u16, u16, u16)>,
    /// Where each of the article's own links was drawn, and which one it is:
    /// row, first column, last column, link number counting from zero.
    pub article_links: Vec<(u16, u16, u16, usize)>,
    /// Status-bar hints, as (first column, last column, what they do).
    pub buttons: Vec<(u16, u16, Action)>,
    /// The overlay covers everything, so it takes every click.
    pub help_open: bool,
}

/// What a pointer event means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    SelectFeed(usize),
    ToggleFolder(Vec<String>),
    SelectEntry {
        feed: usize,
        entry: usize,
    },
    OpenEntry {
        feed: usize,
        entry: usize,
    },
    OpenLink,
    /// One of the article's numbered links, counting from zero.
    OpenArticleLink(usize),
    Scroll {
        pane: Pane,
        delta: isize,
    },
    Focus(Pane),
    Run(Action),
    CloseHelp,
    Nothing,
}

impl Hits {
    fn pane_at(&self, column: u16, row: u16) -> Option<Pane> {
        let inside = |area: Rect| {
            area.width > 0
                && column >= area.x
                && column < area.x + area.width
                && row >= area.y
                && row < area.y + area.height
        };
        if inside(self.feeds_pane) {
            Some(Pane::Feeds)
        } else if inside(self.entries_pane) {
            Some(Pane::Entries)
        } else if inside(self.detail_pane) {
            Some(Pane::Detail)
        } else {
            None
        }
    }

    fn button_at(&self, column: u16, row: u16) -> Option<Action> {
        if row != self.status_bar.y {
            return None;
        }
        self.buttons
            .iter()
            .find(|(from, to, _)| column >= *from && column <= *to)
            .map(|(.., action)| *action)
    }
}

/// Resolves a pointer event against what is on screen.
///
/// `double` says this click is the second of a double-click; the caller owns
/// that timing because it is the only part that needs a clock.
pub fn resolve(hits: &Hits, event: MouseEvent, double: bool) -> Hit {
    let (column, row) = (event.column, event.row);

    // The overlay is drawn over everything, so it answers first.
    if hits.help_open {
        return match event.kind {
            MouseEventKind::Down(_) => Hit::CloseHelp,
            MouseEventKind::ScrollDown => Hit::Scroll {
                pane: Pane::Detail,
                delta: WHEEL_LINES,
            },
            MouseEventKind::ScrollUp => Hit::Scroll {
                pane: Pane::Detail,
                delta: -WHEEL_LINES,
            },
            _ => Hit::Nothing,
        };
    }

    match event.kind {
        MouseEventKind::ScrollDown => scroll(hits, column, row, WHEEL_LINES),
        MouseEventKind::ScrollUp => scroll(hits, column, row, -WHEEL_LINES),
        MouseEventKind::Down(MouseButton::Left) => click(hits, column, row, double),
        _ => Hit::Nothing,
    }
}

fn scroll(hits: &Hits, column: u16, row: u16, delta: isize) -> Hit {
    match hits.pane_at(column, row) {
        // A wheel over a list moves by one item per notch: three rows of
        // prose is a comfortable nudge, three entries is a jump.
        Some(pane @ (Pane::Feeds | Pane::Entries)) => Hit::Scroll {
            pane,
            delta: delta.signum(),
        },
        Some(Pane::Detail) => Hit::Scroll {
            pane: Pane::Detail,
            delta,
        },
        None => Hit::Nothing,
    }
}

fn click(hits: &Hits, column: u16, row: u16, double: bool) -> Hit {
    if let Some(action) = hits.button_at(column, row) {
        return Hit::Run(action);
    }

    match hits.pane_at(column, row) {
        Some(Pane::Feeds) => match hits.feed_rows.iter().find(|(y, _)| *y == row) {
            Some((_, FeedRow::Feed { index, .. })) => Hit::SelectFeed(*index),
            Some((_, FeedRow::Folder { path, .. })) => Hit::ToggleFolder(path.clone()),
            // Empty space below the list still moves focus there.
            None => Hit::Focus(Pane::Feeds),
        },
        Some(Pane::Entries) => match hits.entry_rows.iter().find(|(y, _)| *y == row) {
            Some((_, (feed, entry))) if double => Hit::OpenEntry {
                feed: *feed,
                entry: *entry,
            },
            Some((_, (feed, entry))) => Hit::SelectEntry {
                feed: *feed,
                entry: *entry,
            },
            None => Hit::Focus(Pane::Entries),
        },
        Some(Pane::Detail) => {
            // The article's own links first: they are drawn inside the pane
            // the entry's URL heads, so the narrower target wins.
            let article = hits
                .article_links
                .iter()
                .find(|(y, from, to, _)| *y == row && column >= *from && column <= *to);
            if let Some((.., index)) = article {
                return Hit::OpenArticleLink(*index);
            }
            let on_link = hits
                .link_rows
                .iter()
                .any(|(y, from, to)| *y == row && column >= *from && column <= *to);
            if on_link {
                Hit::OpenLink
            } else {
                Hit::Focus(Pane::Detail)
            }
        }
        None => Hit::Nothing,
    }
}

/// Carries out a pointer hit, returning an action the keyboard also has.
///
/// Anything the pointer can do that the keyboard already does is handed back
/// rather than reimplemented here, so the two cannot drift apart about what
/// `refresh` or `open` means.
pub fn apply(app: &mut crate::app::App, hit: Hit) -> Option<Action> {
    match hit {
        Hit::SelectFeed(index) => app.select_feed(index),
        Hit::ToggleFolder(path) => app.toggle_folder(&path),
        Hit::SelectEntry { feed, entry } => app.select_entry(feed, entry),
        Hit::OpenEntry { feed, entry } => {
            app.select_entry(feed, entry);
            return Some(Action::Open);
        }
        Hit::OpenLink => return Some(Action::Open),
        Hit::OpenArticleLink(index) => {
            // Open already knows how to open a chosen link; choosing one is
            // all a click has to do.
            app.following = Some((index + 1).to_string());
            return Some(Action::Open);
        }
        Hit::Focus(pane) => {
            app.focus = pane;
            if pane == Pane::Entries {
                app.mark_current_read();
            }
        }
        Hit::Scroll { pane, delta } => {
            // Scrolling looks at a pane; it does not move the keyboard there.
            let previous = app.focus;
            app.focus = pane;
            match pane {
                Pane::Detail => app.scroll_detail(delta as i16),
                _ => {
                    for _ in 0..delta.unsigned_abs() {
                        if delta > 0 {
                            app.select_next();
                        } else {
                            app.select_previous();
                        }
                    }
                }
            }
            app.focus = previous;
        }
        Hit::CloseHelp => {
            app.help_open = false;
            app.help_scroll = 0;
        }
        Hit::Run(action) => return Some(action),
        Hit::Nothing => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn down(column: u16, row: u16) -> MouseEvent {
        at(MouseEventKind::Down(MouseButton::Left), column, row)
    }

    /// Feeds 0..20, entries 21..79, detail below, status on the last row.
    fn hits() -> Hits {
        Hits {
            feeds_pane: Rect::new(0, 0, 20, 20),
            entries_pane: Rect::new(20, 0, 60, 10),
            detail_pane: Rect::new(20, 10, 60, 10),
            status_bar: Rect::new(0, 23, 80, 1),
            feed_rows: vec![
                (
                    1,
                    FeedRow::Folder {
                        path: vec!["News".into()],
                        collapsed: false,
                        unread: 3,
                        guides: Vec::new(),
                    },
                ),
                (
                    2,
                    FeedRow::Feed {
                        index: 0,
                        guides: vec![true],
                    },
                ),
                (
                    3,
                    FeedRow::Feed {
                        index: 1,
                        guides: vec![true],
                    },
                ),
            ],
            entry_rows: vec![(1, (0, 5)), (2, (0, 6))],
            link_rows: vec![(12, 22, 40), (13, 22, 30)],
            // An inline link on row 15, columns 24..=31.
            article_links: vec![(15, 24, 31, 0)],
            buttons: vec![(0, 6, Action::Help), (8, 14, Action::Quit)],
            help_open: false,
        }
    }

    #[test]
    fn clicking_a_feed_selects_that_feed() {
        assert_eq!(resolve(&hits(), down(5, 3), false), Hit::SelectFeed(1));
    }

    #[test]
    fn clicking_a_folder_folds_it() {
        assert_eq!(
            resolve(&hits(), down(5, 1), false),
            Hit::ToggleFolder(vec!["News".into()])
        );
    }

    #[test]
    fn rows_come_from_what_was_drawn_not_from_arithmetic() {
        // The same screen row means different things once the list scrolls,
        // which is why the renderer records it rather than this recomputing it.
        let mut hits = hits();
        hits.feed_rows = vec![
            (
                1,
                FeedRow::Feed {
                    index: 7,
                    guides: vec![],
                },
            ),
            (
                2,
                FeedRow::Feed {
                    index: 8,
                    guides: vec![],
                },
            ),
        ];
        assert_eq!(resolve(&hits, down(5, 1), false), Hit::SelectFeed(7));
    }

    #[test]
    fn clicking_an_entry_selects_it_and_double_clicking_opens_it() {
        let hits = hits();
        assert_eq!(
            resolve(&hits, down(30, 2), false),
            Hit::SelectEntry { feed: 0, entry: 6 }
        );
        assert_eq!(
            resolve(&hits, down(30, 2), true),
            Hit::OpenEntry { feed: 0, entry: 6 }
        );
    }

    #[test]
    fn clicking_the_link_opens_it_but_beside_it_only_focuses() {
        let hits = hits();
        assert_eq!(resolve(&hits, down(30, 12), false), Hit::OpenLink);
        assert_eq!(
            resolve(&hits, down(50, 12), false),
            Hit::Focus(Pane::Detail),
            "past the end of the link"
        );
        assert_eq!(
            resolve(&hits, down(25, 13), false),
            Hit::OpenLink,
            "the wrapped tail of a long link is still the link"
        );
        assert_eq!(
            resolve(&hits, down(30, 14), false),
            Hit::Focus(Pane::Detail),
            "the row below the link"
        );
    }

    #[test]
    fn clicking_empty_space_in_a_pane_still_moves_focus_there() {
        assert_eq!(
            resolve(&hits(), down(5, 15), false),
            Hit::Focus(Pane::Feeds)
        );
        assert_eq!(
            resolve(&hits(), down(30, 8), false),
            Hit::Focus(Pane::Entries)
        );
    }

    #[test]
    fn the_wheel_scrolls_whichever_pane_it_is_over() {
        let hits = hits();
        assert_eq!(
            resolve(&hits, at(MouseEventKind::ScrollDown, 5, 5), false),
            Hit::Scroll {
                pane: Pane::Feeds,
                delta: 1
            }
        );
        assert_eq!(
            resolve(&hits, at(MouseEventKind::ScrollUp, 30, 5), false),
            Hit::Scroll {
                pane: Pane::Entries,
                delta: -1
            }
        );
    }

    #[test]
    fn the_wheel_moves_prose_further_than_a_list() {
        // A notch over a list is one entry; over an article it is three lines,
        // because scrolling prose one line at a time feels broken.
        let hits = hits();
        assert_eq!(
            resolve(&hits, at(MouseEventKind::ScrollDown, 30, 15), false),
            Hit::Scroll {
                pane: Pane::Detail,
                delta: WHEEL_LINES
            }
        );
    }

    #[test]
    fn status_bar_hints_are_buttons() {
        let hits = hits();
        assert_eq!(resolve(&hits, down(3, 23), false), Hit::Run(Action::Help));
        assert_eq!(resolve(&hits, down(10, 23), false), Hit::Run(Action::Quit));
        assert_eq!(
            resolve(&hits, down(40, 23), false),
            Hit::Nothing,
            "the gap between hints does nothing"
        );
    }

    #[test]
    fn the_help_overlay_takes_every_click() {
        let mut hits = hits();
        hits.help_open = true;
        // Even over a feed row, which would otherwise select a feed.
        assert_eq!(resolve(&hits, down(5, 3), false), Hit::CloseHelp);
    }

    #[test]
    fn the_help_overlay_scrolls_rather_than_closing_on_the_wheel() {
        let mut hits = hits();
        hits.help_open = true;
        assert_eq!(
            resolve(&hits, at(MouseEventKind::ScrollDown, 5, 3), false),
            Hit::Scroll {
                pane: Pane::Detail,
                delta: WHEEL_LINES
            }
        );
    }

    #[test]
    fn clicks_outside_every_pane_do_nothing() {
        let mut hits = hits();
        hits.status_bar = Rect::new(0, 23, 80, 1);
        assert_eq!(resolve(&hits, down(79, 22), false), Hit::Nothing);
    }

    #[test]
    fn other_buttons_and_movement_are_ignored() {
        let hits = hits();
        for kind in [
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::Drag(MouseButton::Left),
        ] {
            assert_eq!(resolve(&hits, at(kind, 5, 3), false), Hit::Nothing);
        }
    }

    #[test]
    fn clicking_a_link_in_the_article_opens_that_link() {
        assert_eq!(
            resolve(&hits(), down(26, 15), false),
            Hit::OpenArticleLink(0),
            "a click on the link text did not reach the link"
        );
    }

    #[test]
    fn clicking_beside_a_link_only_focuses_the_pane() {
        assert_eq!(
            resolve(&hits(), down(23, 15), false),
            Hit::Focus(Pane::Detail)
        );
        assert_eq!(
            resolve(&hits(), down(32, 15), false),
            Hit::Focus(Pane::Detail)
        );
    }

    #[test]
    fn a_clicked_link_is_the_one_open_then_opens() {
        let mut app = crate::app::App::new(Vec::new(), crate::state::ReadState::default());
        let action = apply(&mut app, Hit::OpenArticleLink(4));
        assert_eq!(action, Some(Action::Open));
        assert_eq!(
            app.following.as_deref(),
            Some("5"),
            "the click chose the wrong link, or none"
        );
    }
}
