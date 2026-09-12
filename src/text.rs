//! Laying text out for a fixed-width pane.

/// Wraps `text` to `width` columns, breaking on whitespace.
///
/// Returns the lines the pane should show. Blank lines in the input are kept,
/// because they are the only paragraph separation a summary has. A word longer
/// than the whole width is split rather than allowed to overflow — a bare URL
/// would otherwise run off the edge.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word;

            // A word that cannot fit on any line is chopped to width.
            while word.chars().count() > width {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let split = word
                    .char_indices()
                    .nth(width)
                    .map(|(i, _)| i)
                    .unwrap_or(word.len());
                let (head, rest) = word.split_at(split);
                lines.push(head.to_string());
                word = rest;
            }

            let needed = if line.is_empty() {
                word.chars().count()
            } else {
                line.chars().count() + 1 + word.chars().count()
            };
            if needed > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_line() {
        assert_eq!(wrap("hello there", 40), vec!["hello there"]);
    }

    #[test]
    fn text_breaks_on_whitespace_at_the_width() {
        assert_eq!(wrap("aaa bbb ccc", 7), vec!["aaa bbb", "ccc"]);
    }

    #[test]
    fn no_line_exceeds_the_width() {
        let text = "The quick brown fox jumps over the lazy dog and keeps running onwards";
        for width in [1usize, 5, 11, 23, 80] {
            for line in wrap(text, width) {
                assert!(
                    line.chars().count() <= width,
                    "line {line:?} exceeds width {width}"
                );
            }
        }
    }

    #[test]
    fn a_word_longer_than_the_width_is_split_not_overflowed() {
        let lines = wrap("https://example.com/a/very/long/path", 10);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.chars().count() <= 10));
        assert_eq!(
            lines.concat().replace(' ', ""),
            "https://example.com/a/very/long/path"
        );
    }

    #[test]
    fn blank_lines_survive_as_paragraph_breaks() {
        assert_eq!(wrap("one\n\ntwo", 20), vec!["one", "", "two"]);
    }

    #[test]
    fn a_zero_width_pane_produces_nothing_rather_than_looping() {
        assert!(wrap("anything at all", 0).is_empty());
    }

    #[test]
    fn empty_text_produces_a_single_blank_line() {
        assert_eq!(wrap("", 10), vec![""]);
    }

    #[test]
    fn multibyte_text_is_measured_in_characters_not_bytes() {
        // Eight characters, sixteen bytes.
        let lines = wrap("ααααα βββββ", 5);
        assert_eq!(lines, vec!["ααααα", "βββββ"]);
    }
}
