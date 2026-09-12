use crate::feed::{Entry, Feed};
use crate::state::ReadState;

/// Which list the arrow keys currently drive.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Feeds,
    Entries,
    Detail,
}

/// All mutable UI state. Rendering reads this and nothing else.
#[derive(Debug, Default)]
pub struct App {
    pub feeds: Vec<Feed>,
    pub selected_feed: usize,
    pub selected_entry: usize,
    pub focus: Pane,
    pub status: Option<String>,
    pub should_quit: bool,
    pub read: ReadState,
    /// First visible line of the detail pane.
    pub detail_scroll: u16,
    /// Inner size of the detail pane, written back by the renderer each frame.
    /// Scrolling needs to know how much fits, and only the renderer knows.
    pub detail_viewport: (u16, u16),
}

impl App {
    pub fn new(feeds: Vec<Feed>, read: ReadState) -> Self {
        Self {
            feeds,
            read,
            ..Default::default()
        }
    }

    /// Marks the entry currently on screen as read.
    ///
    /// Showing an entry in the detail pane is what counts as reading it; there
    /// is no separate "open" step to hang this off.
    pub fn mark_current_read(&mut self) {
        let Some(feed) = self.feeds.get(self.selected_feed) else {
            return;
        };
        if let Some(entry) = feed.entries.get(self.selected_entry) {
            self.read.mark_read(entry);
        }
    }

