use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// On-disk configuration, loaded from `$XDG_CONFIG_HOME/rsst/config.toml`.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub feeds: Vec<FeedSource>,
    /// Whether to capture the mouse. Off gives the terminal its own
    /// click-to-select back.
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub mouse: bool,
    /// Colours.
    #[serde(default)]
    pub theme: crate::theme::ThemeConfig,
    /// Key overrides: action name to key, merged over the defaults.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub keys: std::collections::HashMap<String, String>,
    /// The widest line of prose, in columns. Zero uses the whole pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<usize>,
    /// How often to refresh, in minutes. Zero turns the timer off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_minutes: Option<u64>,
    /// How many feeds may be fetched at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_fetches: Option<usize>,
    /// The most a single feed may send, in megabytes. Zero means the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_feed_megabytes: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedSource {
    pub url: String,
    /// Overrides the global refresh interval for this feed alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_minutes: Option<u64>,
    /// Overrides the title advertised by the feed itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Groups this feed appears under. Empty means ungrouped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl Config {
    /// Loads the config, writing a starter file if none exists yet.
    ///
    /// `override_path` comes from `--config`; without it the platform config
    /// directory is used.
    pub fn load_or_init(override_path: Option<PathBuf>) -> Result<Self> {
        let path = match override_path {
            // An explicitly named config that isn't there is a mistake worth
            // reporting, not something to silently paper over with a starter.
            Some(path) => return Self::load_from(&path),
            None => config_path()?,
        };
        if !path.exists() {
            let starter = Self::starter();
            write(&path, &starter)?;
            return Ok(starter);
        }
        Self::load_from(&path)
    }

    /// Writes the config back, creating its directory if needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        write(path, self)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parsing config at {}", path.display()))
    }

    /// How prose should be set: the configured measure, or the default.
    ///
    /// Zero means "use the whole pane", which is how someone turns it off.
    /// What a fetch is allowed to do, from the config or the defaults.
    pub fn limits(&self) -> crate::feed::Limits {
        let max_body = self
            .max_feed_megabytes
            .filter(|mb| *mb > 0)
            .map(|mb| mb.saturating_mul(1024 * 1024))
            .unwrap_or(crate::feed::DEFAULT_MAX_BODY);
        crate::feed::Limits { max_body }
    }

    pub fn measure(&self, ascii: bool) -> crate::article::Measure {
        let columns = self.measure.unwrap_or(crate::article::DEFAULT_MEASURE);
        crate::article::Measure {
            columns: (columns > 0).then_some(columns),
            ascii,
        }
    }

    /// How often a given feed should be refreshed.
    ///
    /// The feed's own setting, then the global one, then the default. Zero
    /// anywhere means never, which is how the timer is turned off.
    pub fn refresh_interval(&self, source: &FeedSource) -> std::time::Duration {
        let minutes = source
            .refresh_minutes
            .or(self.refresh_minutes)
            .unwrap_or(DEFAULT_REFRESH_MINUTES);
        std::time::Duration::from_secs(minutes * 60)
    }

    /// Simultaneous fetches allowed, falling back to the built-in default.
    pub fn fetch_limit(&self) -> usize {
        self.max_concurrent_fetches
            .unwrap_or(crate::limit::DEFAULT_LIMIT)
    }

    fn starter() -> Self {
        Self {
            mouse: true,
            measure: None,
            refresh_minutes: None,
            theme: Default::default(),
            keys: Default::default(),
            max_concurrent_fetches: None,
            max_feed_megabytes: None,
            feeds: vec![FeedSource {
                url: "https://blog.rust-lang.org/feed.xml".into(),
                refresh_minutes: None,
                title: Some("Rust Blog".into()),
                tags: Vec::new(),
            }],
        }
    }
}

/// The config path actually in use: an explicit one, or the platform default.
/// How often feeds are refreshed when nothing says otherwise.
///
/// Half an hour: often enough that the reader is worth opening, rare enough
/// that no publisher would call it rude.
pub const DEFAULT_REFRESH_MINUTES: u64 = 30;

fn yes() -> bool {
    true
}

fn is_yes(value: &bool) -> bool {
    *value
}

pub fn config_path_or(override_path: Option<PathBuf>) -> Result<PathBuf> {
    match override_path {
        Some(path) => Ok(path),
        None => config_path(),
    }
}

