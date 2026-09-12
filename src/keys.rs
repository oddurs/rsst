//! What every key does, in one place.
//!
//! The dispatcher, the `--help` output and the in-app overlay all read this, so
//! a binding cannot be changed in one and forgotten in the others. Bindings can
//! be overridden from the config's `[keys]` table.

use std::collections::HashMap;

use anyhow::{Result, bail};
use crossterm::event::{KeyCode, KeyModifiers};

/// Something the reader can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Next,
    Previous,
    First,
    Last,
    HalfPageDown,
    HalfPageUp,
    CyclePane,
    ToggleGroup,
    MoveFeed,
    AddFeed,
    NextUnread,
    PreviousUnread,
    ToggleRead,
    MarkFeedRead,
    MarkAllRead,
    ToggleUnreadOnly,
    ToggleStar,
    ToggleStarredView,
    ToggleAllFeeds,
    ToggleSort,
    Search,
    FetchArticle,
    ToggleReading,
    Open,
    CopyLink,
    Refresh,
    ReloadConfig,
    Help,
    Quit,
}

/// Every action, with the config name and description that go with it.
///
/// The order is the order the help is shown in.
const ACTIONS: &[(Action, &str, &str, &str)] = &[
    (
        Action::Next,
        "next",
        "Moving",
        "next item, or scroll the detail pane",
    ),
    (
        Action::Previous,
        "previous",
        "Moving",
        "previous item, or scroll back",
    ),
    (Action::First, "first", "Moving", "first item"),
    (Action::Last, "last", "Moving", "last item"),
    (
        Action::HalfPageDown,
        "half_page_down",
        "Moving",
        "half a pane down",
    ),
    (
        Action::HalfPageUp,
        "half_page_up",
        "Moving",
        "half a pane up",
    ),
    (
        Action::CyclePane,
        "cycle_pane",
        "Moving",
        "cycle feeds / entries / detail",
    ),
    (
        Action::ToggleGroup,
        "toggle_group",
        "Moving",
        "fold a feed group away",
    ),
    (
        Action::NextUnread,
        "next_unread",
        "Moving",
        "next unread, across feeds",
    ),
    (
        Action::PreviousUnread,
        "previous_unread",
        "Moving",
        "previous unread",
    ),
    (
        Action::ToggleRead,
        "toggle_read",
        "Reading",
        "toggle read on this entry",
    ),
    (
        Action::MarkFeedRead,
        "mark_feed_read",
        "Reading",
        "mark this feed read (asks)",
    ),
    (
        Action::MarkAllRead,
        "mark_all_read",
        "Reading",
        "mark every feed read (asks)",
    ),
    (
        Action::ToggleUnreadOnly,
        "toggle_unread_only",
        "Reading",
        "show only unread",
    ),
    (
        Action::ToggleStar,
        "toggle_star",
        "Reading",
        "star this entry",
    ),
    (
        Action::ToggleStarredView,
        "toggle_starred_view",
        "Reading",
        "show only starred",
    ),
    (
        Action::ToggleAllFeeds,
        "toggle_all_feeds",
        "Reading",
        "show every feed as one list",
    ),
    (
        Action::ToggleSort,
        "toggle_sort",
        "Reading",
        "oldest first / newest first",
    ),
    (
        Action::ToggleReading,
        "toggle_reading",
        "Reading",
        "give the article the screen",
    ),
    (
        Action::FetchArticle,
        "fetch_article",
        "Reading",
        "fetch the full article",
    ),
    (Action::Search, "search", "Finding", "search every feed"),
    (
        Action::Open,
        "open",
        "Doing",
        "open the entry in your browser",
    ),
    (Action::CopyLink, "copy_link", "Doing", "copy its link"),
    (Action::Refresh, "refresh", "Doing", "refresh all feeds"),
    (
        Action::ReloadConfig,
        "reload_config",
        "Doing",
        "re-read the config file",
    ),
    (Action::Help, "help", "Doing", "show this help"),
    (Action::Quit, "quit", "Doing", "quit"),
];

