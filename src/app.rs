use crate::feed::{Entry, Feed};
use crate::state::ReadState;

/// Which list the arrow keys currently drive.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Feeds,
    Entries,
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
            Pane::Entries => Pane::Feeds,
        };
        if self.focus == Pane::Entries {
            self.mark_current_read();
        }
    }

    pub fn select_next(&mut self) {
        match self.focus {
            Pane::Feeds => {
                self.selected_feed = step(self.selected_feed, self.feeds.len(), 1);
                self.selected_entry = 0;
            }
            Pane::Entries => {
                self.selected_entry = step(self.selected_entry, self.entry_count(), 1);
                self.mark_current_read();
            }
        }
    }

    pub fn select_previous(&mut self) {
        match self.focus {
            Pane::Feeds => {
                self.selected_feed = step(self.selected_feed, self.feeds.len(), -1);
                self.selected_entry = 0;
            }
            Pane::Entries => {
                self.selected_entry = step(self.selected_entry, self.entry_count(), -1);
                self.mark_current_read();
            }
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
                    entries: vec![entry("a1"), entry("a2")],
                },
                Feed {
                    title: "B".into(),
                    url: "https://b.example".into(),
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
    fn focus_toggles_between_the_two_panes() {
        let mut app = app();
        assert_eq!(app.focus, Pane::Feeds);
        app.toggle_focus();
        assert_eq!(app.focus, Pane::Entries);
        app.toggle_focus();
        assert_eq!(app.focus, Pane::Feeds);
    }
}
