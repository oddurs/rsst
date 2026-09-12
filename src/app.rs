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
    /// Active search, if any.
    pub search: Option<Search>,
    /// A bulk action waiting for the user to say yes.
    pub pending: Option<Bulk>,
}

/// A marking action that affects more than one entry, so it is worth a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bulk {
    /// Every entry in the selected feed.
    Feed,
    /// Every entry in every feed.
    Everything,
}

/// A search over every entry in every feed.
#[derive(Debug, Default, Clone)]
pub struct Search {
    pub query: String,
    /// True while the query is still being typed.
    pub typing: bool,
    /// Matches, as (feed, entry) indices.
    pub results: Vec<(usize, usize)>,
    /// Which match is selected.
    pub selected: usize,
    /// Where the cursor was before the search, to restore on Esc.
    saved: (usize, usize),
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

    /// Flips the selected entry between read and unread.
    pub fn toggle_current_read(&mut self) {
        let Some(entry) = self.current_entry().cloned() else {
            return;
        };
        if self.read.is_read(&entry) {
            self.read.mark_unread(&entry);
        } else {
            self.read.mark_read(&entry);
        }
    }

    /// Queues a bulk mark, to be confirmed or abandoned.
    pub fn request_bulk(&mut self, bulk: Bulk) {
        self.pending = Some(bulk);
    }

    /// How many entries the queued action would mark, for the prompt.
    pub fn pending_count(&self) -> usize {
        match self.pending {
            Some(Bulk::Feed) => self.unread(self.selected_feed),
            Some(Bulk::Everything) => (0..self.feeds.len()).map(|f| self.unread(f)).sum(),
            None => 0,
        }
    }

    /// Carries out the queued action and reports how many it marked.
    pub fn confirm_bulk(&mut self) -> usize {
        let Some(bulk) = self.pending.take() else {
            return 0;
        };
        let feeds: Vec<usize> = match bulk {
            Bulk::Feed => vec![self.selected_feed],
            Bulk::Everything => (0..self.feeds.len()).collect(),
        };

        let mut marked = 0;
        for index in feeds {
            let Some(feed) = self.feeds.get(index) else {
                continue;
            };
            for entry in feed.entries.clone() {
                if !self.read.is_read(&entry) {
                    self.read.mark_read(&entry);
                    marked += 1;
                }
            }
        }
        marked
    }

    /// Abandons the queued action.
    pub fn cancel_bulk(&mut self) {
        self.pending = None;
    }

    /// Opens a search, remembering where the cursor was.
    pub fn start_search(&mut self) {
        self.search = Some(Search {
            typing: true,
            saved: (self.selected_feed, self.selected_entry),
            ..Default::default()
        });
        self.recompute_search();
    }

    /// Adds a character to the query and re-runs it.
    pub fn push_search(&mut self, ch: char) {
        if let Some(search) = &mut self.search {
            search.query.push(ch);
        }
        self.recompute_search();
    }

    /// Removes the last character and re-runs the query.
    pub fn pop_search(&mut self) {
        if let Some(search) = &mut self.search {
            search.query.pop();
        }
        self.recompute_search();
    }

    /// Leaves the text field but keeps the results, so n/N can walk them.
    pub fn confirm_search(&mut self) {
        if let Some(search) = &mut self.search {
            search.typing = false;
        }
    }

    /// Abandons the search and puts the cursor back where it was.
    pub fn cancel_search(&mut self) {
        if let Some(search) = self.search.take() {
            let (feed, entry) = search.saved;
            self.selected_feed = feed;
            self.selected_entry = entry;
            self.detail_scroll = 0;
        }
    }

    /// Moves to the next or previous match, wrapping.
    pub fn step_match(&mut self, delta: isize) {
        let Some(search) = &mut self.search else {
            return;
        };
        if search.results.is_empty() {
            return;
        }
        search.selected = step(search.selected, search.results.len(), delta);
        let (feed, entry) = search.results[search.selected];
        self.selected_feed = feed;
        self.selected_entry = entry;
        self.detail_scroll = 0;
        self.mark_current_read();
    }