/// Rewrites one feed's folder path in the config file.
///
/// Uses `toml_edit` rather than serialising the whole `Config` back out: the
/// config is hand-written and commented, and a round-trip through a plain
/// serialiser would return it stripped of every comment and reordered. Someone
/// moving a feed between folders has not asked for that.
pub fn set_feed_tags(path: &Path, url: &str, tags: &[String]) -> Result<()> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("reading config at {}", path.display()))?;
    let mut document = raw
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("parsing config at {}", path.display()))?;

    let feeds = document
        .get_mut("feeds")
        .and_then(|feeds| feeds.as_array_of_tables_mut())
        .context("the config has no [[feeds]] to edit")?;

    let feed = feeds
        .iter_mut()
        .find(|table| table.get("url").and_then(|u| u.as_str()) == Some(url))
        .with_context(|| format!("no feed in the config has the url {url}"))?;

    if tags.is_empty() {
        feed.remove("tags");
    } else {
        let mut array = toml_edit::Array::new();
        for tag in tags {
            array.push(tag.as_str());
        }
        feed["tags"] = toml_edit::value(array);
    }

    // Same atomic dance as everything else we write: a crash mid-write must
    // not leave someone without a config.
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, document.to_string())
        .with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))
}

/// Appends a feed to the config file.
///
/// Through `toml_edit` for the same reason moving a feed is: the file is
/// hand-written, and a round-trip through a serialiser would return it without
/// its comments.
pub fn add_feed(path: &Path, url: &str, title: Option<&str>) -> Result<()> {
    let raw = fs::read_to_string(path).unwrap_or_default();
    let mut document = raw
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("parsing config at {}", path.display()))?;

    let feeds = document
        .entry("feeds")
        .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .context("`feeds` in the config is not a list of feeds")?;

    if feeds
        .iter()
        .any(|table| table.get("url").and_then(|u| u.as_str()) == Some(url))
    {
        anyhow::bail!("that feed is already in the config");
    }

    let mut table = toml_edit::Table::new();
    table["url"] = toml_edit::value(url);
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        table["title"] = toml_edit::value(title);
    }
    feeds.push(table);

    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, document.to_string())
        .with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(crate::home::dir(crate::home::Kind::Config)?.join("config.toml"))
}

