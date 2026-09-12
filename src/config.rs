use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// On-disk configuration, loaded from `$XDG_CONFIG_HOME/rsst/config.toml`.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub feeds: Vec<FeedSource>,
    /// Colours.
    #[serde(default)]
    pub theme: crate::theme::ThemeConfig,
    /// Key overrides: action name to key, merged over the defaults.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub keys: std::collections::HashMap<String, String>,
    /// How many feeds may be fetched at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_fetches: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedSource {
    pub url: String,
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

    /// Simultaneous fetches allowed, falling back to the built-in default.
    pub fn fetch_limit(&self) -> usize {
        self.max_concurrent_fetches
            .unwrap_or(crate::limit::DEFAULT_LIMIT)
    }

    fn starter() -> Self {
        Self {
            theme: Default::default(),
            keys: Default::default(),
            max_concurrent_fetches: None,
            feeds: vec![FeedSource {
                url: "https://blog.rust-lang.org/feed.xml".into(),
                title: Some("Rust Blog".into()),
                tags: Vec::new(),
            }],
        }
    }
}

/// The config path actually in use: an explicit one, or the platform default.
pub fn config_path_or(override_path: Option<PathBuf>) -> Result<PathBuf> {
    match override_path {
        Some(path) => Ok(path),
        None => config_path(),
    }
}

pub fn config_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "rsst")
        .context("could not determine a config directory for this platform")?;
    Ok(dirs.config_dir().join("config.toml"))
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

    #[test]
    fn an_empty_config_is_valid() {
        let config: Config = toml::from_str("").expect("empty config should parse");
        assert!(config.feeds.is_empty());
    }
}