impl Action {
    /// Every action's config name, for documentation and its tests.
    #[cfg(test)]
    pub fn all_names() -> Vec<&'static str> {
        ACTIONS.iter().map(|(_, name, ..)| *name).collect()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        ACTIONS
            .iter()
            .find(|(_, n, ..)| *n == name)
            .map(|(a, ..)| *a)
    }
}

/// The keys bound to each action.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<(KeyCode, KeyModifiers, Action)>,
}

/// The bindings shipped with rsst, as `(key, action)` in config spelling.
const DEFAULTS: &[(&str, Action)] = &[
    ("j", Action::Next),
    ("down", Action::Next),
    ("k", Action::Previous),
    ("up", Action::Previous),
    ("g", Action::First),
    ("home", Action::First),
    ("G", Action::Last),
    ("end", Action::Last),
    ("ctrl-d", Action::HalfPageDown),
    ("pagedown", Action::HalfPageDown),
    ("ctrl-u", Action::HalfPageUp),
    ("pageup", Action::HalfPageUp),
    ("tab", Action::CyclePane),
    ("enter", Action::ToggleGroup),
    ("space", Action::ToggleGroup),
    ("n", Action::NextUnread),
    ("p", Action::PreviousUnread),
    ("m", Action::ToggleRead),
    ("M", Action::MoveFeed),
    ("+", Action::AddFeed),
    ("a", Action::MarkFeedRead),
    ("A", Action::MarkAllRead),
    ("u", Action::ToggleUnreadOnly),
    ("s", Action::ToggleStar),
    ("S", Action::ToggleStarredView),
    ("v", Action::ToggleAllFeeds),
    ("t", Action::ToggleSort),
    ("/", Action::Search),
    ("f", Action::FetchArticle),
    ("z", Action::ToggleReading),
    ("o", Action::Open),
    ("y", Action::CopyLink),
    ("r", Action::Refresh),
    ("R", Action::ReloadConfig),
    ("?", Action::Help),
    ("q", Action::Quit),
];

impl Default for Keymap {
    fn default() -> Self {
        let bindings = DEFAULTS
            .iter()
            .map(|(key, action)| {
                let (code, mods) = parse_key(key).expect("built-in bindings parse");
                (code, mods, *action)
            })
            .collect();
        Self { bindings }
    }
}

impl Keymap {
    /// The defaults with the config's `[keys]` overrides applied.
    ///
    /// An override *replaces* every default for that action, so rebinding
    /// `quit` to `x` does not leave `q` quitting as well — which is the whole
    /// point of rebinding it.
    pub fn from_config(overrides: &HashMap<String, String>) -> Result<Self> {
        let mut map = Self::default();
        for (name, key) in overrides {
            let Some(action) = Action::from_name(name) else {
                let known: Vec<&str> = ACTIONS.iter().map(|(_, n, ..)| *n).collect();
                bail!(
                    "unknown action `{name}` in [keys].\n\nKnown actions: {}",
                    known.join(", ")
                );
            };
            let (code, mods) =
                parse_key(key).map_err(|err| anyhow::anyhow!("[keys].{name}: {err}"))?;
            map.bindings.retain(|(.., a)| *a != action);
            map.bindings.push((code, mods, action));
        }
        Ok(map)
    }

    /// What this keystroke means, if anything.
    pub fn action(&self, code: KeyCode, mods: KeyModifiers) -> Option<Action> {
        let mods = normalise(code, mods);
        self.bindings
            .iter()
            .find(|(c, m, _)| *c == code && normalise(*c, *m) == mods)
            .map(|(.., a)| *a)
    }

    /// The keys bound to an action, in config spelling.
    pub fn keys_for(&self, action: Action) -> Vec<String> {
        self.bindings
            .iter()
            .filter(|(.., a)| *a == action)
            .map(|(c, m, _)| describe_key(*c, *m))
            .collect()
    }

    /// Actions grouped into sections, for the help.
    pub fn sections(&self) -> Vec<(&'static str, Vec<(String, &'static str)>)> {
        let mut out: Vec<(&str, Vec<(String, &str)>)> = Vec::new();
        for (action, _, section, description) in ACTIONS {
            let keys = self.keys_for(*action);
            if keys.is_empty() {
                continue;
            }
            let row = (keys.join(" / "), *description);
            match out.iter_mut().find(|(name, _)| name == section) {
                Some((_, rows)) => rows.push(row),
                None => out.push((section, vec![row])),
            }
        }
        out
    }
}

