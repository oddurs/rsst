use crate::feed::{Entry, Feed};
use crate::state::ReadState;

/// Choosing where a feed should live.
#[derive(Debug, Clone)]
pub struct Move {
    /// The feed being moved.
    pub feed: usize,
    /// Where it could go: the top level, each existing folder, then a new one.
    pub choices: Vec<MoveTarget>,
    pub selected: usize,
    /// The name being typed, when the new-folder choice is selected.
    pub typed: String,
}

/// Somewhere a feed can be moved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveTarget {
    /// Out of every folder.
    TopLevel,
    /// An existing folder, by its full path.
    Folder(Vec<String>),
    /// A folder that does not exist yet, named as you type.
    New,
}

impl MoveTarget {
    /// How this reads in the picker.
    pub fn label(&self, typed: &str) -> String {
        match self {
            Self::TopLevel => "Top level".into(),
            Self::Folder(path) => path.join(" / "),
            Self::New if typed.is_empty() => "New folder…".into(),
            Self::New => format!("New folder: {typed}"),
        }
    }
}

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
    /// Showing only starred entries, across every feed.
    pub starred_view: bool,
    /// Inner size of the entries pane, written back by the renderer.
    pub entries_viewport: (u16, u16),
    /// Whether the key reference is covering the screen.
    pub help_open: bool,
    /// Folder paths the reader has folded shut, joined by `/`.
    pub collapsed: std::collections::HashSet<String>,
    /// Each feed's folder path, parallel to `feeds`.
    paths: Vec<Vec<String>>,
    /// Colours in use.
    pub theme: crate::theme::Theme,
    /// Showing every feed's entries as one list.
    pub all_feeds_view: bool,
    /// What was drawn where, for hit-testing the pointer.
    pub hits: crate::mouse::Hits,
    /// First visible line of the help overlay.
    pub help_scroll: u16,
    /// The open "move to folder" picker, if any.
    pub moving: Option<Move>,
    /// The full article fetched for the selected entry, if one has been.
    pub article: Option<String>,
    /// The URL being typed into the "add a feed" prompt, if it is open.
    pub adding: Option<String>,
    /// How prose should be set.
    pub measure: crate::article::Measure,
    /// Whether the article has the whole screen.
    pub reading: bool,
    /// The pane that had focus before reading mode took it.
    restore_focus: Pane,
    /// Where the reader had got to in each article they have opened, by key.
    marks: std::collections::HashMap<String, u16>,
    /// The article `detail_scroll` currently describes.
    marked: Option<String>,
    /// Digits typed so far toward following a numbered link.
    pub following: Option<String>,
    /// First visible row of the feed pane, moved by the wheel.
    pub feeds_offset: usize,
    /// First visible row of the entry pane, moved by the wheel.
    pub entries_offset: usize,
    /// What the renderer last drew in each list, so scrolling can be clamped
    /// and the selection followed without a second copy of the tree logic.
    pub feeds_view: ListView,
    pub entries_view: ListView,
    /// The selection as the view last followed it, so the view follows a
    /// selection that moves and stays put when only the wheel moved.
    followed: (usize, usize),
    /// The feed selection as the feed list last followed it.
    followed_feed: usize,
    /// The article as last laid out, and what it was laid out from.
    layout: Layout,
}

/// The detail pane's rows, kept until something that shaped them changes.
///
/// Laying an article out costs about ten milliseconds for a long one, and the
/// pane used to do it twice per frame, ten frames a second, whether or not
/// anything had moved — a quarter of a core to display a page that was not
/// changing. The comparison that avoids it is a string compare of the source,
/// which is three orders of magnitude cheaper than parsing it.
#[derive(Debug, Default)]
struct Layout {
    /// What was laid out. Compared rather than hashed: an exact answer, and
    /// cheap next to what it saves.
    source: String,
    title: String,
    link: Option<String>,
    width: u16,
    measure: Option<crate::article::Measure>,
    rows: Vec<crate::article::Row>,
}

/// What a list pane looked like on the last frame.
///
/// Written back by the renderer, which is the only thing that knows how many
/// rows a folded tree has or how many fit. The app does the arithmetic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListView {
    /// Rows the list has in total.
    pub rows: usize,
    /// Rows that fit on screen.
    pub height: usize,
    /// Which row the selection is drawn on, if it is in the list at all.
    pub selected: Option<usize>,
}

