//! Turning an entry's HTML into something readable in a terminal.
//!
//! The detail pane used to show `strip_html`, which deleted every tag and
//! collapsed whitespace — fine for fitting a summary on one row of a list, and
//! wrong for a reading surface. A code sample arrived as prose, a list as a
//! run-on sentence, and a link as text with the address thrown away.
//!
//! Parsing is deliberately tolerant and hand-rolled rather than a real HTML
//! stack. Feeds are full of unclosed tags, invented elements and markup pasted
//! out of word processors; the job is to make a decent guess at every one of
//! them and never to fail.

use crate::text::wrap;

/// A run of text inside a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Strong(String),
    Emphasis(String),
    Code(String),
    /// Link text, and its number in the reference list at the end.
    Link(String, usize),
}

impl Inline {
    pub fn text(&self) -> &str {
        match self {
            Self::Text(t) | Self::Strong(t) | Self::Emphasis(t) | Self::Code(t) => t,
            Self::Link(t, _) => t,
        }
    }
}

/// A piece of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    /// Preformatted text, kept exactly as written.
    Code(Vec<String>),
    /// An item of a list. `ordinal` is its number in an ordered list, and
    /// `None` in a bulleted one — which glyph that becomes is a question for
    /// the layout, which knows what the terminal can draw.
    Item {
        ordinal: Option<usize>,
        body: Vec<Inline>,
    },
    Quote(Vec<Inline>),
    Rule,
}

/// A laid-out line, ready for the renderer to style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    pub indent: usize,
    pub spans: Vec<Inline>,
}

/// What a line is, so the renderer can style it without parsing it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Body,
    Heading,
    Code,
    Quote,
    Blank,
    Rule,
    /// A line of the reference list at the end.
    Reference,
}

/// A parsed article: its blocks, and the links it referred to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Article {
    pub blocks: Vec<Block>,
    /// Link targets, in the order they were numbered.
    pub links: Vec<String>,
}

