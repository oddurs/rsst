//! Colours, and the ability to not use any.

use anyhow::{Result, bail};
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

/// How each part of the interface is styled.
///
/// A `Style` rather than a `Color` per role, because the most faithful way to
/// follow a terminal's theme is often not to name a colour at all — dimming the
/// foreground it already chose, or reversing it, keeps whatever contrast the
/// user set up. Naming a colour second-guesses a decision they already made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Focused borders, unread counts, the selected row.
    pub accent: Style,
    /// Dates, read entries, anything receding.
    pub dim: Style,
    /// Failed feeds.
    pub error: Style,
    /// Prompts that need an answer.
    pub warning: Style,
    /// Starred entries.
    pub star: Style,
    /// Feed group headings.
    pub group: Style,
    /// Links in the detail pane.
    pub link: Style,
    /// Draw borders with ASCII rather than box-drawing characters.
    pub ascii: bool,
}

/// What the config can say about colours.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ThemeConfig {
    /// A bundled preset: `terminal`, `dark`, `light` or `mono`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Force ASCII borders on or off. Unset means detect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascii: Option<bool>,
    #[serde(flatten, default)]
    pub overrides: std::collections::HashMap<String, String>,
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

impl Theme {
    /// The default: the terminal's own palette, and attributes for emphasis.
    ///
    /// Uses only the sixteen ANSI colours, which every terminal theme defines
    /// and which Ghostty, iTerm2 and the rest remap to their own palette — so
    /// rsst follows the theme rather than fighting it. `dim` names no colour at
    /// all: it dims whatever foreground the terminal is already using, which
    /// cannot vanish into the background the way a fixed grey can.
    pub fn terminal() -> Self {
        Self {
            accent: Style::new().fg(Color::Cyan),
            dim: Style::new().add_modifier(Modifier::DIM),
            error: Style::new().fg(Color::Red),
            warning: Style::new().fg(Color::Yellow),
            star: Style::new().fg(Color::Yellow),
            group: Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            link: Style::new()
                .fg(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
            ascii: false,
        }
    }

    /// The bright half of the palette, which reads better on a dark ground.
    pub fn dark() -> Self {
        Self {
            accent: Style::new().fg(Color::LightCyan),
            dim: Style::new().fg(Color::DarkGray),
            error: Style::new().fg(Color::LightRed),
            warning: Style::new().fg(Color::LightYellow),
            star: Style::new().fg(Color::LightYellow),
            group: Style::new()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
            link: Style::new()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::UNDERLINED),
            ascii: false,
        }
    }

    /// The dim half, which stays legible on white.
    ///
    /// Still the terminal's own palette rather than fixed RGB: a light theme
    /// has picked its own readable blue, and it is a better blue than one
    /// chosen here without knowing the background.
    pub fn light() -> Self {
        Self {
            accent: Style::new().fg(Color::Blue),
            dim: Style::new().fg(Color::Gray),
            error: Style::new().fg(Color::Red),
            warning: Style::new().fg(Color::Magenta),
            star: Style::new().fg(Color::Yellow),
            group: Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            link: Style::new()
                .fg(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
            ascii: false,
        }
    }

    /// No colour at all: emphasis comes from attributes only.
    ///
    /// Used for `NO_COLOR`. Bold, dim and underline are text attributes rather
    /// than colours, so they remain available to say what a thing is.
    pub fn mono() -> Self {
        Self {
            accent: Style::new().add_modifier(Modifier::BOLD),
            dim: Style::new().add_modifier(Modifier::DIM),
            error: Style::new().add_modifier(Modifier::BOLD),
            warning: Style::new().add_modifier(Modifier::BOLD),
            star: Style::new().add_modifier(Modifier::BOLD),
            group: Style::new().add_modifier(Modifier::BOLD),
            link: Style::new().add_modifier(Modifier::UNDERLINED),
            ascii: false,
        }
    }

    /// The style for a status bar carrying `tone`.
    ///
    /// Reversed rather than a foreground on a chosen background: reversing
    /// swaps in the terminal's own background as the text colour, so the bar
    /// contrasts by construction. Picking the text colour ourselves is what
    /// made it unreadable on themes whose cyan is dark.
    pub fn status(&self, tone: Style) -> Style {
        tone.add_modifier(Modifier::REVERSED)
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
            None | Some("terminal") | Some("auto") => Self::terminal(),
            Some("dark") => Self::dark(),
            Some("light") => Self::light(),
            Some("mono") | Some("none") => Self::mono(),
            Some(other) => {
                bail!("unknown theme `{other}`. Try terminal, dark, light or mono.")
            }
        };

        // An explicit setting wins; otherwise ask the terminal.
        theme.ascii = config.ascii.unwrap_or_else(terminal_needs_ascii);

        for (role, value) in &config.overrides {
            let style =
                parse_style(value).map_err(|err| anyhow::anyhow!("[theme].{role}: {err}"))?;
            match role.as_str() {
                "accent" => theme.accent = style,
                "dim" => theme.dim = style,
                "error" => theme.error = style,
                "warning" => theme.warning = style,
                "star" => theme.star = style,
                "group" => theme.group = style,
                "link" => theme.link = style,
                other => bail!(
                    "unknown theme role `{other}`. Known: accent, dim, error, \
                     warning, star, group, link."
                ),
            }
        }
        Ok(theme)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::terminal()
    }
}

