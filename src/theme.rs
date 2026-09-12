//! Colours, and the ability to not use any.

use anyhow::{Result, bail};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

/// Every colour the interface uses.
///
/// Named by role rather than by hue, so a theme can be swapped wholesale and
/// `ui` never has to know which palette it is drawing with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Focused borders, unread counts, the selected row.
    pub accent: Color,
    /// Dates, read entries, anything receding.
    pub dim: Color,
    /// Failed feeds.
    pub error: Color,
    /// Prompts that need an answer.
    pub warning: Color,
    /// Starred entries.
    pub star: Color,
    /// Feed group headings.
    pub group: Color,
    /// Links in the detail pane.
    pub link: Color,
    /// Text on the status bar.
    pub status_foreground: Color,
    /// Draw borders with ASCII rather than box-drawing characters.
    pub ascii: bool,
}

/// Whether this terminal can be trusted with box-drawing characters.
///
/// `TERM=dumb` says so outright. A non-UTF-8 locale is the other common case:
/// the characters would arrive as mojibake, which is worse than plain ASCII.
pub fn terminal_needs_ascii() -> bool {
    let term = std::env::var("TERM").unwrap_or_default();
    if term == "dumb" || term.is_empty() {
        return true;
    }
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .unwrap_or_default();
    // An unset locale is not evidence of anything; a set one that is not UTF-8
    // is.
    !locale.is_empty()
        && !locale.to_ascii_uppercase().contains("UTF-8")
        && !locale.to_ascii_uppercase().contains("UTF8")
}

/// What the config can say about colours.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ThemeConfig {
    /// A bundled preset: `dark`, `light` or `mono`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Force ASCII borders on or off. Unset means detect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascii: Option<bool>,
    #[serde(flatten, default)]
    pub overrides: std::collections::HashMap<String, String>,
}

impl Theme {
    /// Reasonable on a dark background, which is what most terminals are.
    pub fn dark() -> Self {
        Self {
            accent: Color::Cyan,
            dim: Color::DarkGray,
            error: Color::Red,
            warning: Color::Yellow,
            star: Color::Yellow,
            group: Color::Magenta,
            link: Color::Blue,
            status_foreground: Color::Black,
            ascii: false,
        }
    }

    /// For a light background, where bright cyan and yellow wash out.
    ///
    /// The darker variants of each hue, so text stays legible on white rather
    /// than merely being a different colour.
    pub fn light() -> Self {
        Self {
            accent: Color::Blue,
            dim: Color::Gray,
            error: Color::Red,
            warning: Color::Magenta,
            star: Color::Rgb(0xb5, 0x89, 0x00),
            group: Color::Rgb(0x6c, 0x35, 0x83),
            link: Color::Rgb(0x1a, 0x4f, 0xa0),
            status_foreground: Color::White,
            ascii: false,
        }
    }

    /// No colour at all: every role resets to the terminal's own foreground.
    ///
    /// Used for `NO_COLOR`, and for terminals or pipes where colour is noise.
    pub fn mono() -> Self {
        Self {
            accent: Color::Reset,
            dim: Color::Reset,
            error: Color::Reset,
            warning: Color::Reset,
            star: Color::Reset,
            group: Color::Reset,
            link: Color::Reset,
            status_foreground: Color::Reset,
            ascii: false,
        }
    }