/// A marking action that affects more than one entry, so it is worth a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bulk {
    /// Every entry in the selected feed.
    Feed,
    /// Every entry in every feed.
    Everything,
    /// The selected feed itself, and everything stored for it.
    Unsubscribe,
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
        let paths = vec![Vec::new(); feeds.len()];
        Self {
            feeds,
            read,
            paths,
            ..Default::default()
        }
    }

    /// Reconciles the feed list with a freshly read config.
    ///
    /// Feeds already on screen are kept as they are — their entries, their
    /// fetch status, everything — so reloading to add one feed does not blank
    /// the others. Read state is untouched: it lives in `read`, keyed by entry
    /// rather than by position, so it cannot be disturbed by this at all.
    pub fn reconcile(&mut self, sources: &[crate::config::FeedSource]) {
        let existing = std::mem::take(&mut self.feeds);
        self.feeds = sources
            .iter()
            .map(|source| {
                existing
                    .iter()
                    .find(|feed| feed.url == source.url)
                    .cloned()
                    .unwrap_or_else(|| Feed::pending(source))
            })
            .collect();

        self.paths = sources.iter().map(|source| source.tags.clone()).collect();

        // The cursor may have been pointing at a feed that is now gone.
        self.selected_feed = self.selected_feed.min(self.feeds.len().saturating_sub(1));
        self.selected_entry = 0;
        self.detail_scroll = 0;
    }

    /// Which feeds are new since the last config, by index.
    pub fn indices_without_entries(&self) -> Vec<usize> {
        self.feeds
            .iter()
            .enumerate()
            .filter(|(_, feed)| feed.entries.is_empty())
            .map(|(index, _)| index)
            .collect()
    }

    /// Sets the colours the renderer should use.
    pub fn with_theme(mut self, theme: crate::theme::Theme) -> Self {
        self.measure.ascii = theme.ascii;
        self.theme = theme;
        self
    }

    /// Sets how prose should be set.
    pub fn with_measure(mut self, measure: crate::article::Measure) -> Self {
        self.measure = crate::article::Measure {
            ascii: self.theme.ascii,
            ..measure
        };
        self
    }

    /// Records each feed's folder path from the config.
    ///
    /// The whole tag list, not just the first: it is a path into the tree, and
    /// reading only the head of it is what flattened every nested folder.
    pub fn with_tags(mut self, sources: &[crate::config::FeedSource]) -> Self {
        self.paths = sources.iter().map(|source| source.tags.clone()).collect();
        self.paths.resize(self.feeds.len(), Vec::new());
        self
    }

    /// The folder path a feed sits in, empty at the top level.
    pub fn path_of(&self, feed: usize) -> &[String] {
        self.paths.get(feed).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Every folder in the tree, for a picker to offer.
    pub fn folders(&self) -> Vec<Vec<String>> {
        crate::tree::folders(&self.paths)
    }

    /// Whether a folder is folded shut.
    pub fn is_collapsed(&self, path: &[String]) -> bool {
        self.collapsed.contains(&path.join("/"))
    }

    /// The feed list as it should be drawn: group headers and their feeds.
    ///
    /// The sidebar as drawn: folders and feeds, nested.
    pub fn feed_rows(&self) -> Vec<crate::tree::Row> {
        crate::tree::rows(&self.paths, &|index| self.unread(index), &|path| {
            self.is_collapsed(path)
        })
    }

    /// The feeds the cursor can currently land on.
    pub fn selectable_feeds(&self) -> Vec<usize> {
        self.feed_rows()
            .into_iter()
            .filter_map(|row| match row {
                crate::tree::Row::Feed { index, .. } => Some(index),
                crate::tree::Row::Folder { .. } => None,
            })
            .collect()
    }

    /// Folds a folder shut, or opens it.
    ///
    /// Takes the path rather than using the selection, because a click lands on
    /// a folder that may not be the selected feed's.
    pub fn toggle_folder(&mut self, path: &[String]) {
        let key = path.join("/");
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key);
            if !self.selectable_feeds().contains(&self.selected_feed)
                && let Some(&next) = self.selectable_feeds().first()
            {
                self.selected_feed = next;
                self.selected_entry = 0;
                self.detail_scroll = 0;
            }
        }
    }

    /// Selects a feed by index, as a click does.
    pub fn select_feed(&mut self, index: usize) {
        if index >= self.feeds.len() {
            return;
        }
        self.focus = Pane::Feeds;
        self.selected_feed = index;
        self.selected_entry = 0;
        self.detail_scroll = 0;
    }

    /// Selects an entry in a feed, as a click does.
    pub fn select_entry(&mut self, feed: usize, entry: usize) {
        if self
            .feeds
            .get(feed)
            .is_none_or(|f| entry >= f.entries.len())
        {
            return;
        }
        self.focus = Pane::Entries;
        self.selected_feed = feed;
        self.selected_entry = entry;
        self.detail_scroll = 0;
        self.mark_current_read();
    }

    /// Folds the selected feed's folder shut, or opens it.
    pub fn toggle_group(&mut self) {
        let path = self.path_of(self.selected_feed).to_vec();
        if path.is_empty() {
            return;
        }
        self.toggle_folder(&path);
    }

    /// The next selectable feed in `delta`'s direction, wrapping.
    fn step_feed(&self, delta: isize) -> usize {
        let selectable = self.selectable_feeds();
        if selectable.is_empty() {
            return self.selected_feed;
        }
        let at = selectable
            .iter()
            .position(|i| *i == self.selected_feed)
            .unwrap_or(0);
        selectable[step(at, selectable.len(), delta)]
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

    /// Jumps to the first item in the focused pane.
    pub fn select_first(&mut self) {
        match self.focus {
            Pane::Feeds => {
                self.selected_feed = 0;
                self.selected_entry = 0;
                self.detail_scroll = 0;
            }
            Pane::Entries => {
                self.selected_entry = self.first_visible().unwrap_or(0);
                self.detail_scroll = 0;
                self.mark_current_read();
            }
            Pane::Detail => self.detail_scroll = 0,
        }
    }

    /// Jumps to the last item in the focused pane.
    pub fn select_last(&mut self) {
        match self.focus {
            Pane::Feeds => {
                self.selected_feed = self.feeds.len().saturating_sub(1);
                self.selected_entry = 0;
                self.detail_scroll = 0;
            }
            Pane::Entries => {
                self.selected_entry = self.last_visible().unwrap_or(0);
                self.detail_scroll = 0;
                self.mark_current_read();
            }
            Pane::Detail => self.detail_scroll = self.max_detail_scroll(),
        }
    }

    /// Moves by half the focused pane's height, the way Ctrl-d and Ctrl-u do.
    ///
    /// Half a *pane*, not a fixed number: on a tall terminal it should cover
    /// more ground, which is the whole reason the binding exists.
    pub fn half_page(&mut self, direction: isize) {
        let height = match self.focus {
            Pane::Detail => self.detail_viewport.1,
            _ => self.entries_viewport.1,
        };
        let steps = (height / 2).max(1);
        for _ in 0..steps {
            match direction {
                d if d > 0 => self.select_next(),
                _ => self.select_previous(),
            }
        }
    }

    /// Moves to the next unread entry, continuing into other feeds.
    ///
    /// Crossing feed boundaries is the point: working a backlog should not
    /// require noticing that a feed is finished and moving over by hand.
    pub fn next_unread(&mut self, forward: bool) -> bool {
        let total: usize = self.feeds.iter().map(|f| f.entries.len()).sum();
        if total == 0 {
            return false;
        }

        let mut feed = self.selected_feed;
        let mut entry = self.selected_entry;
        for _ in 0..total {
            let Some(next) = self.neighbour(feed, entry, forward) else {
                return false;
            };
            (feed, entry) = next;
            let unread = self.feeds[feed]
                .entries
                .get(entry)
                .is_some_and(|e| !self.read.is_read(e));
            if unread {
                self.selected_feed = feed;
                self.selected_entry = entry;
                self.detail_scroll = 0;
                self.mark_current_read();
                return true;
            }
        }
        false
    }

    /// The next (feed, entry) position, wrapping across feeds and round the end.
    fn neighbour(&self, feed: usize, entry: usize, forward: bool) -> Option<(usize, usize)> {
        if self.feeds.is_empty() {
            return None;
        }
        let mut feed = feed;
        let mut entry = entry as isize + if forward { 1 } else { -1 };

        for _ in 0..=self.feeds.len() {
            let len = self.feeds.get(feed)?.entries.len() as isize;
            if entry >= 0 && entry < len {
                return Some((feed, entry as usize));
            }
            if forward {
                feed = (feed + 1) % self.feeds.len();
                entry = 0;
            } else {
                feed = if feed == 0 {
                    self.feeds.len() - 1
                } else {
                    feed - 1
                };
                entry = self.feeds.get(feed)?.entries.len() as isize - 1;
            }
        }
        None
    }

    fn first_visible(&self) -> Option<usize> {
        self.visible_indices(self.selected_feed).first().copied()
    }

    fn last_visible(&self) -> Option<usize> {
        self.visible_indices(self.selected_feed).last().copied()
    }

    /// Every entry across every feed, in date order.
    ///
    /// Undated entries sort last whichever direction is chosen: a feed that
    /// omits dates should not colonise the top of the list, and reversing the
    /// order is not a reason for it to colonise the bottom either.
    pub fn all_entries(&self) -> Vec<(usize, usize)> {
        let mut rows: Vec<(usize, usize)> = self
            .feeds
            .iter()
            .enumerate()
            .flat_map(|(f, feed)| (0..feed.entries.len()).map(move |e| (f, e)))
            .collect();

        let oldest_first = self.read.oldest_first;
        rows.sort_by(|a, b| {
            let published = |(f, e): &(usize, usize)| {
                self.feeds
                    .get(*f)
                    .and_then(|feed| feed.entries.get(*e))
                    .and_then(|entry| entry.published)
            };
            match (published(a), published(b)) {
                (Some(x), Some(y)) => {
                    if oldest_first {
                        x.cmp(&y)
                    } else {
                        y.cmp(&x)
                    }
                }
                // Undated last, in both directions.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
        rows
    }

    /// Shows or hides the combined all-feeds list.
    pub fn toggle_all_feeds_view(&mut self) {
        self.all_feeds_view = !self.all_feeds_view;
        if self.all_feeds_view
            && let Some(&(feed, entry)) = self.all_entries().first()
        {
            self.selected_feed = feed;
            self.selected_entry = entry;
            self.detail_scroll = 0;
        }
    }

    /// Flips between newest-first and oldest-first.
    pub fn toggle_sort(&mut self) {
        self.read.oldest_first = !self.read.oldest_first;
    }

    /// Opens the prompt for adding a feed.
    pub fn start_add(&mut self) {
        self.adding = Some(String::new());
    }

    pub fn cancel_add(&mut self) {
        self.adding = None;
    }

    pub fn type_add(&mut self, ch: char) {
        if let Some(url) = &mut self.adding {
            url.push(ch);
        }
    }

    pub fn backspace_add(&mut self) {
        if let Some(url) = &mut self.adding {
            url.pop();
        }
    }

    /// The URL as typed, once it is worth trying.
    ///
    /// A bare word is not a URL, and guessing a scheme for it would mean
    /// fetching something the reader never asked for.
    pub fn add_url(&self) -> Option<String> {
        let typed = self.adding.as_ref()?.trim();
        (typed.starts_with("http://") || typed.starts_with("https://")).then(|| typed.to_string())
    }

    /// Opens the picker for moving the selected feed.
    ///
    /// Offers the top level first, then every folder that exists, then a new
    /// one — so the common cases need no typing at all.
    pub fn start_move(&mut self) {
        if self.feeds.is_empty() {
            return;
        }
        let here = self.path_of(self.selected_feed).to_vec();
        let folders = self.folders();

        let mut choices = vec![MoveTarget::TopLevel];
        choices.extend(folders.into_iter().map(MoveTarget::Folder));
        choices.push(MoveTarget::New);

        // Start on where the feed already is, so the picker opens showing the
        // truth rather than an arbitrary first row.
        let selected = choices
            .iter()
            .position(|choice| match choice {
                MoveTarget::TopLevel => here.is_empty(),
                MoveTarget::Folder(path) => path == &here,
                MoveTarget::New => false,
            })
            .unwrap_or(0);

        self.moving = Some(Move {
            feed: self.selected_feed,
            choices,
            selected,
            typed: String::new(),
        });
    }

    pub fn cancel_move(&mut self) {
        self.moving = None;
    }

    /// Moves the picker's cursor, wrapping.
    pub fn step_move(&mut self, delta: isize) {
        if let Some(moving) = &mut self.moving {
            let count = moving.choices.len();
            moving.selected = step(moving.selected, count, delta);
        }
    }

    pub fn type_move(&mut self, ch: char) {
        if let Some(moving) = &mut self.moving
            && moving.choices.get(moving.selected) == Some(&MoveTarget::New)
        {
            moving.typed.push(ch);
        }
    }

    pub fn backspace_move(&mut self) {
        if let Some(moving) = &mut self.moving {
            moving.typed.pop();
        }
    }

    /// The chosen destination, or `None` if it is not yet usable.
    ///
    /// A new folder with no name is not a destination, so confirming does
    /// nothing rather than silently moving the feed to the top level.
    pub fn move_destination(&self) -> Option<(usize, Vec<String>)> {
        let moving = self.moving.as_ref()?;
        let target = moving.choices.get(moving.selected)?;
        let path = match target {
            MoveTarget::TopLevel => Vec::new(),
            MoveTarget::Folder(path) => path.clone(),
            MoveTarget::New => {
                let name = moving.typed.trim();
                if name.is_empty() {
                    return None;
                }
                // A slash nests, so "Rust / Core" can be typed in one go.
                name.split('/')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect()
            }
        };
        Some((moving.feed, path))
    }

    /// Stars the selected entry, or unstars it. Reports the new state.
    pub fn toggle_star(&mut self) -> bool {
        let Some(entry) = self.current_entry().cloned() else {
            return false;
        };
        self.read.toggle_star(&entry)
    }

    pub fn is_starred(&self, entry: &Entry) -> bool {
        self.read.is_starred(entry)
    }

    /// Every starred entry, across all feeds, as (feed, entry) indices.
    pub fn starred_results(&self) -> Vec<(usize, usize)> {
        self.feeds
            .iter()
            .enumerate()
            .flat_map(|(f, feed)| {
                feed.entries
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| self.read.is_starred(entry))
                    .map(move |(e, _)| (f, e))
            })
            .collect()
    }

    /// Shows or hides the starred-only view.
    pub fn toggle_starred_view(&mut self) {
        self.starred_view = !self.starred_view;
        // Land on the first starred entry so the detail pane is not stale.
        if self.starred_view
            && let Some(&(feed, entry)) = self.starred_results().first()
        {
            self.selected_feed = feed;
            self.selected_entry = entry;
            self.detail_scroll = 0;
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
            Some(Bulk::Unsubscribe) | None => 0,
        }
    }

    /// What the queued action is asking, in words.
    ///
    /// Here rather than in the renderer because deciding what a confirmation
    /// says is deciding what it does — and an unsubscribe that did not say
    /// what it was about to throw away would be a trap.
    pub fn pending_prompt(&self) -> Option<String> {
        let pending = self.pending?;
        Some(match pending {
            Bulk::Feed | Bulk::Everything => {
                let what = match pending {
                    Bulk::Feed => "this feed",
                    _ => "every feed",
                };
                format!(
                    " Mark {} unread entries in {what} as read?  y / n ",
                    self.pending_count()
                )
            }
            Bulk::Unsubscribe => {
                let Some(feed) = self.current_feed() else {
                    return Some(" No feed selected.  y / n ".into());
                };
                let entries = feed.entries.len();
                let starred = feed
                    .entries
                    .iter()
                    .filter(|entry| self.read.is_starred(entry))
                    .count();
                // Starred entries are the ones somebody chose to keep, so
                // losing them is the part of this worth saying out loud.
                let kept = match starred {
                    0 => String::new(),
                    1 => ", including 1 starred".into(),
                    many => format!(", including {many} starred"),
                };
                format!(
                    " Unsubscribe from {} and delete {entries} entries{kept}?  y / n ",
                    feed.title
                )
            }
        })
    }

    /// Carries out the queued action and reports how many it marked.
    pub fn confirm_bulk(&mut self) -> usize {
        let Some(bulk) = self.pending.take() else {
            return 0;
        };
        let feeds: Vec<usize> = match bulk {
            Bulk::Feed => vec![self.selected_feed],
            Bulk::Everything => (0..self.feeds.len()).collect(),
            // Not this method's business: unsubscribing writes the config and
            // reloads, which needs more than the app state.
            Bulk::Unsubscribe => return 0,
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

    /// Re-runs the query using the database's full-text index.
    ///
    /// `hits` are (feed url, position) pairs from FTS5; they are mapped back to
    /// what is on screen here, because the pane draws from memory.
    pub fn apply_search_hits(&mut self, hits: &[(String, i64)]) {
        let results: Vec<(usize, usize)> = hits
            .iter()
            .filter_map(|(url, position)| {
                let feed = self.feeds.iter().position(|f| &f.url == url)?;
                let entry = usize::try_from(*position).ok()?;
                (entry < self.feeds[feed].entries.len()).then_some((feed, entry))
            })
            .collect();

        if let Some(search) = &mut self.search {
            search.results = results;
            search.selected = 0;
        }
        if let Some(&(feed, entry)) = self.search.as_ref().and_then(|s| s.results.first()) {
            self.selected_feed = feed;
            self.selected_entry = entry;
            self.detail_scroll = 0;
        }
    }

    /// The query as typed, if a search is open.
    pub fn search_query(&self) -> Option<&str> {
        self.search.as_ref().map(|s| s.query.as_str())
    }

    /// Re-runs the query over every feed and moves to the first match.
    ///
    /// Used when there is no database to ask — the tests, and as the fallback
    /// if a full-text query fails.
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

    /// How many rows the entry pane will draw, whatever view is showing.
    ///
    /// The layout asks so the list can take the room it needs rather than a
    /// fixed share of the screen.
    pub fn listed_entry_count(&self) -> usize {
        if self.search.is_some() {
            return self.search.as_ref().map_or(0, |s| s.results.len());
        }
        if self.all_feeds_view {
            return self.all_entries().len();
        }
        if self.starred_view {
            return self.starred_results().len();
        }
        self.visible_indices(self.selected_feed).len()
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

    /// The article, laid out for the width it is being drawn at.
    ///
    /// The entry's markup is parsed into a document and laid out, rather than
    /// stripped to a run of text — a code block whose line breaks are gone is
    /// not a code block.
    fn lay_out_detail(&self) -> Vec<crate::article::Row> {
        use crate::article::{Inline, Kind, Row};
        let width = self.detail_viewport.0 as usize;
        let Some(entry) = self.current_entry() else {
            return crate::text::wrap("No entry selected.", width)
                .into_iter()
                .map(|text| Row {
                    kind: Kind::Body,
                    indent: 0,
                    spans: vec![Inline::Text(text)],
                })
                .collect();
        };

        // The header follows the same measure as the article; setting it to
        // the full pane while the prose is a column looks like two documents.
        let (prose, margin) = self.measure.fit(width);

        let mut rows: Vec<Row> = crate::text::wrap(&entry.title, prose)
            .into_iter()
            .map(|text| Row {
                kind: Kind::Heading,
                indent: margin,
                spans: vec![Inline::Text(text)],
            })
            .collect();

        if let Some(link) = entry.link.as_deref().filter(|l| !l.is_empty()) {
            rows.extend(crate::text::wrap(link, prose).into_iter().map(|text| Row {
                kind: Kind::Reference,
                indent: margin,
                spans: vec![Inline::Text(text)],
            }));
        }

        // The header is a block of its own; the article should not run
        // straight on from the URL.
        rows.push(Row {
            kind: Kind::Blank,
            indent: 0,
            spans: Vec::new(),
        });

        if let Some(source) = self.article_source() {
            rows.extend(crate::article::layout(
                &crate::article::parse(source),
                width,
                self.measure,
            ));
        }
        rows
    }

    /// The markup the detail pane is showing.
    ///
    /// A fetched article wins over the feed's own text, which for a
    /// summary-only feed is a teaser. An entry stored before the markup was
    /// kept still has its plain text.
    fn article_source(&self) -> Option<&str> {
        let entry = self.current_entry()?;
        Some(match (&self.article, entry.content.is_empty()) {
            (Some(article), _) => article.as_str(),
            (None, true) => entry.summary.as_str(),
            (None, false) => entry.content.as_str(),
        })
    }

    /// The article's links, in the order the reference list numbers them.
    pub fn article_links(&self) -> Vec<String> {
        self.article_source()
            .map(|source| crate::article::parse(source).links)
            .unwrap_or_default()
    }

    /// Takes a digit toward a link number, returning the link once no further
    /// digit could change which one is meant.
    ///
    /// With nine links, typing `3` can only mean the third, so it opens at
    /// once; with ninety, `3` might yet become `30`, so it waits.
    pub fn type_link_digit(&mut self, digit: char) -> Option<String> {
        let links = self.article_links();
        if links.is_empty() {
            self.status = Some(" This article has no links. ".into());
            return None;
        }
        let mut typed = self.following.take().unwrap_or_default();
        typed.push(digit);

        let number: usize = typed.parse().ok()?;
        if number == 0 || number > links.len() {
            self.status = Some(format!(" No link [{typed}]. "));
            return None;
        }
        if number * 10 > links.len() {
            self.status = None;
            return links.get(number - 1).cloned();
        }
        self.status = Some(format!(
            " Open link [{typed}…]  Enter to open, Esc to cancel "
        ));
        self.following = Some(typed);
        None
    }

    /// Opens whatever number has been typed so far.
    pub fn take_typed_link(&mut self) -> Option<String> {
        let typed = self.following.take()?;
        self.status = None;
        let number: usize = typed.parse().ok()?;
        self.article_links().get(number.checked_sub(1)?).cloned()
    }

    /// Abandons a half-typed link number.
    pub fn cancel_following(&mut self) {
        if self.following.take().is_some() {
            self.status = None;
        }
    }

    /// Lays the article out again if anything that shapes it has changed.
    ///
    /// Called at the top of the frame and by anything that needs the rows.
    /// Everything else reads [`Self::detail_rows`], which is a borrow.
    /// Returns whether it actually laid anything out, which is what a test
    /// needs to know and what every caller can ignore.
    pub fn refresh_layout(&mut self) -> bool {
        let width = self.detail_viewport.0;
        let entry = self.current_entry();
        let title = entry.map(|entry| entry.title.clone()).unwrap_or_default();
        let link = entry.and_then(|entry| entry.link.clone());
        let source = self.article_source().unwrap_or_default();

        let same = self.layout.measure == Some(self.measure)
            && self.layout.width == width
            && self.layout.title == title
            && self.layout.link == link
            && self.layout.source == source;
        if same {
            return false;
        }

        let source = source.to_string();
        let rows = self.lay_out_detail();
        self.layout = Layout {
            source,
            title,
            link,
            width,
            measure: Some(self.measure),
            rows,
        };
        true
    }

    /// The rows the detail pane draws, as laid out by [`Self::refresh_layout`].
    pub fn detail_rows(&self) -> &[crate::article::Row] {
        &self.layout.rows
    }

    /// The detail pane as plain text, for measuring and for tests.
    pub fn detail_lines(&mut self) -> Vec<String> {
        self.refresh_layout();
        self.detail_rows()
            .iter()
            .map(|row| {
                let body: String = row
                    .spans
                    .iter()
                    .map(|span| span.text().to_string())
                    .collect();
                format!("{}{}", " ".repeat(row.indent), body)
            })
            .collect()
    }

    /// Which of [`Self::detail_lines`] are the entry's link, as (first, count).
    ///
    /// A long URL wraps, so the answer is a range: clicking the tail of a
    /// wrapped link should open it just as readily as clicking its head.
    pub fn detail_link_lines(&self) -> Option<(usize, usize)> {
        let entry = self.current_entry()?;
        let link = entry.link.as_deref().filter(|l| !l.is_empty())?;
        let width = self.detail_viewport.0 as usize;
        let title = crate::text::wrap(&entry.title, width).len();
        let lines = crate::text::wrap(link, width).len();
        (lines > 0).then_some((title, lines))
    }

    /// The furthest the detail pane can scroll and still show text.
    ///
    /// Stopping here is what keeps the pane from scrolling off into blank space.
    pub fn max_detail_scroll(&mut self) -> u16 {
        self.refresh_layout();
        let lines = self.layout.rows.len() as u16;
        lines.saturating_sub(self.detail_viewport.1)
    }

    /// Scrolls a list's view without disturbing the selection.
    ///
    /// The wheel scrolls what is on screen; it does not drag the cursor
    /// through the list, which would mark entries read on the way past and
    /// never stop, because selection wraps and a view does not.
    pub fn scroll_list(&mut self, pane: Pane, delta: isize) {
        let (offset, view) = match pane {
            Pane::Feeds => (&mut self.feeds_offset, self.feeds_view),
            Pane::Entries => (&mut self.entries_offset, self.entries_view),
            Pane::Detail => return,
        };
        // Stops at both ends. Wrapping is right for `j`, which is a step
        // through a list, and wrong for a wheel, which is a view moving.
        let last = view.rows.saturating_sub(view.height);
        let next = (*offset as isize + delta).clamp(0, last as isize);
        *offset = next as usize;
    }

    /// Whether something is drawn over the panes and owns the screen.
    ///
    /// The renderer draws these last so they cover everything; the hit map has
    /// to agree, or a click lands on a list nobody can see.
    pub fn overlay_open(&self) -> bool {
        self.help_open || self.adding.is_some() || self.moving.is_some()
    }

    /// Where the entry list should start, given what is being drawn now.
    ///
    /// Follows the selection only when the selection has moved: following it
    /// every frame would undo the wheel, and never following it would let `j`
    /// walk off the bottom of the screen.
    ///
    /// Called by the renderer, because only the renderer knows which row the
    /// selection landed on once folders and filters have had their say. The
    /// rule lives here; the facts come from there.
    pub fn entries_start(&mut self, selected: Option<usize>, height: usize, rows: usize) -> usize {
        let moved = (self.selected_feed, self.selected_entry) != self.followed;
        self.followed = (self.selected_feed, self.selected_entry);
        // The pane may have grown since the wheel last moved, stranding the
        // offset past the last row that can be the first one.
        let offset = self.entries_offset.min(rows.saturating_sub(height));
        self.entries_offset = match moved {
            true => scrolled_to_show(selected, offset, height),
            false => offset,
        };
        self.entries_offset
    }

    /// The same for the feed list.
    pub fn feeds_start(&mut self, selected: Option<usize>, height: usize, rows: usize) -> usize {
        let moved = self.selected_feed != self.followed_feed;
        self.followed_feed = self.selected_feed;
        let offset = self.feeds_offset.min(rows.saturating_sub(height));
        self.feeds_offset = match moved {
            true => scrolled_to_show(selected, offset, height),
            false => offset,
        };
        self.feeds_offset
    }

    pub fn scroll_detail(&mut self, delta: i16) {
        let next = self.detail_scroll as i32 + delta as i32;
        self.detail_scroll = next.clamp(0, self.max_detail_scroll() as i32) as u16;
    }

    /// Scrolls by a screenful, less two lines.
    ///
    /// The overlap is what keeps a paragraph from being cut in half by the
    /// page break: the last lines of one page open the next.
    pub fn page(&mut self, delta: i16) {
        let step = self.detail_viewport.1.saturating_sub(2).max(1);
        self.scroll_detail(delta.signum() * step.min(i16::MAX as u16) as i16);
    }

    /// Gives the article the screen, or gives it back.
    pub fn toggle_reading(&mut self) {
        self.reading = !self.reading;
        if self.reading {
            self.restore_focus = self.focus;
            // Nothing else is on screen to steer, so the keys that scroll had
            // better be the ones already under the reader's fingers.
            self.focus = Pane::Detail;
        } else {
            self.focus = self.restore_focus;
        }
    }

    /// Saves where the reader has got to, and restores it when they return.
    ///
    /// Called once per turn of the event loop rather than at each of the many
    /// places that move the selection, so no new one can forget to.
    pub fn keep_place(&mut self) {
        let key = self
            .current_entry()
            .and_then(|entry| entry.keys.first().cloned());
        if key == self.marked {
            if let Some(key) = &self.marked {
                self.marks.insert(key.clone(), self.detail_scroll);
            }
        } else {
            self.detail_scroll = key
                .as_ref()
                .and_then(|key| self.marks.get(key))
                .copied()
                .unwrap_or(0);
            self.marked = key;
        }
    }

    pub fn select_next(&mut self) {
        match self.focus {
            Pane::Feeds => {
                self.selected_feed = self.step_feed(1);
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
                self.selected_feed = self.step_feed(-1);
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
/// The smallest offset change that puts `row` inside a window of `height`.
fn scrolled_to_show(row: Option<usize>, offset: usize, height: usize) -> usize {
    let (Some(row), true) = (row, height > 0) else {
        return offset;
    };
    if row < offset {
        return row;
    }
    if row >= offset + height {
        return row + 1 - height;
    }
    offset
}

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
            content: String::new(),
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
    fn starring_is_reflected_in_the_starred_view() {
        let mut app = two_feeds();
        assert!(app.starred_results().is_empty());

        app.toggle_star();
        assert_eq!(app.starred_results(), vec![(0, 0)]);

        app.selected_feed = 1;
        app.selected_entry = 1;
        app.toggle_star();
        assert_eq!(app.starred_results(), vec![(0, 0), (1, 1)]);
    }

    #[test]
    fn unstarring_removes_it_from_the_view() {
        let mut app = two_feeds();
        assert!(app.toggle_star());
        assert!(!app.toggle_star());
        assert!(app.starred_results().is_empty());
    }

    #[test]
    fn opening_the_starred_view_selects_the_first_starred_entry() {
        let mut app = two_feeds();
        app.selected_feed = 1;
        app.selected_entry = 1;
        app.toggle_star();

        app.selected_feed = 0;
        app.selected_entry = 0;
        app.toggle_starred_view();
        assert_eq!((app.selected_feed, app.selected_entry), (1, 1));
    }

    #[test]
    fn the_starred_view_toggles_off_again() {
        let mut app = two_feeds();
        app.toggle_starred_view();
        assert!(app.starred_view);
        app.toggle_starred_view();
        assert!(!app.starred_view);
    }

    #[test]
    fn g_and_shift_g_jump_to_the_ends_of_the_entry_list() {
        let mut app = app();
        app.focus = Pane::Entries;
        app.select_last();
        assert_eq!(app.selected_entry, 1);
        app.select_first();
        assert_eq!(app.selected_entry, 0);
    }

    #[test]
    fn g_and_shift_g_jump_to_the_ends_of_the_feed_list() {
        let mut app = app();
        app.select_last();
        assert_eq!(app.selected_feed, 1);
        app.select_first();
        assert_eq!(app.selected_feed, 0);
    }

    #[test]
    fn shift_g_in_the_detail_pane_goes_to_the_last_line() {
        let mut app = scrollable();
        app.focus = Pane::Detail;
        app.select_last();
        let max = app.max_detail_scroll();
        assert_eq!(app.detail_scroll, max);
        app.select_first();
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn a_half_page_scales_with_the_pane_height() {
        let mut app = App::new(
            vec![Feed {
                title: "A".into(),
                url: "https://a.example".into(),
                status: crate::feed::Status::Idle,
                entries: (0..40).map(|i| entry(&format!("e{i}"))).collect(),
            }],
            ReadState::default(),
        );
        app.focus = Pane::Entries;

        app.entries_viewport = (80, 10);
        app.half_page(1);
        assert_eq!(app.selected_entry, 5, "half of ten");

        app.selected_entry = 0;
        app.entries_viewport = (80, 20);
        app.half_page(1);
        assert_eq!(app.selected_entry, 10, "half of twenty");
    }

    #[test]
    fn a_half_page_in_a_tiny_pane_still_moves() {
        let mut app = app();
        app.focus = Pane::Entries;
        app.entries_viewport = (80, 1);
        app.half_page(1);
        assert_eq!(app.selected_entry, 1);
    }

    #[test]
    fn next_unread_crosses_into_the_following_feed() {
        let mut app = app();
        app.focus = Pane::Entries;
        // Read everything in feed 0.
        for entry in app.feeds[0].entries.clone() {
            app.read.mark_read(&entry);
        }
        assert!(app.next_unread(true));
        assert_eq!(app.selected_feed, 1, "moved on to the next feed");
        assert_eq!(app.selected_entry, 0);
    }

    #[test]
    fn next_unread_reports_when_there_is_nothing_left() {
        let mut app = app();
        for index in 0..app.feeds.len() {
            for entry in app.feeds[index].entries.clone() {
                app.read.mark_read(&entry);
            }
        }
        assert!(!app.next_unread(true), "nothing unread anywhere");
    }

    #[test]
    fn previous_unread_walks_backwards_across_feeds() {
        let mut app = app();
        app.selected_feed = 1;
        app.selected_entry = 0;
        assert!(app.next_unread(false));
        assert_eq!(app.selected_feed, 0);
        assert_eq!(app.selected_entry, 1, "last entry of the previous feed");
    }

    #[test]
    fn next_unread_on_an_empty_reader_is_harmless() {
        let mut app = App::new(Vec::new(), ReadState::default());
        assert!(!app.next_unread(true));
    }

    fn tagged() -> App {
        let sources = vec![
            crate::config::FeedSource {
                url: "https://a.example".into(),
                refresh_minutes: None,
                title: None,
                tags: vec!["News".into(), "Rust".into()],
            },
            crate::config::FeedSource {
                url: "https://b.example".into(),
                refresh_minutes: None,
                title: None,
                tags: Vec::new(),
            },
        ];
        app().with_tags(&sources)
    }

    fn dated() -> App {
        use chrono::TimeZone;
        let at = |y| chrono::Utc.with_ymd_and_hms(y, 1, 1, 0, 0, 0).unwrap();
        let make = |title: &str, year: Option<i32>| Entry {
            title: title.into(),
            link: None,
            published: year.map(at),
            summary: String::new(),
            content: String::new(),
            keys: vec![format!("id:{title}")],
        };
        App::new(
            vec![
                Feed {
                    title: "A".into(),
                    url: "https://a.example".into(),
                    status: crate::feed::Status::Idle,
                    entries: vec![make("a-2020", Some(2020)), make("a-undated", None)],
                },
                Feed {
                    title: "B".into(),
                    url: "https://b.example".into(),
                    status: crate::feed::Status::Idle,
                    entries: vec![make("b-2022", Some(2022)), make("b-2018", Some(2018))],
                },
            ],
            ReadState::default(),
        )
    }

    fn titles(app: &App, rows: &[(usize, usize)]) -> Vec<String> {
        rows.iter()
            .map(|(f, e)| app.feeds[*f].entries[*e].title.clone())
            .collect()
    }

    #[test]
    fn the_all_feeds_view_lists_every_entry_newest_first() {
        let app = dated();
        assert_eq!(
            titles(&app, &app.all_entries()),
            ["b-2022", "a-2020", "b-2018", "a-undated"]
        );
    }

    #[test]
    fn the_sort_order_reverses() {
        let mut app = dated();
        app.toggle_sort();
        assert_eq!(
            titles(&app, &app.all_entries()),
            ["b-2018", "a-2020", "b-2022", "a-undated"]
        );
    }

    #[test]
    fn undated_entries_sort_last_in_both_directions() {
        let mut app = dated();
        let last = |app: &App| titles(app, &app.all_entries()).last().cloned().unwrap();
        assert_eq!(last(&app), "a-undated");
        app.toggle_sort();
        assert_eq!(last(&app), "a-undated", "still last when reversed");
    }

    #[test]
    fn the_all_feeds_view_toggles_and_selects_the_first_row() {
        let mut app = dated();
        app.selected_feed = 0;
        app.selected_entry = 1;
        app.toggle_all_feeds_view();
        assert!(app.all_feeds_view);
        assert_eq!((app.selected_feed, app.selected_entry), (1, 0), "b-2022");
        app.toggle_all_feeds_view();
        assert!(!app.all_feeds_view);
    }

    fn source(url: &str) -> crate::config::FeedSource {
        crate::config::FeedSource {
            url: url.into(),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn reloading_picks_up_a_new_feed_without_disturbing_the_others() {
        let mut app = app();
        app.focus = Pane::Entries;
        app.mark_current_read();
        let before = app.unread(0);

        app.reconcile(&[
            source("https://a.example"),
            source("https://b.example"),
            source("https://new.example"),
        ]);

        assert_eq!(app.feeds.len(), 3);
        assert_eq!(app.feeds[0].entries.len(), 2, "existing entries kept");
        assert!(app.feeds[2].entries.is_empty(), "the new feed is empty");
        assert_eq!(app.unread(0), before, "read state survived the reload");
    }

    #[test]
    fn reloading_drops_a_feed_that_was_removed() {
        let mut app = app();
        app.selected_feed = 1;
        app.reconcile(&[source("https://a.example")]);
        assert_eq!(app.feeds.len(), 1);
        assert_eq!(app.selected_feed, 0, "cursor moved off the removed feed");
    }

    #[test]
    fn reloading_applies_new_tags() {
        let mut app = app();
        let mut tagged = source("https://a.example");
        tagged.tags = vec!["News".into()];
        app.reconcile(&[tagged, source("https://b.example")]);
        assert_eq!(app.path_of(0), ["News"]);
        assert!(app.path_of(1).is_empty());
    }

    #[test]
    fn only_the_new_feeds_need_fetching() {
        let mut app = app();
        app.reconcile(&[
            source("https://a.example"),
            source("https://b.example"),
            source("https://new.example"),
        ]);
        assert_eq!(app.indices_without_entries(), vec![2]);
    }

    #[test]
    fn a_feed_keeps_its_whole_folder_path() {
        // Reading only the first tag is what flattened every nested folder.
        let app = tagged();
        assert_eq!(app.path_of(0), ["News", "Rust"]);
        assert!(app.path_of(1).is_empty());
    }

    #[test]
    fn every_folder_on_the_path_can_be_moved_into() {
        assert_eq!(
            tagged().folders(),
            vec![
                vec!["News".to_string()],
                vec!["News".to_string(), "Rust".to_string()],
            ]
        );
    }

    #[test]
    fn folding_an_outer_folder_hides_the_feeds_below_it() {
        let mut app = tagged();
        assert_eq!(app.selectable_feeds(), vec![0, 1]);

        app.toggle_folder(&["News".to_string()]);
        assert_eq!(app.selectable_feeds(), vec![1], "the nested feed is hidden");

        app.toggle_folder(&["News".to_string()]);
        assert_eq!(app.selectable_feeds(), vec![0, 1], "and comes back");
    }

    #[test]
    fn folding_is_per_path_not_per_name() {
        // Two folders can share a name at different points in the tree.
        let mut app = tagged();
        app.toggle_folder(&["News".to_string(), "Rust".to_string()]);
        assert!(app.is_collapsed(&["News".to_string(), "Rust".to_string()]));
        assert!(!app.is_collapsed(&["News".to_string()]));
    }

    #[test]
    fn the_move_picker_opens_on_where_the_feed_already_is() {
        // Opening on an arbitrary row would show a lie about the feed.
        let mut app = tagged();
        app.selected_feed = 0;
        app.start_move();
        let moving = app.moving.as_ref().expect("open");
        assert_eq!(
            moving.choices[moving.selected],
            MoveTarget::Folder(vec!["News".into(), "Rust".into()])
        );
    }

    #[test]
    fn the_picker_offers_the_top_level_every_folder_and_a_new_one() {
        let mut app = tagged();
        app.start_move();
        let choices = &app.moving.as_ref().expect("open").choices;
        assert_eq!(choices.first(), Some(&MoveTarget::TopLevel));
        assert_eq!(choices.last(), Some(&MoveTarget::New));
        assert!(choices.contains(&MoveTarget::Folder(vec!["News".into()])));
    }

    #[test]
    fn choosing_the_top_level_clears_the_path() {
        let mut app = tagged();
        app.selected_feed = 0;
        app.start_move();
        app.moving.as_mut().expect("open").selected = 0;
        assert_eq!(app.move_destination(), Some((0, Vec::new())));
    }

    #[test]
    fn a_new_folder_needs_a_name_before_it_is_a_destination() {
        let mut app = tagged();
        app.start_move();
        let last = app.moving.as_ref().expect("open").choices.len() - 1;
        app.moving.as_mut().expect("open").selected = last;

        assert_eq!(
            app.move_destination(),
            None,
            "an unnamed folder is not a place"
        );

        for ch in "Papers".chars() {
            app.type_move(ch);
        }
        assert_eq!(
            app.move_destination().map(|(_, path)| path),
            Some(vec!["Papers".to_string()])
        );
    }

    #[test]
    fn a_typed_slash_nests_in_one_go() {
        let mut app = tagged();
        app.start_move();
        let last = app.moving.as_ref().expect("open").choices.len() - 1;
        app.moving.as_mut().expect("open").selected = last;
        for ch in "Reading / Papers".chars() {
            app.type_move(ch);
        }
        assert_eq!(
            app.move_destination().map(|(_, path)| path),
            Some(vec!["Reading".to_string(), "Papers".to_string()])
        );
    }

    #[test]
    fn typing_only_reaches_the_new_folder_choice() {
        // Otherwise typing while an existing folder is selected would quietly
        // build a name nobody can see.
        let mut app = tagged();
        app.start_move();
        app.moving.as_mut().expect("open").selected = 0;
        app.type_move('x');
        assert!(app.moving.as_ref().expect("open").typed.is_empty());
    }

    #[test]
    fn the_picker_cursor_wraps() {
        let mut app = tagged();
        app.start_move();
        let count = app.moving.as_ref().expect("open").choices.len();
        app.moving.as_mut().expect("open").selected = count - 1;
        app.step_move(1);
        assert_eq!(app.moving.as_ref().expect("open").selected, 0);
    }

    #[test]
    fn cancelling_the_picker_changes_nothing() {
        let mut app = tagged();
        app.start_move();
        app.cancel_move();
        assert!(app.moving.is_none());
        assert_eq!(app.path_of(0), ["News", "Rust"], "the feed did not move");
    }

    /// A feed that publishes a teaser and a link, as many do.
    fn teaser() -> App {
        App::new(
            vec![Feed {
                title: "Teaser".into(),
                url: "https://a.example".into(),
                status: crate::feed::Status::Idle,
                entries: vec![Entry {
                    title: "Post".into(),
                    link: Some("https://a.example/post".into()),
                    published: None,
                    summary: "A short teaser.".into(),
                    content: "<p>A short teaser.</p>".into(),
                    keys: vec!["id:post".into()],
                }],
            }],
            ReadState::default(),
        )
    }

    #[test]
    fn without_a_fetch_the_feeds_own_text_is_shown() {
        let mut app = teaser();
        app.detail_viewport = (60, 20);
        let text = app.detail_lines().join(" ");
        assert!(text.contains("A short teaser."));
    }

    #[test]
    fn a_fetched_article_replaces_the_teaser() {
        let mut app = teaser();
        app.detail_viewport = (60, 20);
        app.article = Some("<p>The whole piece, at last.</p>".into());

        let text = app.detail_lines().join(" ");
        assert!(text.contains("The whole piece, at last."));
        assert!(
            !text.contains("A short teaser."),
            "the teaser was still shown: {text}"
        );
    }

    #[test]
    fn a_fetched_article_is_rendered_as_a_document_not_flattened() {
        let mut app = teaser();
        app.detail_viewport = (60, 20);
        app.article = Some("<p>Intro.</p><pre><code>one\n  two</code></pre>".into());

        let lines = app.detail_lines();
        assert!(lines.iter().any(|line| line.contains("Intro.")));
        assert!(lines.iter().any(|line| line.trim_end() == "  one"));
        assert!(lines.iter().any(|line| line.trim_end() == "    two"));
    }

    #[test]
    fn a_typed_url_is_only_offered_once_it_looks_like_one() {
        // Guessing a scheme would mean fetching something nobody asked for.
        let mut app = app();
        app.start_add();
        for ch in "example".chars() {
            app.type_add(ch);
        }
        assert_eq!(app.add_url(), None);

        app.cancel_add();
        app.start_add();
        for ch in "https://example.com/feed".chars() {
            app.type_add(ch);
        }
        assert_eq!(app.add_url().as_deref(), Some("https://example.com/feed"));
    }

    #[test]
    fn the_add_prompt_can_be_corrected_and_cancelled() {
        let mut app = app();
        app.start_add();
        for ch in "https://a.example/x".chars() {
            app.type_add(ch);
        }
        app.backspace_add();
        assert_eq!(app.add_url().as_deref(), Some("https://a.example/"));

        app.cancel_add();
        assert!(app.adding.is_none());
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
                    content: String::new(),
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

    #[test]
    fn reading_mode_takes_focus_and_gives_it_back() {
        let mut app = app();
        app.focus = Pane::Entries;

        app.toggle_reading();
        assert!(app.reading);
        assert_eq!(
            app.focus,
            Pane::Detail,
            "the scroll keys must reach the article"
        );

        app.toggle_reading();
        assert!(!app.reading);
        assert_eq!(
            app.focus,
            Pane::Entries,
            "the reader was put back where they were"
        );
    }

    #[test]
    fn a_page_is_a_screenful_less_an_overlap() {
        let mut app = app();
        app.focus = Pane::Detail;
        app.detail_viewport = (60, 20);
        // Deeper than any paging this test does, so nothing clamps.
        app.feeds[0].entries[0].summary = "word ".repeat(4000);

        app.page(1);
        assert_eq!(
            app.detail_scroll, 18,
            "a page is the viewport less two lines"
        );
        app.page(1);
        assert_eq!(app.detail_scroll, 36);
        app.page(-1);
        assert_eq!(
            app.detail_scroll, 18,
            "paging back returns to where it started"
        );
    }

    #[test]
    fn paging_stops_at_the_end_rather_than_running_past_it() {
        let mut app = app();
        app.detail_viewport = (60, 20);
        for _ in 0..50 {
            app.page(1);
        }
        let max = app.max_detail_scroll();
        assert_eq!(app.detail_scroll, max);
    }

    #[test]
    fn an_article_is_reopened_where_it_was_left() {
        let mut app = app();
        app.focus = Pane::Entries;
        app.detail_viewport = (60, 20);
        app.feeds[0].entries[0].summary = "word ".repeat(4000);
        app.keep_place();

        app.scroll_detail(12);
        app.keep_place();
        assert_eq!(app.detail_scroll, 12);

        // Move away: the next article starts at its own beginning.
        app.select_next();
        app.keep_place();
        assert_eq!(app.detail_scroll, 0, "a new article starts at the top");

        // And back: the place is still kept.
        app.select_previous();
        app.keep_place();
        assert_eq!(
            app.detail_scroll, 12,
            "the reader was returned to their place"
        );
    }

    #[test]
    fn a_place_is_kept_per_article_not_shared_between_them() {
        let mut app = app();
        app.focus = Pane::Entries;
        app.detail_viewport = (60, 20);
        for entry in &mut app.feeds[0].entries {
            entry.summary = "word ".repeat(4000);
        }
        app.keep_place();
        app.scroll_detail(5);
        app.keep_place();

        app.select_next();
        app.keep_place();
        app.scroll_detail(9);
        app.keep_place();

        app.select_previous();
        app.keep_place();
        assert_eq!(app.detail_scroll, 5, "the first article kept its own place");
        app.select_next();
        app.keep_place();
        assert_eq!(app.detail_scroll, 9, "and the second kept its own");
    }

    /// An entry whose text carries `count` distinct links.
    fn with_links(count: usize) -> App {
        let mut app = app();
        app.focus = Pane::Entries;
        app.detail_viewport = (80, 20);
        app.article = Some(
            (1..=count)
                .map(|n| format!("<p>See <a href=\"https://example.com/{n}\">link {n}</a>.</p>"))
                .collect::<String>(),
        );
        app
    }

    #[test]
    fn the_articles_links_are_numbered_in_the_order_they_appear() {
        let app = with_links(3);
        assert_eq!(
            app.article_links(),
            vec![
                "https://example.com/1",
                "https://example.com/2",
                "https://example.com/3"
            ]
        );
    }

    #[test]
    fn a_digit_that_can_only_mean_one_link_opens_it_at_once() {
        let mut app = with_links(3);
        assert_eq!(
            app.type_link_digit('2').as_deref(),
            Some("https://example.com/2")
        );
        assert!(app.following.is_none(), "nothing is left half-typed");
    }

    #[test]
    fn a_digit_that_might_yet_grow_waits_for_the_next_one() {
        let mut app = with_links(30);
        // With thirty links, `2` might still become `21`.
        assert_eq!(app.type_link_digit('2'), None);
        assert_eq!(app.following.as_deref(), Some("2"));
        assert!(app.status.is_some(), "the reader is not told it is waiting");

        assert_eq!(
            app.type_link_digit('1').as_deref(),
            Some("https://example.com/21")
        );
    }

    #[test]
    fn a_half_typed_number_can_be_opened_or_abandoned() {
        let mut app = with_links(30);
        app.type_link_digit('2');
        assert_eq!(
            app.take_typed_link().as_deref(),
            Some("https://example.com/2"),
            "enter opens what has been typed"
        );

        app.type_link_digit('2');
        app.cancel_following();
        assert!(app.following.is_none());
        assert!(app.status.is_none(), "the prompt was left on screen");
        assert_eq!(app.take_typed_link(), None);
    }

    #[test]
    fn a_number_with_no_link_behind_it_says_so_rather_than_opening_something_else() {
        let mut app = with_links(3);
        assert_eq!(app.type_link_digit('9'), None);
        assert!(
            app.status
                .as_deref()
                .unwrap_or_default()
                .contains("No link"),
            "{:?}",
            app.status
        );
        assert!(app.following.is_none(), "a dead number does not linger");

        // Nor does zero, which numbers nothing.
        assert_eq!(app.type_link_digit('0'), None);
    }

    #[test]
    fn an_article_with_no_links_says_so() {
        let mut app = app();
        app.detail_viewport = (80, 20);
        app.article = Some("<p>Nothing to follow.</p>".into());
        assert_eq!(app.type_link_digit('1'), None);
        assert!(
            app.status
                .as_deref()
                .unwrap_or_default()
                .contains("no links"),
            "{:?}",
            app.status
        );
    }

    #[test]
    fn an_unchanged_article_is_laid_out_once() {
        // It used to be laid out twice a frame, ten frames a second: a quarter
        // of a core to show a page that was not changing.
        let mut app = app();
        app.detail_viewport = (80, 20);
        app.article = Some("<p>Something to lay out.</p>".into());

        assert!(app.refresh_layout(), "the first frame has to do the work");
        for _ in 0..10 {
            assert!(!app.refresh_layout(), "it laid the same article out again");
        }
    }

    #[test]
    fn everything_that_shapes_the_article_lays_it_out_again() {
        let mut app = app();
        app.detail_viewport = (80, 20);
        app.article = Some("<p>One.</p>".into());
        app.refresh_layout();

        // A different article.
        app.article = Some("<p>Two.</p>".into());
        assert!(app.refresh_layout(), "new markup was not noticed");

        // A different width: the wrap points move.
        app.detail_viewport = (60, 20);
        assert!(app.refresh_layout(), "a resize was not noticed");

        // A different measure: so does the margin.
        app.measure = crate::article::Measure {
            columns: Some(40),
            ascii: false,
        };
        assert!(app.refresh_layout(), "a new measure was not noticed");

        // ASCII changes the glyphs, so it changes the rows.
        app.measure = crate::article::Measure {
            columns: Some(40),
            ascii: true,
        };
        assert!(app.refresh_layout(), "the ASCII fallback was not noticed");

        // A different entry, with its own title and link in the header.
        app.focus = Pane::Entries;
        app.select_next();
        assert!(
            app.refresh_layout(),
            "moving to another entry was not noticed"
        );
    }

    #[test]
    fn an_article_of_the_same_length_is_still_noticed() {
        // Length is the cheap half of the comparison; it must not be all of it.
        let mut app = app();
        app.detail_viewport = (80, 20);
        app.article = Some("<p>aaaa</p>".into());
        app.refresh_layout();
        app.article = Some("<p>bbbb</p>".into());
        assert!(app.refresh_layout(), "same length, different words, missed");
        assert!(app.detail_lines().iter().any(|line| line.contains("bbbb")));
    }

    #[test]
    fn a_refresh_that_changes_an_entry_relays_it_out() {
        // The feed came back with a longer version of the same entry: same
        // selection, same width, different text.
        let mut app = app();
        app.detail_viewport = (80, 20);
        app.focus = Pane::Entries;
        app.feeds[0].entries[0].content = "<p>Short.</p>".into();
        app.refresh_layout();

        app.feeds[0].entries[0].content = "<p>Rather longer, now.</p>".into();
        assert!(
            app.refresh_layout(),
            "a refreshed entry kept the old layout"
        );
    }

    #[test]
    fn the_unsubscribe_prompt_says_what_it_is_about_to_throw_away() {
        let mut app = app();
        app.read.toggle_star(&app.feeds[0].entries[0].clone());

        app.request_bulk(Bulk::Unsubscribe);
        let prompt = app.pending_prompt().expect("a prompt");
        assert!(prompt.contains('A'), "it does not name the feed: {prompt}");
        assert!(
            prompt.contains('2'),
            "it does not say how many entries: {prompt}"
        );
        assert!(
            prompt.contains("1 starred"),
            "it does not warn about the starred entry: {prompt}"
        );
        assert!(prompt.contains("y / n"), "it does not say how to answer");
    }

    #[test]
    fn the_unsubscribe_prompt_stays_quiet_about_stars_when_there_are_none() {
        let mut app = app();
        app.request_bulk(Bulk::Unsubscribe);
        let prompt = app.pending_prompt().expect("a prompt");
        assert!(!prompt.contains("starred"), "{prompt}");
    }

    #[test]
    fn cancelling_an_unsubscribe_changes_nothing() {
        let mut app = app();
        let before = app.feeds.len();
        app.request_bulk(Bulk::Unsubscribe);
        app.cancel_bulk();
        assert!(app.pending_prompt().is_none());
        assert_eq!(app.feeds.len(), before);
    }

    #[test]
    fn confirming_an_unsubscribe_marks_nothing_read() {
        // It shares the confirmation machinery with the bulk marks, and must
        // not accidentally inherit what they do.
        let mut app = app();
        let unread = app.unread(0);
        app.request_bulk(Bulk::Unsubscribe);
        assert_eq!(app.confirm_bulk(), 0);
        assert_eq!(app.unread(0), unread, "it marked entries read");
    }

    #[test]
    fn dropping_a_feed_from_the_config_drops_it_from_the_reader() {
        // What `reload` does after the config is written.
        let mut app = two_feeds();
        app.selected_feed = 1;
        let sources: Vec<crate::config::FeedSource> = vec![crate::config::FeedSource {
            url: app.feeds[0].url.clone(),
            refresh_minutes: None,
            title: None,
            tags: Vec::new(),
        }];

        app.reconcile(&sources);
        assert_eq!(app.feeds.len(), 1);
        assert_eq!(
            app.selected_feed, 0,
            "the cursor was left pointing past the end"
        );
    }
}