/// Reads an entry's HTML.
pub fn parse(html: &str) -> Article {
    let mut article = Article::default();
    let mut inlines: Vec<Inline> = Vec::new();
    let mut text = String::new();

    // What we are inside of. A stack, because feeds nest these arbitrarily and
    // often forget to close them.
    let mut strong = 0usize;
    let mut emphasis = 0usize;
    let mut code = 0usize;
    let mut quote = 0usize;
    let mut link: Option<String> = None;
    let mut heading: Option<u8> = None;
    let mut pre: Option<String> = None;
    let mut ordered: Vec<Option<usize>> = Vec::new();
    // A space between two inline elements is real content; flushing the text
    // buffer must not swallow it.
    let mut pending_space = false;

    // Flushed before every change of inline state, not only at block
    // boundaries — otherwise the whole paragraph is attributed to whatever
    // happened to be open when the block ended, and `<code>` inside a sentence
    // styles the sentence.
    macro_rules! flush_text {
        () => {{
            if !text.is_empty() {
                let taken = std::mem::take(&mut text);
                inlines.push(if code > 0 {
                    Inline::Code(taken)
                } else if strong > 0 {
                    Inline::Strong(taken)
                } else if emphasis > 0 {
                    Inline::Emphasis(taken)
                } else {
                    Inline::Text(taken)
                });
            }
        }};
    }

    macro_rules! flush_block {
        () => {{
            flush_text!();
            let taken = std::mem::take(&mut inlines);
            if taken.iter().any(|inline| !inline.text().trim().is_empty()) {
                article.blocks.push(match (heading, quote > 0) {
                    (Some(level), _) => Block::Heading(level, taken),
                    (None, true) => Block::Quote(taken),
                    (None, false) => Block::Paragraph(taken),
                });
            }
        }};
    }

    for token in tokens(html) {
        match token {
            Token::Text(chunk) => {
                if let Some(buffer) = &mut pre {
                    buffer.push_str(&chunk);
                } else {
                    // Whitespace between tags is layout, not content — but a
                    // space between two inline elements is a real space, and
                    // flushing must not swallow it.
                    for ch in chunk.chars() {
                        if ch.is_whitespace() {
                            if !text.is_empty() || !inlines.is_empty() {
                                pending_space = true;
                            }
                        } else {
                            if pending_space && !text.is_empty() {
                                text.push(' ');
                            } else if pending_space {
                                inlines.push(Inline::Text(" ".into()));
                            }
                            pending_space = false;
                            text.push(ch);
                        }
                    }
                }
            }
            Token::Open(name, attributes) => match name.as_str() {
                "p" | "div" | "section" | "article" => flush_block!(),
                "br" => {
                    flush_block!();
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    flush_block!();
                    heading = name[1..].parse().ok();
                }
                "pre" => {
                    flush_block!();
                    pre = Some(String::new());
                }
                "code" if pre.is_none() => {
                    flush_text!();
                    code += 1;
                }
                "strong" | "b" => {
                    flush_text!();
                    strong += 1;
                }
                "em" | "i" => {
                    flush_text!();
                    emphasis += 1;
                }
                "blockquote" => {
                    flush_block!();
                    quote += 1;
                }
                "ul" => {
                    flush_block!();
                    ordered.push(None);
                }
                "ol" => {
                    flush_block!();
                    ordered.push(Some(1));
                }
                "li" => {
                    flush_block!();
                    let ordinal = match ordered.last_mut() {
                        Some(Some(n)) => {
                            let ordinal = *n;
                            *n += 1;
                            Some(ordinal)
                        }
                        _ => None,
                    };
                    // Items are collected as paragraphs, then relabelled when
                    // the item closes — which is the only point the whole body
                    // of the item is known.
                    inlines.push(Inline::Text(String::new()));
                    article.blocks.push(Block::Item {
                        ordinal,
                        body: Vec::new(),
                    });
                    inlines.clear();
                }
                "a" => {
                    flush_text!();
                    let href = attributes
                        .iter()
                        .find(|(key, _)| key == "href")
                        .map(|(_, value)| value.clone())
                        .unwrap_or_default();
                    if !href.is_empty() {
                        link = Some(href);
                    }
                }
                "hr" => {
                    flush_block!();
                    article.blocks.push(Block::Rule);
                }
                _ => {}
            },
            Token::Close(name) => match name.as_str() {
                "p" | "div" | "section" | "article" => flush_block!(),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    flush_block!();
                    heading = None;
                }
                "pre" => {
                    if let Some(buffer) = pre.take() {
                        let lines: Vec<String> = buffer
                            .trim_matches('\n')
                            .lines()
                            .map(|line| line.trim_end().to_string())
                            .collect();
                        if !lines.is_empty() {
                            article.blocks.push(Block::Code(lines));
                        }
                    }
                }
                "code" if pre.is_none() => {
                    flush_text!();
                    code = code.saturating_sub(1);
                }
                "strong" | "b" => {
                    flush_text!();
                    strong = strong.saturating_sub(1);
                }
                "em" | "i" => {
                    flush_text!();
                    emphasis = emphasis.saturating_sub(1);
                }
                "blockquote" => {
                    flush_block!();
                    quote = quote.saturating_sub(1);
                }
                "ul" | "ol" => {
                    flush_block!();
                    ordered.pop();
                }
                "li" => {
                    flush_text!();
                    let body = std::mem::take(&mut inlines);
                    if let Some(Block::Item { body: slot, .. }) =
                        article.blocks.iter_mut().rev().find(
                            |block| matches!(block, Block::Item { body, .. } if body.is_empty()),
                        )
                    {
                        *slot = body;
                    }
                }
                "a" => {
                    if let Some(href) = link.take() {
                        // Numbered only if it had visible text: a bare anchor
                        // or a tracking pixel would otherwise fill the
                        // reference list with entries nothing points at.
                        if text.trim().is_empty() {
                            text.clear();
                        } else {
                            let index = article.links.len();
                            article.links.push(href);
                            inlines.push(Inline::Link(std::mem::take(&mut text), index));
                        }
                    } else {
                        flush_text!();
                    }
                }
                _ => {}
            },
        }
    }
    flush_block!();

    // An article that is one bare run of text still has to be shown.
    if article.blocks.is_empty() && !html.trim().is_empty() {
        let plain = crate::feed::to_plain_text(html);
        if !plain.is_empty() {
            article
                .blocks
                .push(Block::Paragraph(vec![Inline::Text(plain)]));
        }
    }
    article
}