    /// The theme a config asks for, with `NO_COLOR` overriding everything.
    pub fn resolve(config: &ThemeConfig, no_color: bool) -> Result<Self> {
        // https://no-color.org: honour it whatever the config says, because the
        // person setting it is stating a requirement, not a preference.
        if no_color {
            return Ok(Self {
                ascii: config.ascii.unwrap_or_else(terminal_needs_ascii),
                ..Self::mono()
            });
        }

        let mut theme = match config.name.as_deref() {
            None | Some("dark") => Self::dark(),
            Some("light") => Self::light(),
            Some("mono") | Some("none") => Self::mono(),
            Some(other) => bail!("unknown theme `{other}`. Try dark, light or mono."),
        };

        // An explicit setting wins; otherwise ask the terminal.
        theme.ascii = config.ascii.unwrap_or_else(terminal_needs_ascii);

        for (role, value) in &config.overrides {
            let colour =
                parse_colour(value).map_err(|err| anyhow::anyhow!("[theme].{role}: {err}"))?;
            match role.as_str() {
                "accent" => theme.accent = colour,
                "dim" => theme.dim = colour,
                "error" => theme.error = colour,
                "warning" => theme.warning = colour,
                "star" => theme.star = colour,
                "group" => theme.group = colour,
                "link" => theme.link = colour,
                "status_foreground" => theme.status_foreground = colour,
                other => bail!(
                    "unknown theme colour `{other}`. Known: accent, dim, error, \
                     warning, star, group, link, status_foreground."
                ),
            }
        }
        Ok(theme)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Reads a colour name or `#rrggbb`.
fn parse_colour(text: &str) -> Result<Color> {
    if let Some(hex) = text.strip_prefix('#') {
        if hex.len() != 6 {
            bail!("`{text}` is not a #rrggbb colour");
        }
        let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16);
        return match (channel(0), channel(2), channel(4)) {
            (Ok(r), Ok(g), Ok(b)) => Ok(Color::Rgb(r, g, b)),
            _ => bail!("`{text}` is not a #rrggbb colour"),
        };
    }
    Ok(match text.to_ascii_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "white" => Color::White,
        "reset" | "none" => Color::Reset,
        other => bail!("`{other}` is not a colour rsst knows"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: Option<&str>, overrides: &[(&str, &str)]) -> ThemeConfig {
        ThemeConfig {
            name: name.map(Into::into),
            ascii: Some(false),
            overrides: overrides
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }

    #[test]
    fn the_default_is_the_dark_preset() {
        assert_eq!(
            Theme::resolve(&config(None, &[]), false).unwrap(),
            Theme::dark()
        );
    }

    #[test]
    fn a_named_preset_changes_the_colours() {
        let light = Theme::resolve(&config(Some("light"), &[]), false).unwrap();
        assert_eq!(light, Theme::light());
        assert_ne!(light, Theme::dark(), "light is not just dark again");
    }

    #[test]
    fn no_color_wins_over_the_config() {
        // Someone setting NO_COLOR is stating a requirement, not a preference.
        let theme = Theme::resolve(&config(Some("light"), &[("accent", "red")]), true).unwrap();
        assert_eq!(theme, Theme::mono());
    }

    #[test]
    fn mono_resets_every_role_so_nothing_is_coloured() {
        let mono = Theme::mono();
        for colour in [
            mono.accent,
            mono.dim,
            mono.error,
            mono.warning,
            mono.star,
            mono.group,
            mono.link,
            mono.status_foreground,
        ] {
            assert_eq!(colour, Color::Reset);
        }
    }

    #[test]
    fn individual_roles_can_be_overridden() {
        let theme = Theme::resolve(&config(Some("dark"), &[("accent", "green")]), false).unwrap();
        assert_eq!(theme.accent, Color::Green);
        assert_eq!(theme.dim, Theme::dark().dim, "other roles are untouched");
    }

    #[test]
    fn hex_colours_are_accepted() {
        let theme = Theme::resolve(&config(None, &[("accent", "#1a4fa0")]), false).unwrap();
        assert_eq!(theme.accent, Color::Rgb(0x1a, 0x4f, 0xa0));
    }

    #[test]
    fn ascii_can_be_forced_either_way() {
        let mut c = config(None, &[]);
        c.ascii = Some(true);
        assert!(Theme::resolve(&c, false).unwrap().ascii);
        c.ascii = Some(false);
        assert!(!Theme::resolve(&c, false).unwrap().ascii);
    }

    #[test]
    fn no_color_still_honours_the_ascii_setting() {
        let mut c = config(None, &[]);
        c.ascii = Some(true);
        let theme = Theme::resolve(&c, true).unwrap();
        assert!(theme.ascii, "NO_COLOR is about colour, not box drawing");
        assert_eq!(theme.accent, Color::Reset);
    }

    #[test]
    fn an_unknown_theme_is_reported() {
        let err = Theme::resolve(&config(Some("solarised"), &[]), false).unwrap_err();
        assert!(err.to_string().contains("unknown theme `solarised`"));
    }

    #[test]
    fn an_unknown_role_is_reported() {
        let err = Theme::resolve(&config(None, &[("borders", "red")]), false).unwrap_err();
        assert!(err.to_string().contains("unknown theme colour `borders`"));
    }

    #[test]
    fn a_malformed_colour_names_the_role_it_came_from() {
        let err = Theme::resolve(&config(None, &[("accent", "#12")]), false).unwrap_err();
        assert!(err.to_string().contains("[theme].accent"));
    }

    #[test]
    fn the_two_colour_presets_differ_in_every_role_that_matters() {
        let (dark, light) = (Theme::dark(), Theme::light());
        // Legibility on opposite backgrounds means the accent, group, link and
        // status foreground cannot be shared.
        assert_ne!(dark.accent, light.accent);
        assert_ne!(dark.group, light.group);
        assert_ne!(dark.link, light.link);
        assert_ne!(dark.status_foreground, light.status_foreground);
    }
}
