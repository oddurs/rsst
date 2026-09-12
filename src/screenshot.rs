//! Rendering one frame to SVG.
//!
//! Draws through the same `ui::draw` the reader uses, so a screenshot cannot
//! drift from the interface: there is no second code path to keep in step. SVG
//! rather than PNG because it is text — a few kilobytes that diff, compress and
//! clone like source, instead of a binary blob in the repository forever.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use crate::app::App;
use crate::keys::Keymap;

const CELL_WIDTH: f32 = 8.4;
const CELL_HEIGHT: f32 = 18.0;

/// Renders `app` at the given size and returns an SVG document.
pub fn svg(app: &mut App, keymap: &Keymap, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height))
        .expect("the test backend cannot fail to build");
    terminal
        .draw(|frame| crate::ui::draw(frame, app, keymap))
        .expect("drawing to a buffer cannot fail");
    let buffer = terminal.backend().buffer().clone();

    let w = f32::from(width) * CELL_WIDTH;
    let h = f32::from(height) * CELL_HEIGHT;
    let background = if app.theme.ascii {
        "#1c1c1c"
    } else {
        "#12151a"
    };

    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" \
         width=\"{w:.0}\" height=\"{h:.0}\" font-family=\"ui-monospace, SFMono-Regular, \
         Menlo, Consolas, monospace\" font-size=\"14\">\n\
         <rect width=\"100%\" height=\"100%\" rx=\"8\" fill=\"{background}\"/>\n"
    );

    for y in 0..height {
        // One <text> per row, with runs of identical styling merged, so the
        // file stays small rather than one element per cell.
        let mut runs: Vec<(String, Color, bool)> = Vec::new();
        for x in 0..width {
            let cell = &buffer[(x, y)];
            let bold = cell.modifier.contains(ratatui::style::Modifier::BOLD);
            match runs.last_mut() {
                Some((text, colour, was_bold)) if *colour == cell.fg && *was_bold == bold => {
                    text.push_str(cell.symbol());
                }
                _ => runs.push((cell.symbol().to_string(), cell.fg, bold)),
            }
        }

        let mut column = 0usize;
        for (text, colour, bold) in runs {
            let length = text.chars().count();
            if !text.trim().is_empty() {
                let x = column as f32 * CELL_WIDTH;
                let baseline = f32::from(y) * CELL_HEIGHT + CELL_HEIGHT * 0.75;
                let weight = if bold { " font-weight=\"bold\"" } else { "" };
                out.push_str(&format!(
                    "<text x=\"{x:.1}\" y=\"{baseline:.1}\" fill=\"{}\"{weight} \
                     xml:space=\"preserve\">{}</text>\n",
                    hex(colour),
                    escape(&text)
                ));
            }
            column += length;
        }
    }

    out.push_str("</svg>\n");
    out
}

/// A terminal colour as SVG hex.
///
/// The named colours are given the palette a reader would actually see rather
/// than the pure primaries, which are unreadably bright on a dark ground.
fn hex(colour: Color) -> String {
    match colour {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => "#1c1c1c".into(),
        Color::Red => "#e06c75".into(),
        Color::Green => "#98c379".into(),
        Color::Yellow => "#e5c07b".into(),
        Color::Blue => "#61afef".into(),
        Color::Magenta => "#c678dd".into(),
        Color::Cyan => "#56b6c2".into(),
        Color::Gray => "#abb2bf".into(),
        Color::DarkGray => "#5c6370".into(),
        Color::White => "#f0f0f0".into(),
        // Reset is the terminal's own foreground, which for a dark ground is
        // the ordinary text colour.
        _ => "#c8ccd4".into(),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Reads a `WIDTHxHEIGHT` argument.
pub fn parse_size(text: &str) -> Option<(u16, u16)> {
    let (w, h) = text.split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::{Entry, Feed, Status};
    use crate::state::ReadState;

    fn app() -> App {
        App::new(
            vec![Feed {
                title: "Rust Blog".into(),
                url: "https://blog.rust-lang.org/feed.xml".into(),
                status: Status::Idle,
                entries: vec![Entry {
                    title: "Announcing Rust 1.0".into(),
                    link: Some("https://blog.rust-lang.org/2015/05/15/Rust-1.0.html".into()),
                    published: None,
                    summary: "Today we are very proud to announce the 1.0 release.".into(),
                    keys: vec!["id:a".into()],
                }],
            }],
            ReadState::default(),
        )
    }

    #[test]
    fn sizes_parse_and_nonsense_does_not() {
        assert_eq!(parse_size("100x30"), Some((100, 30)));
        assert_eq!(parse_size("80X24"), Some((80, 24)));
        assert_eq!(parse_size("wide"), None);
        assert_eq!(parse_size("100"), None);
    }

    #[test]
    fn the_svg_is_well_formed_and_shows_the_real_interface() {
        let svg = svg(&mut app(), &Keymap::default(), 100, 24);
        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        // Drawn through ui::draw, so the actual content is present.
        assert!(svg.contains("Rust Blog"));
        assert!(svg.contains("Announcing Rust 1.0"));
    }

    #[test]
    fn markup_in_an_entry_cannot_break_the_svg() {
        let mut app = app();
        app.feeds[0].entries[0].title = "a < b & c > d".into();
        let svg = svg(&mut app, &Keymap::default(), 100, 24);
        assert!(svg.contains("&lt;"), "angle brackets are escaped");
        assert!(svg.contains("&amp;"), "ampersands are escaped");
        assert!(
            !svg.contains("a < b"),
            "raw markup leaked into the document"
        );
    }

    #[test]
    fn a_screenshot_is_small_enough_to_commit() {
        let svg = svg(&mut app(), &Keymap::default(), 100, 30);
        assert!(
            svg.len() < 64 * 1024,
            "a screenshot grew to {} bytes",
            svg.len()
        );
    }
}