/// Appends `chunk` to `text`, collapsing runs of whitespace to one space.
/// Lays an article out for a pane of the given width.
///
/// `ascii` decides the glyphs, not the structure — a terminal that cannot draw
/// a bullet still has lists.
pub fn layout(article: &Article, width: usize, ascii: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    if width == 0 {
        return rows;
    }

    // A leading blank would push the article down by a row for nothing; one
    // between blocks is what separates them.
    let blank = |rows: &mut Vec<Row>| {
        if !rows.is_empty() {
            rows.push(Row {
                kind: Kind::Blank,
                indent: 0,
                spans: Vec::new(),
            });
        }
    };

    for block in &article.blocks {
        match block {
            Block::Paragraph(spans) => {
                blank(&mut rows);
                rows.extend(wrap_spans(spans, width, 0, Kind::Body));
            }
            Block::Heading(_, spans) => {
                blank(&mut rows);
                rows.extend(wrap_spans(spans, width, 0, Kind::Heading));
            }
            Block::Quote(spans) => {
                blank(&mut rows);
                // Indented rather than prefixed here; the renderer draws the
                // rule, because only it knows what the terminal can draw.
                rows.extend(wrap_spans(spans, width.saturating_sub(2), 2, Kind::Quote));
            }
            Block::Item { ordinal, body } => {
                let marker = match ordinal {
                    Some(n) => format!("{n}. "),
                    None if ascii => "* ".to_string(),
                    None => "• ".to_string(),
                };
                let indent = marker.chars().count();
                let mut lines = wrap_spans(body, width.saturating_sub(indent), indent, Kind::Body);
                // The marker replaces the first line's indent, so the rest of
                // the item hangs beneath the text rather than the bullet.
                if let Some(first) = lines.first_mut() {
                    first.indent = 0;
                    first.spans.insert(0, Inline::Text(marker));
                }
                rows.extend(lines);
            }
            Block::Code(lines) => {
                blank(&mut rows);
                for line in lines {
                    // Never reflowed: the line breaks are the content. A long
                    // line is cut rather than wrapped, because a wrapped code
                    // sample is a wrong code sample.
                    let text: String = line.chars().take(width.saturating_sub(2)).collect();
                    rows.push(Row {
                        kind: Kind::Code,
                        indent: 2,
                        spans: vec![Inline::Code(text)],
                    });
                }
            }
            Block::Rule => {
                blank(&mut rows);
                rows.push(Row {
                    kind: Kind::Rule,
                    indent: 0,
                    spans: Vec::new(),
                });
            }
        }
    }

    if !article.links.is_empty() {
        blank(&mut rows);
        for (index, href) in article.links.iter().enumerate() {
            let label = format!("[{}] ", index + 1);
            let indent = label.chars().count();
            for (line, text) in wrap(href, width.saturating_sub(indent))
                .into_iter()
                .enumerate()
            {
                rows.push(Row {
                    kind: Kind::Reference,
                    indent: if line == 0 { 0 } else { indent },
                    spans: if line == 0 {
                        vec![Inline::Text(label.clone()), Inline::Text(text)]
                    } else {
                        vec![Inline::Text(text)]
                    },
                });
            }
        }
    }

    rows
}

/// Wraps a run of styled text, keeping the styling across line breaks.
fn wrap_spans(spans: &[Inline], width: usize, indent: usize, kind: Kind) -> Vec<Row> {
    if width == 0 {
        return Vec::new();
    }

    // Wrap the plain text first, then walk the spans alongside it, so the
    // measure is right and the styling survives.
    let plain: String = spans
        .iter()
        .map(|span| match span {
            Inline::Link(text, index) => format!("{text}[{}]", index + 1),
            other => other.text().to_string(),
        })
        .collect();

    let mut styled: Vec<(char, Option<usize>)> = Vec::new();
    for (position, span) in spans.iter().enumerate() {
        let rendered = match span {
            Inline::Link(text, index) => format!("{text}[{}]", index + 1),
            other => other.text().to_string(),
        };
        for ch in rendered.chars() {
            styled.push((ch, Some(position)));
        }
    }

    let mut cursor = 0usize;
    let mut rows = Vec::new();
    for line in wrap(&plain, width) {
        let mut out: Vec<Inline> = Vec::new();
        for ch in line.chars() {
            // Skip whatever the wrapper dropped between lines.
            while cursor < styled.len() && styled[cursor].0 != ch {
                cursor += 1;
            }
            let owner = styled.get(cursor).and_then(|(_, owner)| *owner);
            cursor += 1;
            match (out.last_mut(), owner.and_then(|index| spans.get(index))) {
                (Some(last), Some(span)) if same_kind(last, span) => push_char(last, ch),
                (_, Some(span)) => out.push(empty_like(span, ch)),
                (_, None) => out.push(Inline::Text(ch.to_string())),
            }
        }
        rows.push(Row {
            kind,
            indent,
            spans: out,
        });
    }
    rows
}