/// Drops Shift from a character key, where it is already in the character.
///
/// Terminals report Shift+r as `Char('R')` *with* the Shift modifier set. A
/// binding written `A` parses to `Char('A')` with no modifiers, so comparing
/// modifiers literally would leave every capital-letter binding dead. Shift
/// still matters for keys with no character of their own, like Shift+Tab.
fn normalise(code: KeyCode, mods: KeyModifiers) -> KeyModifiers {
    match code {
        KeyCode::Char(_) => mods & !KeyModifiers::SHIFT,
        _ => mods,
    }
}

/// Reads a key from its config spelling, such as `j`, `ctrl-d` or `pagedown`.
pub fn parse_key(text: &str) -> Result<(KeyCode, KeyModifiers)> {
    let mut mods = KeyModifiers::NONE;
    let mut rest = text;

    loop {
        let lower = rest.to_ascii_lowercase();
        if let Some(tail) = lower.strip_prefix("ctrl-") {
            mods |= KeyModifiers::CONTROL;
            rest = &rest[rest.len() - tail.len()..];
        } else if let Some(tail) = lower.strip_prefix("alt-") {
            mods |= KeyModifiers::ALT;
            rest = &rest[rest.len() - tail.len()..];
        } else {
            break;
        }
    }

    let code = match rest.to_ascii_lowercase().as_str() {
        "" => bail!("`{text}` names no key"),
        "tab" => KeyCode::Tab,
        "enter" => KeyCode::Enter,
        "space" => KeyCode::Char(' '),
        "esc" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        _ => {
            let mut chars = rest.chars();
            match (chars.next(), chars.next()) {
                // Case matters for a literal character: `A` is not `a`.
                (Some(ch), None) => KeyCode::Char(ch),
                _ => bail!("`{text}` is not a key rsst knows"),
            }
        }
    };
    Ok((code, mods))
}

/// The config spelling of a key, for showing in the help.
fn describe_key(code: KeyCode, mods: KeyModifiers) -> String {
    let mut out = String::new();
    if mods.contains(KeyModifiers::CONTROL) {
        out.push_str("ctrl-");
    }
    if mods.contains(KeyModifiers::ALT) {
        out.push_str("alt-");
    }
    let name = match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(ch) => ch.to_string(),
        KeyCode::Tab => "tab".into(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        other => format!("{other:?}").to_lowercase(),
    };
    out.push_str(&name);
    out
}

/// The width of the widest key column, so the two columns line up.
pub fn key_column_width(map: &Keymap) -> usize {
    map.sections()
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .map(|(keys, _)| keys.len())
        .max()
        .unwrap_or(0)
}