/// Reads a role's styling: a colour, attributes, or both.
///
/// `cyan`, `#1a4fa0`, `bold`, `dim`, `underline`, or any combination separated
/// by spaces — `bold cyan`. Attributes alone name no colour, which is how a
/// role follows the terminal's own foreground.
fn parse_style(text: &str) -> Result<Style> {
    let mut style = Style::new();
    let mut named_colour = false;

    for word in text.split_whitespace() {
        match word.to_ascii_lowercase().as_str() {
            "bold" => style = style.add_modifier(Modifier::BOLD),
            "dim" | "faint" => style = style.add_modifier(Modifier::DIM),
            "italic" => style = style.add_modifier(Modifier::ITALIC),
            "underline" | "underlined" => style = style.add_modifier(Modifier::UNDERLINED),
            "reverse" | "reversed" => style = style.add_modifier(Modifier::REVERSED),
            _ => {
                style = style.fg(parse_colour(word)?);
                named_colour = true;
            }
        }
    }

    if text.trim().is_empty() {
        bail!("names nothing");
    }
    // Attributes with no colour are deliberate, not an oversight.
    let _ = named_colour;
    Ok(style)
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
        // The bright half of the palette, which terminals theme separately.
        "lightred" | "brightred" => Color::LightRed,
        "lightgreen" | "brightgreen" => Color::LightGreen,
        "lightyellow" | "brightyellow" => Color::LightYellow,
        "lightblue" | "brightblue" => Color::LightBlue,
        "lightmagenta" | "brightmagenta" => Color::LightMagenta,
        "lightcyan" | "brightcyan" => Color::LightCyan,
        // The terminal's own foreground, whatever it is.
        "reset" | "none" | "default" | "terminal" => Color::Reset,
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

    fn roles(theme: &Theme) -> [Style; 7] {
        [
            theme.accent,
            theme.dim,
            theme.error,
            theme.warning,
            theme.star,
            theme.group,
            theme.link,
        ]
    }

    #[test]
    fn the_default_follows_the_terminal() {
        assert_eq!(
            Theme::resolve(&config(None, &[]), false).unwrap(),
            Theme::terminal()
        );
    }

    #[test]
    fn no_preset_uses_a_colour_the_terminal_cannot_theme() {
        // The sixteen ANSI colours are remapped by every terminal theme;
        // an RGB value is not, which is the whole bug.
        for theme in [
            Theme::terminal(),
            Theme::dark(),
            Theme::light(),
            Theme::mono(),
        ] {
            for style in roles(&theme) {
                if let Some(Color::Rgb(..) | Color::Indexed(_)) = style.fg {
                    panic!("a preset pinned an absolute colour: {style:?}");
                }
            }
        }
    }

    #[test]
    fn the_status_bar_reverses_rather_than_naming_a_text_colour() {
        // Reversing swaps in the terminal's own background as the text colour,
        // so the bar contrasts whatever the theme. Naming the text colour is
        // what made it unreadable on themes with a dark cyan.
        let theme = Theme::terminal();
        let status = theme.status(theme.accent);
        assert!(status.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(status.bg, None, "the status bar pinned a background");
    }

    #[test]
    fn dim_names_no_colour_so_it_cannot_vanish() {
        // A fixed grey disappears on any theme whose background is near it.
        // Dimming the terminal's own foreground cannot.
        let dim = Theme::terminal().dim;
        assert_eq!(dim.fg, None);
        assert!(dim.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn a_named_preset_changes_the_styling() {
        let light = Theme::resolve(&config(Some("light"), &[]), false).unwrap();
        assert_eq!(light, Theme::light());
        assert_ne!(light, Theme::dark(), "light is not just dark again");
    }

    #[test]
    fn dark_and_light_differ_in_the_roles_that_matter() {
        let (dark, light) = (Theme::dark(), Theme::light());
        assert_ne!(dark.accent, light.accent);
        assert_ne!(dark.dim, light.dim);
        assert_ne!(dark.error, light.error);
    }

    #[test]
    fn no_color_wins_over_the_config() {
        let theme = Theme::resolve(&config(Some("light"), &[("accent", "red")]), true).unwrap();
        assert_eq!(roles(&theme), roles(&Theme::mono()), "NO_COLOR did not win");
    }

    #[test]
    fn mono_names_no_colour_but_keeps_attributes() {
        let mono = Theme::mono();
        for style in roles(&mono) {
            assert_eq!(style.fg, None, "mono named a colour");
            assert!(
                !style.add_modifier.is_empty(),
                "mono gave up on saying what a thing is"
            );
        }
    }

    #[test]
    fn no_color_still_honours_the_ascii_setting() {
        let mut c = config(None, &[]);
        c.ascii = Some(true);
        let theme = Theme::resolve(&c, true).unwrap();
        assert!(theme.ascii, "NO_COLOR is about colour, not box drawing");
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
    fn a_role_can_be_overridden_with_a_colour() {
        let theme = Theme::resolve(&config(Some("dark"), &[("accent", "green")]), false).unwrap();
        assert_eq!(theme.accent.fg, Some(Color::Green));
        assert_eq!(theme.dim, Theme::dark().dim, "other roles are untouched");
    }

    #[test]
    fn a_role_can_be_overridden_with_attributes_alone() {
        // Naming no colour is how a role follows the terminal's foreground.
        let theme = Theme::resolve(&config(None, &[("star", "bold underline")]), false).unwrap();
        assert_eq!(theme.star.fg, None);
        assert!(theme.star.add_modifier.contains(Modifier::BOLD));
        assert!(theme.star.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn a_role_can_combine_an_attribute_and_a_colour() {
        let theme = Theme::resolve(&config(None, &[("group", "bold magenta")]), false).unwrap();
        assert_eq!(theme.group.fg, Some(Color::Magenta));
        assert!(theme.group.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn default_means_the_terminals_own_foreground() {
        let theme = Theme::resolve(&config(None, &[("link", "default")]), false).unwrap();
        assert_eq!(theme.link.fg, Some(Color::Reset));
    }

    #[test]
    fn the_bright_half_of_the_palette_is_addressable() {
        let theme = Theme::resolve(&config(None, &[("accent", "lightcyan")]), false).unwrap();
        assert_eq!(theme.accent.fg, Some(Color::LightCyan));
    }

    #[test]
    fn hex_colours_are_still_accepted_for_people_who_want_them() {
        let theme = Theme::resolve(&config(None, &[("accent", "#1a4fa0")]), false).unwrap();
        assert_eq!(theme.accent.fg, Some(Color::Rgb(0x1a, 0x4f, 0xa0)));
    }

    #[test]
    fn an_unknown_theme_is_reported() {
        let err = Theme::resolve(&config(Some("solarised"), &[]), false).unwrap_err();
        assert!(err.to_string().contains("unknown theme `solarised`"));
        assert!(err.to_string().contains("terminal"), "names the default");
    }

    #[test]
    fn an_unknown_role_is_reported() {
        let err = Theme::resolve(&config(None, &[("borders", "red")]), false).unwrap_err();
        assert!(err.to_string().contains("unknown theme role `borders`"));
    }

    #[test]
    fn a_malformed_value_names_the_role_it_came_from() {
        let err = Theme::resolve(&config(None, &[("accent", "#12")]), false).unwrap_err();
        assert!(err.to_string().contains("[theme].accent"));
    }

    #[test]
    fn an_empty_value_is_rejected() {
        assert!(parse_style("   ").is_err());
    }
}