fn same_kind(a: &Inline, b: &Inline) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
        && match (a, b) {
            (Inline::Link(_, x), Inline::Link(_, y)) => x == y,
            _ => true,
        }
}

fn push_char(span: &mut Inline, ch: char) {
    match span {
        Inline::Text(t) | Inline::Strong(t) | Inline::Emphasis(t) | Inline::Code(t) => t.push(ch),
        Inline::Link(t, _) => t.push(ch),
    }
}

fn empty_like(span: &Inline, ch: char) -> Inline {
    let text = ch.to_string();
    match span {
        Inline::Text(_) => Inline::Text(text),
        Inline::Strong(_) => Inline::Strong(text),
        Inline::Emphasis(_) => Inline::Emphasis(text),
        Inline::Code(_) => Inline::Code(text),
        Inline::Link(_, index) => Inline::Link(text, *index),
    }
}

/// A tag or a run of text.
enum Token {
    Open(String, Vec<(String, String)>),
    Close(String),
    Text(String),
}

/// Splits HTML into tags and text, tolerating anything.
///
/// An unterminated tag at the end of the input is dropped rather than treated
/// as text — a truncated feed should lose its last tag, not spray markup into
/// the article.
fn tokens(html: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let chars: Vec<char> = html.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '<' {
            let Some(end) = (index + 1..chars.len()).find(|i| chars[*i] == '>') else {
                break;
            };
            let raw: String = chars[index + 1..end].iter().collect();
            index = end + 1;

            // Comments, doctypes and processing instructions are not content.
            if raw.starts_with('!') || raw.starts_with('?') {
                continue;
            }
            let closing = raw.starts_with('/');
            let raw = raw.trim_start_matches('/').trim_end_matches('/');
            let mut parts = raw.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or("").to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            if closing {
                out.push(Token::Close(name));
            } else {
                out.push(Token::Open(name, attributes(parts.next().unwrap_or(""))));
            }
        } else {
            let start = index;
            while index < chars.len() && chars[index] != '<' {
                index += 1;
            }
            let raw: String = chars[start..index].iter().collect();
            out.push(Token::Text(crate::feed::decode_entities(&raw)));
        }
    }
    out
}

