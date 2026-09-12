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
    /// An image, described by its alt text. A terminal cannot show the
    /// picture, but it can say one was here and what it was of — which is
    /// more than dropping it silently.
    Image(String),
    /// Rows of cells. The first is the header when the table had one.
    Table {
        header: Option<Vec<String>>,
        rows: Vec<Vec<String>>,
    },
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
    /// A stand-in for a picture.
    Image,
    /// A row of a table.
    Table,
}

/// A parsed article: its blocks, and the links it referred to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Article {
    pub blocks: Vec<Block>,
    /// Link targets, in the order they were numbered.
    pub links: Vec<String>,
}

/// Reads an entry's HTML.
/// A table being gathered as the document is walked.
#[derive(Default)]
struct Gathering {
    header: Option<Vec<String>>,
    rows: Vec<Vec<String>>,
}

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
    // A table is gathered as it is walked: cells into a row, rows into a
    // table, because none of it means anything until the table closes.
    let mut table: Option<Gathering> = None;
    let mut row: Vec<String> = Vec::new();
    let mut header_row = false;
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
                "img" => {
                    flush_block!();
                    let alt = attributes
                        .iter()
                        .find(|(key, _)| key == "alt")
                        .map(|(_, value)| value.trim().to_string())
                        .unwrap_or_default();
                    article.blocks.push(Block::Image(alt));
                }
                "table" => {
                    flush_block!();
                    table = Some(Gathering::default());
                }
                "tr" => {
                    flush_text!();
                    inlines.clear();
                    row.clear();
                    header_row = false;
                }
                "th" | "td" => {
                    flush_text!();
                    inlines.clear();
                    header_row |= name == "th";
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
                "th" | "td" => {
                    flush_text!();
                    let cell: String = inlines.iter().map(Inline::text).collect();
                    inlines.clear();
                    row.push(cell.trim().to_string());
                }
                "tr" => {
                    if let Some(Gathering { header, rows }) = &mut table
                        && !row.is_empty()
                    {
                        let cells = std::mem::take(&mut row);
                        if header_row && header.is_none() {
                            *header = Some(cells);
                        } else {
                            rows.push(cells);
                        }
                    }
                    row.clear();
                }
                "table" => {
                    if let Some(Gathering { header, rows }) = table.take()
                        && (header.is_some() || !rows.is_empty())
                    {
                        article.blocks.push(Block::Table { header, rows });
                    }
                    inlines.clear();
                    text.clear();
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
/// The widest line of prose worth setting.
///
/// Typography has converged on roughly 45–75 characters for a reason that is
/// mechanical rather than aesthetic: at the end of a line the eye has to travel
/// back and find the start of the next, and the further it travels the more
/// often it lands on the wrong one. A wider window should not make a reader
/// harder to read.
pub const DEFAULT_MEASURE: usize = 72;

/// How prose should be set in the space available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measure {
    /// The widest line of prose, or `None` to use whatever there is.
    pub columns: Option<usize>,
    /// Draw with ASCII only.
    pub ascii: bool,
}

impl Default for Measure {
    fn default() -> Self {
        Self {
            columns: Some(DEFAULT_MEASURE),
            ascii: false,
        }
    }
}

impl Measure {
    /// The width prose should wrap to, and how far to indent it.
    ///
    /// Centred, so a wide pane does not leave the text against one edge. A pane
    /// narrower than the measure is simply used as it is — indenting there
    /// would push text off the screen to honour a rule meant to help.
    pub fn fit(&self, available: usize) -> (usize, usize) {
        match self.columns {
            Some(columns) if columns < available => (columns, (available - columns) / 2),
            _ => (available, 0),
        }
    }
}

/// Lays an article out for a pane of the given width.
///
/// `ascii` decides the glyphs, not the structure — a terminal that cannot draw
/// a bullet still has lists.
pub fn layout(article: &Article, width: usize, measure: Measure) -> Vec<Row> {
    let mut rows = Vec::new();
    if width == 0 {
        return rows;
    }
    let ascii = measure.ascii;
    // Prose holds a measure; code keeps the full width, because its line
    // breaks belong to whoever wrote it and narrowing them loses meaning.
    let (prose, margin) = measure.fit(width);

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
                rows.extend(wrap_spans(spans, prose, margin, Kind::Body));
            }
            Block::Heading(_, spans) => {
                blank(&mut rows);
                rows.extend(wrap_spans(spans, prose, margin, Kind::Heading));
            }
            Block::Quote(spans) => {
                blank(&mut rows);
                // Indented rather than prefixed here; the renderer draws the
                // rule, because only it knows what the terminal can draw.
                rows.extend(wrap_spans(
                    spans,
                    prose.saturating_sub(2),
                    margin + 2,
                    Kind::Quote,
                ));
            }
            Block::Item { ordinal, body } => {
                let marker = match ordinal {
                    Some(n) => format!("{n}. "),
                    None if ascii => "* ".to_string(),
                    None => "• ".to_string(),
                };
                let indent = marker.chars().count();
                let mut lines = wrap_spans(
                    body,
                    prose.saturating_sub(indent),
                    margin + indent,
                    Kind::Body,
                );
                // The marker replaces the first line's indent, so the rest of
                // the item hangs beneath the text rather than the bullet.
                if let Some(first) = lines.first_mut() {
                    first.indent = margin;
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
                        indent: margin.min(2) + 2,
                        spans: vec![Inline::Code(text)],
                    });
                }
            }
            Block::Image(alt) => {
                blank(&mut rows);
                let label = if alt.is_empty() {
                    // Still acknowledged: a reader should know a picture was
                    // here, even when the publisher did not describe it.
                    "[image]".to_string()
                } else {
                    format!("[image: {alt}]")
                };
                rows.extend(wrap_spans(
                    &[Inline::Text(label)],
                    prose,
                    margin,
                    Kind::Image,
                ));
            }
            Block::Table {
                header,
                rows: cells,
            } => {
                blank(&mut rows);
                rows.extend(table_rows(header.as_deref(), cells, prose, margin, ascii));
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
            // A URL is not prose, so it may run past the measure - but it still
            // starts on the same left edge as the text that referenced it.
            for (line, text) in wrap(href, width.saturating_sub(margin + indent))
                .into_iter()
                .enumerate()
            {
                rows.push(Row {
                    kind: Kind::Reference,
                    indent: if line == 0 { margin } else { margin + indent },
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

/// Lays a table out as aligned columns.
///
/// Where the columns will not fit, each row is set as `header: value` lines
/// instead — a narrow table that has been stacked is still readable, whereas
/// one squeezed into too few columns is not.
fn table_rows(
    header: Option<&[String]>,
    body: &[Vec<String>],
    width: usize,
    margin: usize,
    ascii: bool,
) -> Vec<Row> {
    let columns = header
        .map(<[String]>::len)
        .into_iter()
        .chain(body.iter().map(Vec::len))
        .max()
        .unwrap_or(0);
    if columns == 0 || width == 0 {
        return Vec::new();
    }

    fn cell(row: &[String], index: usize) -> &str {
        row.get(index).map(String::as_str).unwrap_or("")
    }
    let widths: Vec<usize> = (0..columns)
        .map(|index| {
            header
                .map(|row| cell(row, index).chars().count())
                .into_iter()
                .chain(body.iter().map(|row| cell(row, index).chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let gap = 2;
    let needed: usize = widths.iter().sum::<usize>() + gap * columns.saturating_sub(1);
    let mut out = Vec::new();

    let line = |row: &[String], kind: Kind| Row {
        kind,
        indent: margin,
        spans: vec![Inline::Text(
            (0..columns)
                .map(|index| {
                    let text = cell(row, index);
                    let pad = widths[index].saturating_sub(text.chars().count());
                    // The last column is not padded; trailing space is not
                    // alignment, it is just space.
                    if index + 1 == columns {
                        text.to_string()
                    } else {
                        format!("{text}{}{}", " ".repeat(pad), " ".repeat(gap))
                    }
                })
                .collect::<String>(),
        )],
    };

    if needed <= width {
        if let Some(header) = header {
            out.push(line(header, Kind::Heading));
            let rule = if ascii { "-" } else { "─" };
            out.push(Row {
                kind: Kind::Table,
                indent: margin,
                spans: vec![Inline::Text(rule.repeat(needed.min(width)))],
            });
        }
        out.extend(body.iter().map(|row| line(row, Kind::Table)));
        return out;
    }

    // Too wide: stack each row as labelled lines rather than mangling it.
    for (position, row) in body.iter().enumerate() {
        if position > 0 {
            out.push(Row {
                kind: Kind::Blank,
                indent: 0,
                spans: Vec::new(),
            });
        }
        for index in 0..columns {
            let value = cell(row, index);
            if value.is_empty() {
                continue;
            }
            let label = header
                .map(|head| cell(head, index))
                .filter(|label| !label.is_empty())
                .map(|label| format!("{label}: "))
                .unwrap_or_default();
            out.extend(wrap_spans(
                &[Inline::Text(format!("{label}{value}"))],
                width,
                margin,
                Kind::Table,
            ));
        }
    }
    out
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
        text_of(&layout(
            &parse(html),
            width,
            Measure {
                columns: None,
                ascii: false,
            },
        ))
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

    fn measured(html: &str, width: usize, columns: usize) -> Vec<String> {
        text_of(&layout(
            &parse(html),
            width,
            Measure {
                columns: Some(columns),
                ascii: false,
            },
        ))
    }

    #[test]
    fn prose_holds_its_measure_on_a_wide_pane() {
        // The point of the whole item: a wider window must not mean longer
        // lines, because long lines are mechanically harder to read.
        let html = format!("<p>{}</p>", "word ".repeat(200));
        for line in measured(&html, 200, 60) {
            assert!(
                line.trim_end().chars().count() <= 60 + 70,
                "a line ran to {}",
                line.trim_end().chars().count()
            );
        }
        let widest = measured(&html, 200, 60)
            .iter()
            .map(|line| line.trim_start().trim_end().chars().count())
            .max()
            .unwrap_or(0);
        assert!(widest <= 60, "text wrapped to {widest}, not 60");
    }

    #[test]
    fn the_measure_is_centred_in_the_pane() {
        let rows = measured("<p>Short line.</p>", 100, 60);
        // (100 - 60) / 2
        assert!(rows[0].starts_with(&" ".repeat(20)), "{:?}", rows[0]);
    }

    #[test]
    fn a_pane_narrower_than_the_measure_is_used_as_it_is() {
        // Indenting here would push text off the screen to honour a rule
        // that exists to make it easier to read.
        let rows = measured("<p>Some words here.</p>", 30, 60);
        assert!(!rows[0].starts_with(' '), "{:?}", rows[0]);
    }

    #[test]
    fn code_keeps_the_full_width_even_when_prose_does_not() {
        // Its line breaks are the author's; narrowing them loses meaning.
        let long = "x".repeat(90);
        let rows = measured(&format!("<pre>{long}</pre>"), 100, 40);
        assert!(
            rows[0].trim().chars().count() > 40,
            "code was cut to the prose measure: {}",
            rows[0].trim().chars().count()
        );
    }

    #[test]
    fn turning_the_measure_off_uses_the_whole_pane() {
        let html = format!("<p>{}</p>", "word ".repeat(100));
        let rows = text_of(&layout(
            &parse(&html),
            120,
            Measure {
                columns: None,
                ascii: false,
            },
        ));
        let widest = rows
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        assert!(widest > 100, "widest line was only {widest}");
    }

    #[test]
    fn the_reference_list_follows_the_measure_too() {
        let html = r#"<p>See <a href="https://example.com/a">a</a>.</p>"#;
        let rows = measured(html, 100, 60);
        let reference = rows
            .iter()
            .find(|row| row.contains("[1]"))
            .expect("a reference");
        assert!(reference.starts_with(&" ".repeat(20)), "{reference:?}");
    }

    #[test]
    fn an_image_becomes_its_alt_text() {
        let rows = render(
            r#"<p>Before</p><img src="/d.png" alt="A diagram of the checker">"#,
            60,
        );
        assert!(
            rows.iter()
                .any(|row| row.contains("[image: A diagram of the checker]")),
            "{rows:?}"
        );
    }

    #[test]
    fn an_image_with_no_alt_text_is_still_acknowledged() {
        // Dropping it silently tells the reader nothing was there.
        let rows = render(r#"<img src="/d.png">"#, 60);
        assert!(rows.iter().any(|row| row.contains("[image]")), "{rows:?}");
    }

    #[test]
    fn a_table_is_laid_out_in_aligned_columns() {
        let html = "<table><tr><th>Version</th><th>Date</th></tr>                    <tr><td>1.98</td><td>Sep 2026</td></tr>                    <tr><td>1.97</td><td>Jul 2026</td></tr></table>";
        let rows = render(html, 60);
        let body: Vec<&String> = rows.iter().filter(|row| !row.trim().is_empty()).collect();

        assert!(
            body[0].contains("Version") && body[0].contains("Date"),
            "{body:?}"
        );
        // Every row starts its second column at the same place.
        let at = |row: &str, needle: &str| row.find(needle).expect("column");
        assert_eq!(at(body[0], "Date"), at(body[2], "Sep 2026"));
        assert_eq!(at(body[2], "Sep 2026"), at(body[3], "Jul 2026"));
    }

    #[test]
    fn the_table_that_prompted_this_is_no_longer_a_word() {
        // It rendered as `VersionDate1.98Sep 2026`.
        let html = "<table><tr><th>Version</th><th>Date</th></tr>                    <tr><td>1.98</td><td>Sep 2026</td></tr></table>";
        let joined = render(html, 60).join("\n");
        assert!(!joined.contains("VersionDate"), "{joined}");
        assert!(!joined.contains("1.98Sep"), "{joined}");
    }

    #[test]
    fn a_header_is_separated_from_the_body() {
        let html = "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>";
        let rows = layout(
            &parse(html),
            60,
            Measure {
                columns: None,
                ascii: false,
            },
        );
        assert!(
            rows.iter().any(|row| row.kind == Kind::Heading),
            "no header row"
        );
    }

    #[test]
    fn a_table_too_wide_to_fit_is_stacked_rather_than_mangled() {
        let wide = "x".repeat(40);
        let html = format!(
            "<table><tr><th>First</th><th>Second</th></tr><tr><td>{wide}</td><td>{wide}</td></tr></table>"
        );
        let rows = render(&html, 30);
        let joined = rows.join("\n");
        // Labelled lines rather than columns squeezed into nothing.
        assert!(joined.contains("First:"), "{joined}");
        assert!(joined.contains("Second:"), "{joined}");
        for row in &rows {
            assert!(
                row.chars().count() <= 30,
                "a row ran to {}",
                row.chars().count()
            );
        }
    }

    #[test]
    fn a_table_with_no_header_still_lays_out() {
        let html = "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>";
        let rows = render(html, 40);
        let body: Vec<&String> = rows.iter().filter(|row| !row.trim().is_empty()).collect();
        assert_eq!(body.len(), 2, "{body:?}");
    }

    #[test]
    fn ragged_rows_do_not_lose_cells_or_panic() {
        let html = "<table><tr><td>a</td></tr><tr><td>b</td><td>c</td><td>d</td></tr></table>";
        let joined = render(html, 40).join("\n");
        for needle in ["a", "b", "c", "d"] {
            assert!(joined.contains(needle), "{joined} lost {needle}");
        }
    }

    #[test]
    fn an_empty_or_malformed_table_is_harmless() {
        assert!(render("<table></table>", 40).is_empty());
        assert!(!render("<table><tr><td>only</td>", 40).is_empty());
        let _ = render("<table><table><tr><td>nested</td></tr></table></table>", 40);
    }

    #[test]
    fn a_terminal_without_a_bullet_still_gets_a_list() {
        let rows = text_of(&layout(
            &parse("<ul><li>One</li></ul>"),
            40,
            Measure {
                columns: None,
                ascii: true,
            },
        ));
        assert_eq!(rows, ["* One"]);
        let rows = text_of(&layout(
            &parse("<ul><li>One</li></ul>"),
            40,
            Measure {
                columns: None,
                ascii: false,
            },
        ));
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
            Measure {
                columns: None,
                ascii: false,
            },
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
        let rows = layout(
            &parse("<h2>A heading</h2><p>Body.</p>"),
            40,
            Measure {
                columns: None,
                ascii: false,
            },
        );
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
        let rows = layout(
            &parse(html),
            20,
            Measure {
                columns: None,
                ascii: false,
            },
        );
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
        let rows = layout(
            &parse("<p>Call <code>main()</code> first.</p>"),
            60,
            Measure {
                columns: None,
                ascii: false,
            },
        );
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
        assert!(
            layout(
                &parse("<p>Anything</p>"),
                0,
                Measure {
                    columns: None,
                    ascii: false
                }
            )
            .is_empty()
        );
    }

    #[test]
    fn empty_input_produces_nothing() {
        assert!(
            layout(
                &parse(""),
                40,
                Measure {
                    columns: None,
                    ascii: false
                }
            )
            .is_empty()
        );
        assert!(
            layout(
                &parse("   "),
                40,
                Measure {
                    columns: None,
                    ascii: false
                }
            )
            .is_empty()
        );
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