    /// Re-runs the query over every feed and moves to the first match.
    ///
    /// Called on every keystroke, which is what makes results update as you
    /// type rather than on Enter.
    fn recompute_search(&mut self) {
        let Some(search) = &self.search else {
            return;
        };
        let needle = search.query.to_lowercase();
        let results = if needle.is_empty() {
            Vec::new()
        } else {
            self.feeds
                .iter()
                .enumerate()
                .flat_map(|(f, feed)| {
                    let needle = &needle;
                    feed.entries
                        .iter()
                        .enumerate()
                        .filter_map(move |(e, entry)| {
                            let matches = entry.title.to_lowercase().contains(needle)
                                || entry.summary.to_lowercase().contains(needle);
                            matches.then_some((f, e))
                        })
                })
                .collect()
        };

        if let Some(search) = &mut self.search {
            search.results = results;
            search.selected = 0;
        }
        // Follow the first match so the detail pane shows what was found.
        if let Some(&(feed, entry)) = self.search.as_ref().and_then(|s| s.results.first()) {
            self.selected_feed = feed;
            self.selected_entry = entry;
            self.detail_scroll = 0;
        }
    }

    /// Whether an entry should be listed, given the current filter.
    ///
    /// The selected entry always shows, even once it has been marked read.
    /// Otherwise reading an entry would make it vanish from under the cursor,
    /// which is disorienting and loses your place.
    pub fn is_visible(&self, feed_index: usize, entry_index: usize) -> bool {
        if !self.read.unread_only {
            return true;
        }
        if feed_index == self.selected_feed && entry_index == self.selected_entry {
            return true;
        }
        self.feeds
            .get(feed_index)
            .and_then(|feed| feed.entries.get(entry_index))
            .is_some_and(|entry| !self.read.is_read(entry))
    }

    /// The entries of a feed that are currently listed, by index.
    pub fn visible_indices(&self, feed_index: usize) -> Vec<usize> {
        let Some(feed) = self.feeds.get(feed_index) else {
            return Vec::new();
        };
        (0..feed.entries.len())
            .filter(|index| self.is_visible(feed_index, *index))
            .collect()
    }