fn write(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating config directory {}", parent.display()))?;
    }
    let body = toml::to_string_pretty(config).context("serializing config")?;
    fs::write(path, body).with_context(|| format!("writing config to {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_feed_list() {
        let config: Config = toml::from_str(
            r#"
            [[feeds]]
            url = "https://example.com/feed.xml"

            [[feeds]]
            url = "https://example.org/atom"
            title = "Example Org"
            "#,
        )
        .expect("config should parse");

        assert_eq!(config.feeds.len(), 2);
        assert_eq!(config.feeds[0].title, None);
        assert_eq!(config.feeds[1].title.as_deref(), Some("Example Org"));
    }

    #[test]
    fn tags_are_optional_and_default_to_none() {
        let config: Config = toml::from_str(
            r#"
            [[feeds]]
            url = "https://a.example/feed"

            [[feeds]]
            url = "https://b.example/feed"
            tags = ["Rust", "Weekly"]
            "#,
        )
        .expect("parses");
        assert!(config.feeds[0].tags.is_empty());
        assert_eq!(config.feeds[1].tags, ["Rust", "Weekly"]);
    }

    #[test]
    fn the_measure_defaults_and_can_be_turned_off() {
        let bare: Config = toml::from_str("").expect("parses");
        assert_eq!(
            bare.measure(false).columns,
            Some(crate::article::DEFAULT_MEASURE)
        );

        let set: Config = toml::from_str("measure = 60").expect("parses");
        assert_eq!(set.measure(false).columns, Some(60));

        let off: Config = toml::from_str("measure = 0").expect("parses");
        assert_eq!(off.measure(false).columns, None, "zero uses the whole pane");
    }

    #[test]
    fn the_refresh_interval_falls_back_from_feed_to_global_to_default() {
        let config: Config = toml::from_str(
            r#"
            refresh_minutes = 10

            [[feeds]]
            url = "https://a.example/feed"

            [[feeds]]
            url = "https://b.example/feed"
            refresh_minutes = 120
            "#,
        )
        .expect("parses");

        assert_eq!(config.refresh_interval(&config.feeds[0]).as_secs(), 600);
        assert_eq!(config.refresh_interval(&config.feeds[1]).as_secs(), 7200);

        let bare: Config =
            toml::from_str("[[feeds]]\nurl = \"https://c.example\"").expect("parses");
        assert_eq!(
            bare.refresh_interval(&bare.feeds[0]).as_secs(),
            DEFAULT_REFRESH_MINUTES * 60
        );
    }

    #[test]
    fn zero_minutes_turns_the_timer_off() {
        let config: Config =
            toml::from_str("refresh_minutes = 0\n\n[[feeds]]\nurl = \"https://a.example\"")
                .expect("parses");
        assert!(config.refresh_interval(&config.feeds[0]).is_zero());
    }

    #[test]
    fn the_mouse_is_on_unless_the_config_says_otherwise() {
        let config: Config = toml::from_str("").expect("parses");
        assert!(config.mouse);

        let off: Config = toml::from_str("mouse = false").expect("parses");
        assert!(!off.mouse);
    }

    #[test]
    fn the_fetch_limit_defaults_when_unset() {
        let config: Config = toml::from_str("").expect("parses");
        assert_eq!(config.fetch_limit(), crate::limit::DEFAULT_LIMIT);
    }

    #[test]
    fn the_fetch_limit_is_configurable() {
        let config: Config = toml::from_str("max_concurrent_fetches = 3").expect("parses");
        assert_eq!(config.fetch_limit(), 3);
    }

    /// Every key the parser accepts, taken from serde rather than a list kept
    /// by hand — so a new field cannot be added without this noticing.
    fn documented_keys() -> Vec<String> {
        let populated = Config {
            feeds: vec![FeedSource {
                url: "https://example.com/feed".into(),
                refresh_minutes: None,
                title: Some("Example".into()),
                tags: vec!["Tag".into()],
            }],
            theme: crate::theme::ThemeConfig {
                name: Some("dark".into()),
                ascii: Some(false),
                overrides: Default::default(),
            },
            keys: Default::default(),
            max_concurrent_fetches: Some(8),
            max_feed_megabytes: Some(8),
            measure: Some(72),
            refresh_minutes: Some(30),
            mouse: false,
        };
        let rendered = toml::to_string(&populated).expect("serializes");
        rendered
            .lines()
            .filter_map(|line| line.split('=').next())
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty() && !key.starts_with('[') && !key.starts_with('#'))
            .collect()
    }

    #[test]
    fn the_example_config_parses() {
        let example = include_str!("../docs/config.example.toml");
        let config: Config = toml::from_str(example).expect("the documented example must parse");
        assert_eq!(config.feeds.len(), 2);
        assert_eq!(config.fetch_limit(), 8);
    }

    #[test]
    fn the_example_config_exercises_every_key_the_parser_accepts() {
        let example = include_str!("../docs/config.example.toml");
        for key in documented_keys() {
            assert!(
                example.contains(&key),
                "docs/config.example.toml never mentions `{key}`"
            );
        }
    }

    #[test]
    fn the_example_binds_every_action() {
        let example = include_str!("../docs/config.example.toml");
        let config: Config = toml::from_str(example).expect("parses");
        // It must also be a *valid* keymap, not merely valid TOML.
        crate::keys::Keymap::from_config(&config.keys).expect("the documented keys must be valid");
        // Every action the app knows must appear, so nothing is undiscoverable.
        for name in crate::keys::Action::all_names() {
            assert!(
                config.keys.contains_key(name),
                "docs/config.example.toml never binds `{name}`"
            );
        }
    }

    #[test]
    fn the_example_theme_resolves() {
        let example = include_str!("../docs/config.example.toml");
        let config: Config = toml::from_str(example).expect("parses");
        crate::theme::Theme::resolve(&config.theme, false)
            .expect("the documented theme must resolve");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rsst-config-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    const COMMENTED: &str = r#"# My feeds. Hand-written, and I would like to keep it that way.

# How many at once.
max_concurrent_fetches = 4

[[feeds]]
url = "https://a.example/feed"   # the good one
title = "Alpha"

[[feeds]]
url = "https://b.example/feed"
title = "Beta"
tags = ["Old"]
"#;

    #[test]
    fn moving_a_feed_keeps_every_comment_and_the_rest_of_the_file() {
        let path = scratch("comments.toml");
        fs::write(&path, COMMENTED).expect("write");

        set_feed_tags(
            &path,
            "https://a.example/feed",
            &["News".into(), "Rust".into()],
        )
        .expect("set");

        let after = fs::read_to_string(&path).expect("read");
        assert!(
            after.contains("# My feeds. Hand-written"),
            "header comment lost"
        );
        assert!(after.contains("# How many at once."), "comment lost");
        assert!(after.contains("# the good one"), "inline comment lost");
        assert!(after.contains("max_concurrent_fetches = 4"));
        assert!(after.contains(r#"tags = ["News", "Rust"]"#), "{after}");
    }

    #[test]
    fn moving_a_feed_leaves_the_other_feeds_alone() {
        let path = scratch("others.toml");
        fs::write(&path, COMMENTED).expect("write");
        set_feed_tags(&path, "https://a.example/feed", &["New".into()]).expect("set");

        let config = Config::load_from(&path).expect("parses");
        let beta = config
            .feeds
            .iter()
            .find(|feed| feed.url == "https://b.example/feed")
            .expect("beta");
        assert_eq!(beta.tags, ["Old"], "the other feed was rewritten");
    }

    #[test]
    fn moving_a_feed_to_the_top_level_removes_its_tags() {
        let path = scratch("toplevel.toml");
        fs::write(&path, COMMENTED).expect("write");
        set_feed_tags(&path, "https://b.example/feed", &[]).expect("set");

        let after = fs::read_to_string(&path).expect("read");
        assert!(!after.contains("tags ="), "tags survived: {after}");
        assert!(
            Config::load_from(&path).expect("parses").feeds[1]
                .tags
                .is_empty()
        );
    }

    #[test]
    fn the_result_still_parses_as_a_config() {
        let path = scratch("roundtrip.toml");
        fs::write(&path, COMMENTED).expect("write");
        set_feed_tags(&path, "https://a.example/feed", &["X".into()]).expect("set");

        let config = Config::load_from(&path).expect("the edited config must parse");
        assert_eq!(config.feeds.len(), 2);
        assert_eq!(config.fetch_limit(), 4);
    }

    #[test]
    fn adding_a_feed_keeps_the_comments() {
        let path = scratch("add.toml");
        fs::write(&path, COMMENTED).expect("write");

        add_feed(&path, "https://new.example/feed", Some("New One")).expect("add");

        let after = fs::read_to_string(&path).expect("read");
        assert!(after.contains("# My feeds. Hand-written"), "header lost");
        assert!(after.contains("# the good one"), "inline comment lost");

        let config = Config::load_from(&path).expect("parses");
        assert_eq!(config.feeds.len(), 3);
        let added = config.feeds.last().expect("the new one");
        assert_eq!(added.url, "https://new.example/feed");
        assert_eq!(added.title.as_deref(), Some("New One"));
    }

    #[test]
    fn adding_a_feed_already_there_is_refused() {
        let path = scratch("dupe.toml");
        fs::write(&path, COMMENTED).expect("write");
        let err = add_feed(&path, "https://a.example/feed", None).expect_err("refused");
        assert!(err.to_string().contains("already in the config"));
        assert_eq!(Config::load_from(&path).expect("parses").feeds.len(), 2);
    }

    #[test]
    fn adding_without_a_title_leaves_the_feed_to_name_itself() {
        let path = scratch("untitled.toml");
        fs::write(&path, COMMENTED).expect("write");
        add_feed(&path, "https://new.example/feed", None).expect("add");
        assert!(
            Config::load_from(&path).expect("parses").feeds[2]
                .title
                .is_none()
        );
    }

    #[test]
    fn adding_to_a_config_with_no_feeds_yet_works() {
        let path = scratch("empty.toml");
        fs::write(&path, "# Nothing here yet.\n").expect("write");
        add_feed(&path, "https://first.example/feed", None).expect("add");

        let after = fs::read_to_string(&path).expect("read");
        assert!(after.contains("# Nothing here yet."));
        assert_eq!(Config::load_from(&path).expect("parses").feeds.len(), 1);
    }

    #[test]
    fn a_url_that_is_not_in_the_config_is_an_error() {
        let path = scratch("missing.toml");
        fs::write(&path, COMMENTED).expect("write");
        let err =
            set_feed_tags(&path, "https://nope.example/feed", &["X".into()]).expect_err("rejected");
        assert!(err.to_string().contains("no feed in the config"));
    }

    #[test]
    fn writing_leaves_no_temporary_file_behind() {
        let path = scratch("clean.toml");
        fs::write(&path, COMMENTED).expect("write");
        set_feed_tags(&path, "https://a.example/feed", &["X".into()]).expect("set");
        assert!(!path.with_extension("toml.tmp").exists());
    }

    #[test]
    fn an_empty_config_is_valid() {
        let config: Config = toml::from_str("").expect("empty config should parse");
        assert!(config.feeds.is_empty());
    }
}