    /// How many of a feed's entries have not been read.
    pub fn unread(&self, feed: usize) -> usize {
        self.feeds
            .get(feed)
            .map(|feed| {
                feed.entries
                    .iter()
                    .filter(|entry| !self.read.is_read(entry))
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn is_read(&self, entry: &Entry) -> bool {
        self.read.is_read(entry)
    }

    pub fn current_feed(&self) -> Option<&Feed> {
        self.feeds.get(self.selected_feed)
    }

    pub fn current_entry(&self) -> Option<&Entry> {
        self.current_feed()?.entries.get(self.selected_entry)
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Pane::Feeds => Pane::Entries,
            Pane::Entries => Pane::Detail,
            Pane::Detail => Pane::Feeds,
        };
        if self.focus == Pane::Entries {
            self.mark_current_read();
        }
    }

    /// The detail pane's text, wrapped to the width it is being drawn at.
    pub fn detail_lines(&self) -> Vec<String> {
        let width = self.detail_viewport.0 as usize;
        let Some(entry) = self.current_entry() else {
            return crate::text::wrap("No entry selected.", width);
        };

        let mut lines = crate::text::wrap(&entry.title, width);
        if let Some(link) = &entry.link {
            lines.extend(crate::text::wrap(link, width));
        }
        lines.push(String::new());
        lines.extend(crate::text::wrap(&entry.summary, width));
        lines
    }

    /// The furthest the detail pane can scroll and still show text.
    ///
    /// Stopping here is what keeps the pane from scrolling off into blank space.
    pub fn max_detail_scroll(&self) -> u16 {
        let lines = self.detail_lines().len() as u16;
        lines.saturating_sub(self.detail_viewport.1)
    }

    pub fn scroll_detail(&mut self, delta: i16) {
        let next = self.detail_scroll as i32 + delta as i32;
        self.detail_scroll = next.clamp(0, self.max_detail_scroll() as i32) as u16;
    }

    pub fn select_next(&mut self) {
        match self.focus {
            Pane::Feeds => {
                self.selected_feed = step(self.selected_feed, self.feeds.len(), 1);
                self.selected_entry = 0;
                self.detail_scroll = 0;
            }
            Pane::Entries => {
                self.selected_entry = step(self.selected_entry, self.entry_count(), 1);
                self.detail_scroll = 0;
                self.mark_current_read();
            }
            Pane::Detail => self.scroll_detail(1),
        }
    }

    pub fn select_previous(&mut self) {
        match self.focus {
            Pane::Feeds => {
                self.selected_feed = step(self.selected_feed, self.feeds.len(), -1);
                self.selected_entry = 0;
                self.detail_scroll = 0;
            }
            Pane::Entries => {
                self.selected_entry = step(self.selected_entry, self.entry_count(), -1);
                self.detail_scroll = 0;
                self.mark_current_read();
            }
            Pane::Detail => self.scroll_detail(-1),
        }
    }

    fn entry_count(&self) -> usize {
        self.current_feed().map_or(0, |feed| feed.entries.len())
    }
}

/// Moves `current` by `delta` within `len`, wrapping at both ends.
fn step(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = len as isize;
    (((current as isize + delta) % len + len) % len) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str) -> Entry {
        Entry {
            title: title.into(),
            link: None,
            published: None,
            summary: String::new(),
            keys: vec![format!("id:{title}")],
        }
    }

    fn app() -> App {
        App::new(
            vec![
                Feed {
                    title: "A".into(),
                    url: "https://a.example".into(),
                    loading: false,
                    entries: vec![entry("a1"), entry("a2")],
                },
                Feed {
                    title: "B".into(),
                    url: "https://b.example".into(),
                    loading: false,
                    entries: vec![entry("b1")],
                },
            ],
            ReadState::default(),
        )
    }

    #[test]
    fn feed_selection_wraps_around() {
        let mut app = app();
        app.select_previous();
        assert_eq!(app.selected_feed, 1);
        app.select_next();
        assert_eq!(app.selected_feed, 0);
    }

    #[test]
    fn changing_feed_resets_the_entry_cursor() {
        let mut app = app();
        app.focus = Pane::Entries;
        app.select_next();
        assert_eq!(app.selected_entry, 1);

        app.focus = Pane::Feeds;
        app.select_next();
        assert_eq!(app.selected_entry, 0);
    }

    #[test]
    fn entry_navigation_stays_within_the_selected_feed() {
        let mut app = app();
        app.selected_feed = 1; // one entry only
        app.focus = Pane::Entries;
        app.select_next();
        assert_eq!(app.selected_entry, 0);
        assert_eq!(app.current_entry().map(|e| e.title.as_str()), Some("b1"));
    }

    #[test]
    fn navigation_is_inert_without_feeds() {
        let mut app = App::new(Vec::new(), ReadState::default());
        app.select_next();
        app.select_previous();
        assert_eq!(app.selected_feed, 0);
        assert!(app.current_entry().is_none());
    }

    #[test]
    fn moving_through_entries_marks_them_read() {
        let mut app = app();
        assert_eq!(app.unread(0), 2);

        app.focus = Pane::Entries;
        app.mark_current_read();
        assert_eq!(app.unread(0), 1);

        app.select_next();
        assert_eq!(app.unread(0), 0);
    }

    #[test]
    fn focusing_the_entry_pane_marks_what_is_already_shown() {
        let mut app = app();
        assert_eq!(app.unread(0), 2);
        app.toggle_focus(); // Feeds -> Entries
        assert_eq!(app.unread(0), 1);
    }

    #[test]
    fn moving_between_feeds_does_not_mark_anything_read() {
        let mut app = app();
        app.select_next(); // focus is Feeds
        app.select_previous();
        assert_eq!(app.unread(0), 2);
        assert_eq!(app.unread(1), 1);
    }

    #[test]
    fn unread_counts_are_per_feed_and_safe_out_of_range() {
        let app = app();
        assert_eq!(app.unread(0), 2);
        assert_eq!(app.unread(1), 1);
        assert_eq!(app.unread(99), 0);
    }

    #[test]
    fn focus_cycles_through_all_three_panes() {
        let mut app = app();
        assert_eq!(app.focus, Pane::Feeds);
        app.toggle_focus();
        assert_eq!(app.focus, Pane::Entries);
        app.toggle_focus();
        assert_eq!(app.focus, Pane::Detail);
        app.toggle_focus();
        assert_eq!(app.focus, Pane::Feeds);
    }

    /// An app whose detail pane is 10 columns by 3 rows, holding a long summary.
    fn scrollable() -> App {
        let mut app = App::new(
            vec![Feed {
                title: "A".into(),
                url: "https://a.example".into(),
                loading: false,
                entries: vec![Entry {
                    title: "Title".into(),
                    link: None,
                    published: None,
                    summary: "one two three four five six seven eight nine ten".into(),
                    keys: vec!["id:x".into()],
                }],
            }],
            ReadState::default(),
        );
        app.detail_viewport = (10, 3);
        app
    }

    #[test]
    fn the_detail_pane_scrolls_a_line_at_a_time() {
        let mut app = scrollable();
        app.focus = Pane::Detail;
        assert_eq!(app.detail_scroll, 0);
        app.select_next();
        assert_eq!(app.detail_scroll, 1);
        app.select_previous();
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn scrolling_stops_at_the_top_and_at_the_last_line() {
        let mut app = scrollable();
        let max = app.max_detail_scroll();
        assert!(max > 0, "the fixture should overflow its pane");

        app.scroll_detail(-5);
        assert_eq!(app.detail_scroll, 0, "cannot scroll above the first line");

        app.scroll_detail(500);
        assert_eq!(app.detail_scroll, max, "cannot scroll past the last line");
    }

    #[test]
    fn a_pane_taller_than_its_text_does_not_scroll() {
        let mut app = scrollable();
        app.detail_viewport = (10, 200);
        assert_eq!(app.max_detail_scroll(), 0);
        app.scroll_detail(3);
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn selecting_another_entry_resets_the_scroll() {
        let mut app = app();
        app.detail_viewport = (10, 1);
        app.focus = Pane::Detail;
        app.scroll_detail(1);

        app.focus = Pane::Entries;
        app.select_next();
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn selecting_another_feed_resets_the_scroll() {
        let mut app = app();
        app.detail_viewport = (10, 1);
        app.focus = Pane::Detail;
        app.scroll_detail(1);

        app.focus = Pane::Feeds;
        app.select_next();
        assert_eq!(app.detail_scroll, 0);
    }
}