    /// Turns the unread-only filter on or off.
    pub fn toggle_unread_only(&mut self) {
        self.read.unread_only = !self.read.unread_only;
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

    /// Marks every idle feed as loading and reports which ones to fetch.
    ///
    /// A feed already in flight is skipped, so leaning on `r` cannot stack up
    /// duplicate requests for the same URL — the `loading` flag is both the
    /// indicator in the feed list and the guard.
    pub fn begin_refresh(&mut self) -> Vec<usize> {
        let mut starting = Vec::new();
        for (index, feed) in self.feeds.iter_mut().enumerate() {
            if feed.status != crate::feed::Status::Fetching {
                feed.status = crate::feed::Status::Fetching;
                starting.push(index);
            }
        }
        starting
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
                self.selected_entry = self.step_visible(1);
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
                self.selected_entry = self.step_visible(-1);
                self.detail_scroll = 0;
                self.mark_current_read();
            }
            Pane::Detail => self.scroll_detail(-1),
        }
    }

    /// The next listed entry in `delta`'s direction, wrapping.
    ///
    /// Walks the underlying list rather than a filtered copy, so the cursor
    /// lands on real entries and skips whatever the filter hides.
    fn step_visible(&self, delta: isize) -> usize {
        let count = self.entry_count();
        if count == 0 {
            return 0;
        }
        let mut index = self.selected_entry;
        for _ in 0..count {
            index = step(index, count, delta);
            if self.is_visible(self.selected_feed, index) && index != self.selected_entry {
                return index;
            }
        }
        self.selected_entry
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
                    status: crate::feed::Status::Idle,
                    entries: vec![entry("a1"), entry("a2")],
                },
                Feed {
                    title: "B".into(),
                    url: "https://b.example".into(),
                    status: crate::feed::Status::Idle,
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
    fn refreshing_starts_every_idle_feed() {
        let mut app = app();
        assert_eq!(app.begin_refresh(), vec![0, 1]);
        assert!(
            app.feeds
                .iter()
                .all(|f| f.status == crate::feed::Status::Fetching)
        );
    }

    #[test]
    fn refreshing_again_while_in_flight_starts_nothing() {
        let mut app = app();
        app.begin_refresh();
        assert!(
            app.begin_refresh().is_empty(),
            "a second refresh queued duplicate fetches"
        );
    }

    #[test]
    fn a_feed_that_has_landed_can_be_refreshed_again() {
        let mut app = app();
        app.begin_refresh();
        app.feeds[1].status = crate::feed::Status::Idle; // this one came back
        assert_eq!(app.begin_refresh(), vec![1]);
    }

    #[test]
    fn entries_stay_readable_while_a_feed_is_refreshing() {
        let mut app = app();
        app.begin_refresh();
        assert_eq!(app.current_feed().map(|f| f.entries.len()), Some(2));
        assert!(app.current_entry().is_some());
    }

    #[test]
    fn the_filter_hides_read_entries_but_keeps_the_selected_one() {
        let mut app = app();
        app.focus = Pane::Entries;
        app.mark_current_read(); // entry 0 of feed 0

        app.toggle_unread_only();
        // Entry 0 is read, but it is selected, so it stays put rather than
        // vanishing from under the cursor.
        assert_eq!(app.visible_indices(0), vec![0, 1]);

        app.selected_entry = 1;
        assert_eq!(app.visible_indices(0), vec![1]);
    }

    #[test]
    fn unfiltered_everything_is_listed() {
        let mut app = app();
        app.focus = Pane::Entries;
        app.mark_current_read();
        assert_eq!(app.visible_indices(0), vec![0, 1]);
    }

    #[test]
    fn the_visible_count_agrees_with_the_unread_count() {
        let mut app = app();
        app.toggle_unread_only();
        app.selected_feed = 1;
        assert_eq!(app.visible_indices(1).len(), app.unread(1));

        app.selected_feed = 0;
        assert_eq!(app.visible_indices(0).len(), app.unread(0));
    }

    #[test]
    fn navigation_skips_entries_the_filter_hides() {
        let mut app = App::new(
            vec![Feed {
                title: "A".into(),
                url: "https://a.example".into(),
                status: crate::feed::Status::Idle,
                entries: vec![entry("a1"), entry("a2"), entry("a3")],
            }],
            ReadState::default(),
        );
        // Mark the middle entry read without selecting it.
        app.read.mark_read(&app.feeds[0].entries[1].clone());
        app.toggle_unread_only();
        app.focus = Pane::Entries;

        app.select_next();
        assert_eq!(app.selected_entry, 2, "hopped over the read entry");
    }

    #[test]
    fn a_fully_read_feed_under_the_filter_shows_only_the_selection() {
        let mut app = app();
        for entry in app.feeds[0].entries.clone() {
            app.read.mark_read(&entry);
        }
        app.toggle_unread_only();
        assert_eq!(app.visible_indices(0), vec![0]);
        assert_eq!(app.unread(0), 0);
    }

    #[test]
    fn toggling_the_filter_is_reversible() {
        let mut app = app();
        assert!(!app.read.unread_only);
        app.toggle_unread_only();
        assert!(app.read.unread_only);
        app.toggle_unread_only();
        assert!(!app.read.unread_only);
    }

    fn two_feeds() -> App {
        App::new(
            vec![
                Feed {
                    title: "Alpha".into(),
                    url: "https://a.example".into(),
                    status: crate::feed::Status::Idle,
                    entries: vec![entry("Rust release notes"), entry("Cooking with gas")],
                },
                Feed {
                    title: "Beta".into(),
                    url: "https://b.example".into(),
                    status: crate::feed::Status::Idle,
                    entries: vec![entry("Rusty pipes"), entry("Nothing relevant")],
                },
            ],
            ReadState::default(),
        )
    }

    #[test]
    fn search_matches_across_every_feed() {
        let mut app = two_feeds();
        app.start_search();
        for ch in "rust".chars() {
            app.push_search(ch);
        }
        let results = &app.search.as_ref().expect("searching").results;
        assert_eq!(results, &[(0, 0), (1, 0)], "matched in both feeds");
    }

    #[test]
    fn search_is_case_insensitive() {
        let mut app = two_feeds();
        app.start_search();
        for ch in "RUST".chars() {
            app.push_search(ch);
        }
        assert_eq!(app.search.as_ref().expect("searching").results.len(), 2);
    }

    #[test]
    fn results_narrow_as_the_query_grows() {
        let mut app = two_feeds();
        app.start_search();
        for ch in "rust".chars() {
            app.push_search(ch);
        }
        assert_eq!(app.search.as_ref().expect("searching").results.len(), 2);

        app.push_search('y'); // "rusty"
        assert_eq!(
            app.search.as_ref().expect("searching").results,
            vec![(1, 0)]
        );

        app.pop_search(); // back to "rust"
        assert_eq!(app.search.as_ref().expect("searching").results.len(), 2);
    }

    #[test]
    fn search_also_looks_in_the_summary() {
        let mut app = two_feeds();
        app.feeds[0].entries[1].summary = "a distinctive phrase".into();
        app.start_search();
        for ch in "distinctive".chars() {
            app.push_search(ch);
        }
        assert_eq!(
            app.search.as_ref().expect("searching").results,
            vec![(0, 1)]
        );
    }

    #[test]
    fn escape_restores_the_selection_the_search_started_from() {
        let mut app = two_feeds();
        app.selected_feed = 1;
        app.selected_entry = 1;

        app.start_search();
        for ch in "rust".chars() {
            app.push_search(ch);
        }
        assert_eq!((app.selected_feed, app.selected_entry), (0, 0));

        app.cancel_search();
        assert!(app.search.is_none());
        assert_eq!((app.selected_feed, app.selected_entry), (1, 1));
    }

    #[test]
    fn stepping_through_matches_wraps_and_crosses_feeds() {
        let mut app = two_feeds();
        app.start_search();
        for ch in "rust".chars() {
            app.push_search(ch);
        }
        app.confirm_search();

        app.step_match(1);
        assert_eq!((app.selected_feed, app.selected_entry), (1, 0));
        app.step_match(1);
        assert_eq!((app.selected_feed, app.selected_entry), (0, 0), "wrapped");
    }

    #[test]
    fn an_empty_query_matches_nothing_rather_than_everything() {
        let mut app = two_feeds();
        app.start_search();
        assert!(app.search.as_ref().expect("searching").results.is_empty());
    }

    #[test]
    fn stepping_with_no_matches_is_harmless() {
        let mut app = two_feeds();
        app.start_search();
        for ch in "zzzz".chars() {
            app.push_search(ch);
        }
        app.step_match(1);
        assert_eq!(app.search.as_ref().expect("searching").selected, 0);
    }

    #[test]
    fn confirming_leaves_the_text_field_but_keeps_the_results() {
        let mut app = two_feeds();
        app.start_search();
        for ch in "rust".chars() {
            app.push_search(ch);
        }
        app.confirm_search();
        let search = app.search.as_ref().expect("searching");
        assert!(!search.typing);
        assert_eq!(search.results.len(), 2);
    }

    #[test]
    fn an_entry_toggles_between_read_and_unread() {
        let mut app = app();
        assert_eq!(app.unread(0), 2);

        app.toggle_current_read();
        assert_eq!(app.unread(0), 1);

        app.toggle_current_read();
        assert_eq!(app.unread(0), 2, "toggled back");
    }

    #[test]
    fn marking_a_feed_read_needs_confirming_first() {
        let mut app = app();
        app.request_bulk(Bulk::Feed);
        assert_eq!(app.pending_count(), 2);
        // Nothing has happened yet.
        assert_eq!(app.unread(0), 2);

        assert_eq!(app.confirm_bulk(), 2);
        assert_eq!(app.unread(0), 0);
        assert!(app.pending.is_none());
    }

    #[test]
    fn abandoning_a_bulk_mark_changes_nothing() {
        let mut app = app();
        app.request_bulk(Bulk::Feed);
        app.cancel_bulk();
        assert_eq!(app.unread(0), 2);
        assert_eq!(app.confirm_bulk(), 0, "nothing left queued");
    }

    #[test]
    fn marking_one_feed_leaves_the_others_alone() {
        let mut app = app();
        app.request_bulk(Bulk::Feed);
        app.confirm_bulk();
        assert_eq!(app.unread(0), 0);
        assert_eq!(app.unread(1), 1, "the other feed is untouched");
    }

    #[test]
    fn marking_everything_covers_every_feed() {
        let mut app = app();
        app.request_bulk(Bulk::Everything);
        assert_eq!(app.pending_count(), 3);
        assert_eq!(app.confirm_bulk(), 3);
        assert_eq!(app.unread(0), 0);
        assert_eq!(app.unread(1), 0);
    }

    #[test]
    fn a_bulk_mark_counts_only_what_was_still_unread() {
        let mut app = app();
        app.toggle_current_read(); // one already read
        app.request_bulk(Bulk::Everything);
        assert_eq!(app.confirm_bulk(), 2);
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
                status: crate::feed::Status::Idle,
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