/// Reads a tag's attributes, in any of the spellings feeds use.
fn attributes(raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        let start = index;
        while index < chars.len() && chars[index] != '=' && !chars[index].is_whitespace() {
            index += 1;
        }
        if start == index {
            break;
        }
        let key: String = chars[start..index]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();

        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() || chars[index] != '=' {
            out.push((key, String::new()));
            continue;
        }
        index += 1;
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }

        let value: String = if index < chars.len() && (chars[index] == '"' || chars[index] == '\'')
        {
            let quote = chars[index];
            index += 1;
            let start = index;
            while index < chars.len() && chars[index] != quote {
                index += 1;
            }
            let value = chars[start..index].iter().collect();
            index += 1;
            value
        } else {
            let start = index;
            while index < chars.len() && !chars[index].is_whitespace() {
                index += 1;
            }
            chars[start..index].iter().collect()
        };
        out.push((key, crate::feed::decode_entities(&value)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                let body: String = row.spans.iter().map(Inline::text).collect();
                format!("{}{}", " ".repeat(row.indent), body)
            })
            .collect()
    }

    fn render(html: &str, width: usize) -> Vec<String> {
        text_of(&layout(&parse(html), width, false))
    }

    #[test]
    fn paragraphs_are_separated_rather_than_run_together() {
        let rows = render("<p>First para.</p><p>Second para.</p>", 40);
        assert_eq!(rows, ["First para.", "", "Second para."]);
    }

    #[test]
    fn a_code_block_keeps_its_own_line_breaks_and_indentation() {
        // The whole point of a code block: its line breaks are the content.
        let html = "<p>Try:</p><pre><code>fn main() {\n    let x = 1;\n}</code></pre>";
        let rows = render(html, 60);
        assert_eq!(
            rows,
            ["Try:", "", "  fn main() {", "      let x = 1;", "  }"]
        );
    }

    #[test]
    fn a_long_code_line_is_cut_rather_than_reflowed() {
        // A wrapped code sample is a wrong code sample.
        let long = "x".repeat(100);
        let rows = render(&format!("<pre>{long}</pre>"), 20);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].trim().len() <= 18);
    }

    #[test]
    fn list_items_are_bulleted_with_a_hanging_indent() {
        let html = "<ul><li>A short one</li><li>An item long enough that it has to wrap onto another line</li></ul>";
        let rows = render(html, 30);
        assert_eq!(rows[0], "• A short one");
        assert!(rows[1].starts_with("• An item"));
        // The continuation hangs under the text, not under the bullet.
        assert!(rows[2].starts_with("  "), "{:?}", rows[2]);
        assert!(!rows[2].trim_start().starts_with('•'));
    }

    #[test]
    fn a_terminal_without_a_bullet_still_gets_a_list() {
        let rows = text_of(&layout(&parse("<ul><li>One</li></ul>"), 40, true));
        assert_eq!(rows, ["* One"]);
        let rows = text_of(&layout(&parse("<ul><li>One</li></ul>"), 40, false));
        assert_eq!(rows, ["• One"]);
    }

    #[test]
    fn ordered_lists_are_numbered_and_count_up() {
        let rows = render("<ol><li>First</li><li>Second</li><li>Third</li></ol>", 40);
        assert_eq!(rows, ["1. First", "2. Second", "3. Third"]);
    }

    #[test]
    fn a_quote_is_marked_out_from_body_text() {
        let rows = layout(
            &parse("<p>Before</p><blockquote>Quoted.</blockquote>"),
            40,
            false,
        );
        let quote = rows
            .iter()
            .find(|row| row.kind == Kind::Quote)
            .expect("a quote");
        assert_eq!(
            quote.spans.iter().map(Inline::text).collect::<String>(),
            "Quoted."
        );
        assert_eq!(quote.indent, 2);
    }

    #[test]
    fn headings_are_distinct_from_body() {
        let rows = layout(&parse("<h2>A heading</h2><p>Body.</p>"), 40, false);
        assert!(rows.iter().any(|row| row.kind == Kind::Heading));
        assert!(rows.iter().any(|row| row.kind == Kind::Body));
    }

    #[test]
    fn links_are_numbered_in_the_text_and_listed_at_the_end() {
        let html = r#"<p>See <a href="https://example.com/rfc">RFC 2094</a> for detail.</p>"#;
        let rows = render(html, 60);
        assert_eq!(rows[0], "See RFC 2094[1] for detail.");
        assert!(
            rows.iter()
                .any(|row| row.contains("[1] https://example.com/rfc")),
            "{rows:?}"
        );
    }

    #[test]
    fn several_links_are_numbered_in_order() {
        let html =
            r#"<p><a href="https://a.example">A</a> and <a href="https://b.example">B</a></p>"#;
        let article = parse(html);
        assert_eq!(article.links, ["https://a.example", "https://b.example"]);
        assert_eq!(render(html, 60)[0], "A[1] and B[2]");
    }

    #[test]
    fn a_link_with_no_visible_text_is_not_numbered() {
        // Tracking pixels and bare anchors would otherwise fill the reference
        // list with entries nothing points at.
        let article = parse(r#"<p>Body <a href="https://tracker.example"></a></p>"#);
        assert!(article.links.is_empty());
    }

    #[test]
    fn entities_are_decoded() {
        assert_eq!(
            render("<p>Tom &amp; Jerry&nbsp;say it&#8217;s fine</p>", 60)[0],
            "Tom & Jerry say it\u{2019}s fine"
        );
    }

    #[test]
    fn styling_survives_a_line_break() {
        let html =
            "<p><strong>A long stretch of bold text that must wrap across lines</strong></p>";
        let rows = layout(&parse(html), 20, false);
        assert!(rows.len() > 1, "should have wrapped");
        for row in &rows {
            assert!(
                row.spans
                    .iter()
                    .all(|span| matches!(span, Inline::Strong(_))),
                "bold was lost on wrap: {row:?}"
            );
        }
    }

    #[test]
    fn inline_code_keeps_its_own_styling() {
        let rows = layout(&parse("<p>Call <code>main()</code> first.</p>"), 60, false);
        assert!(
            rows[0]
                .spans
                .iter()
                .any(|span| matches!(span, Inline::Code(c) if c == "main()")),
            "{:?}",
            rows[0]
        );
    }

    #[test]
    fn the_real_world_article_that_prompted_this() {
        let html = "<p>The borrow checker now accepts this:</p>\
            <pre><code>fn main() {\n    let v = vec![1];\n}</code></pre>\
            <p>Three things changed:</p>\
            <ul><li>Loans end at last use</li><li><strong>Closures</strong> capture fields</li></ul>\
            <blockquote>The biggest change since NLL.</blockquote>";
        let rows = render(html, 50);
        // Every structure survives, rather than collapsing into one paragraph.
        assert!(rows.contains(&"The borrow checker now accepts this:".to_string()));
        assert!(rows.contains(&"  fn main() {".to_string()));
        assert!(rows.contains(&"      let v = vec![1];".to_string()));
        assert!(rows.contains(&"• Loans end at last use".to_string()));
        assert!(
            rows.iter()
                .any(|row| row.contains("The biggest change since NLL."))
        );
    }

    // ─── Tolerance ───────────────────────────────────────────────────────

    #[test]
    fn unclosed_tags_do_not_lose_the_text() {
        let rows = render("<p>One<p>Two<strong>Three", 40);
        let joined = rows.join(" ");
        for needle in ["One", "Two", "Three"] {
            assert!(joined.contains(needle), "{joined:?} lost {needle}");
        }
    }

    #[test]
    fn a_truncated_tag_does_not_spray_markup_into_the_article() {
        let rows = render("<p>Body text</p><a href=\"https://exa", 40);
        assert_eq!(rows, ["Body text"]);
    }

    #[test]
    fn stray_closing_tags_are_harmless() {
        assert_eq!(render("</strong></p></div>Text</em>", 40), ["Text"]);
    }

    #[test]
    fn comments_and_doctypes_are_not_content() {
        let rows = render("<!doctype html><!-- a note --><p>Real text</p>", 40);
        assert_eq!(rows, ["Real text"]);
    }

    #[test]
    fn unknown_elements_keep_their_text() {
        assert_eq!(
            render("<custom-thing>Inside</custom-thing>", 40),
            ["Inside"]
        );
    }

    #[test]
    fn plain_text_with_no_markup_still_renders() {
        assert_eq!(render("Just a sentence.", 40), ["Just a sentence."]);
    }

    #[test]
    fn deeply_nested_markup_does_not_blow_the_stack() {
        let html = format!("{}deep{}", "<div>".repeat(5000), "</div>".repeat(5000));
        assert!(render(&html, 40).iter().any(|row| row.contains("deep")));
    }

    #[test]
    fn a_zero_width_pane_produces_nothing_rather_than_looping() {
        assert!(layout(&parse("<p>Anything</p>"), 0, false).is_empty());
    }

    #[test]
    fn empty_input_produces_nothing() {
        assert!(layout(&parse(""), 40, false).is_empty());
        assert!(layout(&parse("   "), 40, false).is_empty());
    }

    #[test]
    fn attributes_parse_in_every_spelling_feeds_use() {
        for html in [
            r#"<a href="https://example.com">X</a>"#,
            r#"<a href='https://example.com'>X</a>"#,
            r#"<a href=https://example.com>X</a>"#,
            r#"<a  class="c"   href = "https://example.com" >X</a>"#,
        ] {
            assert_eq!(parse(html).links, ["https://example.com"], "{html}");
        }
    }
}