/// The key reference as plain text, for `--help`.
pub fn as_text(map: &Keymap) -> String {
    let width = key_column_width(map);
    let mut out = String::new();
    for (section, rows) in map.sections() {
        out.push_str(&format!("\n{}:\n", section.to_uppercase()));
        for (keys, description) in rows {
            out.push_str(&format!("  {keys:width$}  {description}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overrides(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn the_defaults_all_parse() {
        let map = Keymap::default();
        assert_eq!(
            map.action(KeyCode::Char('q'), KeyModifiers::NONE),
            Some(Action::Quit)
        );
        assert_eq!(
            map.action(KeyCode::Char('d'), KeyModifiers::CONTROL),
            Some(Action::HalfPageDown)
        );
    }

    #[test]
    fn case_distinguishes_two_bindings() {
        let map = Keymap::default();
        assert_eq!(
            map.action(KeyCode::Char('a'), KeyModifiers::NONE),
            Some(Action::MarkFeedRead)
        );
        assert_eq!(
            map.action(KeyCode::Char('A'), KeyModifiers::NONE),
            Some(Action::MarkAllRead)
        );
    }

    #[test]
    fn a_capital_letter_binding_matches_the_shift_the_terminal_reports() {
        // Terminals send Shift+a as Char('A') WITH the Shift modifier; a
        // binding written `A` has none. Every capital binding depends on this.
        let map = Keymap::default();
        assert_eq!(
            map.action(KeyCode::Char('A'), KeyModifiers::SHIFT),
            Some(Action::MarkAllRead)
        );
        assert_eq!(
            map.action(KeyCode::Char('G'), KeyModifiers::SHIFT),
            Some(Action::Last)
        );
        assert_eq!(
            map.action(KeyCode::Char('R'), KeyModifiers::SHIFT),
            Some(Action::ReloadConfig)
        );
    }

    #[test]
    fn shift_still_matters_for_keys_without_a_character() {
        let map = Keymap::default();
        // Tab is bound; Shift+Tab is a different keystroke and is not.
        assert_eq!(
            map.action(KeyCode::Tab, KeyModifiers::NONE),
            Some(Action::CyclePane)
        );
        assert_eq!(map.action(KeyCode::Tab, KeyModifiers::SHIFT), None);
    }

    #[test]
    fn control_is_still_required_where_it_is_bound() {
        let map = Keymap::default();
        assert_eq!(
            map.action(KeyCode::Char('d'), KeyModifiers::CONTROL),
            Some(Action::HalfPageDown)
        );
        // Plain `d` is not bound to anything.
        assert_eq!(map.action(KeyCode::Char('d'), KeyModifiers::NONE), None);
    }

    #[test]
    fn an_override_replaces_the_default_rather_than_adding_to_it() {
        let map = Keymap::from_config(&overrides(&[("quit", "x")])).expect("valid");
        assert_eq!(
            map.action(KeyCode::Char('x'), KeyModifiers::NONE),
            Some(Action::Quit)
        );
        assert_eq!(map.action(KeyCode::Char('q'), KeyModifiers::NONE), None);
    }

    #[test]
    fn an_unknown_action_is_reported_with_the_known_ones() {
        let err = Keymap::from_config(&overrides(&[("quti", "x")])).expect_err("rejected");
        let message = err.to_string();
        assert!(message.contains("unknown action `quti`"));
        assert!(message.contains("quit"), "lists what it could have meant");
    }

    #[test]
    fn an_unparseable_key_names_the_action_it_came_from() {
        let err = Keymap::from_config(&overrides(&[("quit", "nonsense")])).expect_err("rejected");
        assert!(err.to_string().contains("[keys].quit"));
    }

    #[test]
    fn modifiers_and_named_keys_parse() {
        assert_eq!(
            parse_key("ctrl-d").expect("parses"),
            (KeyCode::Char('d'), KeyModifiers::CONTROL)
        );
        assert_eq!(
            parse_key("pagedown").expect("parses"),
            (KeyCode::PageDown, KeyModifiers::NONE)
        );
        assert_eq!(
            parse_key("space").expect("parses"),
            (KeyCode::Char(' '), KeyModifiers::NONE)
        );
    }

    #[test]
    fn an_empty_key_is_rejected() {
        assert!(parse_key("").is_err());
        assert!(parse_key("ctrl-").is_err());
    }

    #[test]
    fn the_help_reflects_a_rebinding() {
        let map = Keymap::from_config(&overrides(&[("quit", "x")])).expect("valid");
        let text = as_text(&map);
        assert!(text.contains("  x "), "the new key is shown");
        let quit_row = text
            .lines()
            .find(|line| line.contains("quit"))
            .expect("quit is listed");
        assert!(!quit_row.contains(" q "), "the old key is gone: {quit_row}");
    }

    #[test]
    fn every_action_is_bound_and_described_by_default() {
        let map = Keymap::default();
        for (action, name, section, description) in ACTIONS {
            assert!(!map.keys_for(*action).is_empty(), "{name} is unbound");
            assert!(!description.is_empty(), "{name} has no description");
            assert!(!section.is_empty(), "{name} has no section");
        }
    }

    #[test]
    fn every_row_fits_an_eighty_column_terminal() {
        let map = Keymap::default();
        let width = key_column_width(&map);
        for (_, rows) in map.sections() {
            for (keys, description) in rows {
                let rendered = 2 + width + 2 + description.len() + 2;
                assert!(
                    rendered <= 80,
                    "{keys} + {description} is {rendered} columns"
                );
            }
        }
    }
}
