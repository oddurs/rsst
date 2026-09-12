use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// On-disk configuration, loaded from `$XDG_CONFIG_HOME/rsst/config.toml`.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub feeds: Vec<FeedSource>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedSource {
    pub url: String,
    /// Overrides the title advertised by the feed itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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

    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parsing config at {}", path.display()))
    }

    fn starter() -> Self {
        Self {
            feeds: vec![FeedSource {
                url: "https://blog.rust-lang.org/feed.xml".into(),
                title: Some("Rust Blog".into()),
            }],
        }
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
    fn an_empty_config_is_valid() {
        let config: Config = toml::from_str("").expect("empty config should parse");
        assert!(config.feeds.is_empty());
    }
}
