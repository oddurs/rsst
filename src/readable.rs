//! Pulling the article out of a web page.
//!
//! A page is mostly not the article: navigation, a cookie banner, a sidebar of
//! related posts, a footer of links. The job is to find the part someone came
//! to read and throw the rest away.
//!
//! The heuristic is the one every reader-mode implementation starts from —
//! the container holding the most prose wins — with a penalty for text that is
//! mostly links, because that is what navigation looks like: plenty of words,
//! almost all of them anchors.

/// Elements whose contents are never the article.
const NOISE: &[&str] = &[
    "script", "style", "noscript", "nav", "header", "footer", "aside", "form", "iframe", "svg",
    "button", "figure",
];

/// Elements worth considering as the article.
const CANDIDATES: &[&str] = &["article", "main", "section", "div"];

/// Text below this is not an article, whatever else it looks like.
const MINIMUM: usize = 200;

/// The most containers to weigh up.
///
/// A page nested a thousand divs deep offers a thousand candidates, each one
/// nearly the whole document — scoring them all is quadratic, and a page can
/// choose to be nested that deeply. Found by the fuzzer, which hung here.
const MAX_CANDIDATES: usize = 200;

/// The most of a candidate to weigh.
///
/// Scoring compares prose against link text, which is a ratio — a prefix
/// answers it as well as the whole thing. Reading all of every candidate is
/// what made nesting quadratic: two hundred candidates, each nearly the whole
/// document. Deep nesting went 182ms → 2.9s → 15.1s at 1k, 10k and 50k levels.
const MAX_SCORED: usize = 32 * 1024;

/// The most markup to consider at all.
///
/// Past this the page is not an article anyone is going to read, and the cost
/// of deciding that grows with the input.
const MAX_INPUT: usize = 1 << 21;

/// Finds the article in a page, returning its markup.
///
/// `None` when the page has nothing that reads like an article — better to keep
/// showing the feed's own summary than to replace it with a cookie notice.
pub fn extract(html: &str) -> Option<String> {
    let html = prefix(html, MAX_INPUT);
    let cleaned = strip(html, NOISE);

    // Ranges rather than copies: materialising the inner markup of every
    // candidate is what made a deeply nested page quadratic.
    let best = containers(&cleaned, CANDIDATES)
        .into_iter()
        .map(|(start, end)| (score(&cleaned[start..end]), start, end))
        .filter(|(score, ..)| *score >= MINIMUM as isize)
        .max_by_key(|(score, ..)| *score);

    match best {
        Some((_, start, end)) => Some(cleaned[start..end].to_string()),
        // No container stood out, but the page may simply be plain: fall back
        // to the whole body if it has enough prose to be worth showing.
        None => {
            let (start, end) = containers(&cleaned, &["body"]).into_iter().next()?;
            let body = &cleaned[start..end];
            (score(body) >= MINIMUM as isize).then(|| body.to_string())
        }
    }
}

/// How much this looks like prose rather than navigation.
fn score(html: &str) -> isize {
    let html = prefix(html, MAX_SCORED);
    let text = crate::feed::to_plain_text(html).chars().count() as isize;
    let linked = link_text(html).chars().count() as isize;
    // Navigation is plenty of words, almost all of them anchors.
    text - linked * 3
}

/// The first `limit` bytes, cut on a character boundary.
fn prefix(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The visible text inside anchors.
fn link_text(html: &str) -> String {
    let mut out = String::new();
    let lower = html.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(start) = lower[cursor..].find("<a").map(|at| at + cursor) {
        let Some(open_end) = lower[start..].find('>').map(|at| at + start) else {
            break;
        };
        let Some(close) = lower[open_end..].find("</a").map(|at| at + open_end) else {
            break;
        };
        out.push_str(&crate::feed::to_plain_text(&html[open_end + 1..close]));
        out.push(' ');
        cursor = close + 3;
    }
    out
}

/// Removes elements and everything inside them.
fn strip(html: &str, names: &[&str]) -> String {
    let mut out = html.to_string();
    for name in names {
        loop {
            let lower = out.to_ascii_lowercase();
            let open = format!("<{name}");
            let Some(start) = lower.find(&open) else {
                break;
            };
            // Make sure it is the tag and not a longer name starting the same.
            let after = lower[start + open.len()..].chars().next();
            if !matches!(after, Some(c) if c.is_whitespace() || c == '>' || c == '/') {
                // Not this element; step past it to avoid looping forever.
                let Some(skip) = lower[start + 1..].find('<') else {
                    break;
                };
                let _ = skip;
                break;
            }
            let close = format!("</{name}");
            let end = match lower[start..].find(&close) {
                Some(at) => lower[start + at..]
                    .find('>')
                    .map(|gt| start + at + gt + 1)
                    .unwrap_or(out.len()),
                // Unclosed: drop to the end, which is what a browser does with
                // an unclosed script too.
                None => out.len(),
            };
            out.replace_range(start..end, "");
        }
    }
    out
}

/// The inner range of every candidate element, in one pass over the document.
///
/// One pass, not one per candidate. Matching each element's closing tag by
/// scanning forward from it is O(n) each time, and a page nested a thousand
/// levels deep offers a thousand elements — which is how 50,000 levels came to
/// take fifteen seconds. A stack answers all of them at once.
fn containers(html: &str, names: &[&str]) -> Vec<(usize, usize)> {
    let bytes = html.as_bytes();
    let mut open: Vec<(usize, usize)> = Vec::new(); // (name index, inner start)
    let mut out = Vec::new();
    let mut at = 0usize;

    while at < bytes.len() && out.len() < MAX_CANDIDATES {
        let Some(lt) = bytes[at..].iter().position(|b| *b == b'<').map(|i| i + at) else {
            break;
        };
        let Some(gt) = bytes[lt..].iter().position(|b| *b == b'>').map(|i| i + lt) else {
            break;
        };
        at = gt + 1;

        let raw = &html[lt + 1..gt];
        let closing = raw.starts_with('/');
        let name = raw
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        let Some(which) = names.iter().position(|candidate| *candidate == name) else {
            continue;
        };

        if closing {
            // Unwind to the matching open, tolerating tags left unclosed.
            if let Some(position) = open.iter().rposition(|(index, _)| *index == which) {
                let (_, inner) = open.remove(position);
                open.truncate(position);
                out.push((inner, lt));
            }
        } else if !raw.ends_with('/') {
            open.push((which, gt + 1));
        }
    }

    // Anything still open ran to the end of the document, which is what a
    // browser does with an unclosed element too.
    for (_, inner) in open.into_iter().take(MAX_CANDIDATES - out.len()) {
        out.push((inner, html.len()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prose(words: usize) -> String {
        (0..words)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn the_article_wins_over_the_furniture() {
        let html = format!(
            "<html><body>\
             <nav><a href='/'>Home</a><a href='/about'>About</a></nav>\
             <article><p>{}</p></article>\
             <footer><a href='/tos'>Terms</a></footer>\
             </body></html>",
            prose(80)
        );
        let found = extract(&html).expect("an article");
        assert!(found.contains("word0"));
        assert!(!found.contains("Home"), "navigation came along: {found}");
        assert!(!found.contains("Terms"));
    }

    #[test]
    fn scripts_and_styles_never_reach_the_reader() {
        let html = format!(
            "<body><article><script>alert('x')</script><style>p{{color:red}}</style>\
             <p>{}</p></article></body>",
            prose(80)
        );
        let found = extract(&html).expect("an article");
        assert!(!found.contains("alert"));
        assert!(!found.contains("color:red"));
    }

    #[test]
    fn a_wall_of_links_loses_to_real_prose() {
        // Navigation is plenty of words, almost all of them anchors.
        let links: String = (0..60)
            .map(|i| format!("<a href='/p{i}'>Some long link title here {i}</a>"))
            .collect();
        let html = format!(
            "<body><div>{links}</div><div><p>{}</p></div></body>",
            prose(80)
        );
        let found = extract(&html).expect("an article");
        assert!(found.contains("word0"), "picked the link wall: {found}");
    }

    #[test]
    fn the_biggest_of_several_candidates_wins() {
        let html = format!(
            "<body><div><p>{}</p></div><div><p>{}</p></div></body>",
            prose(40),
            prose(200)
        );
        let found = extract(&html).expect("an article");
        assert!(found.contains(&format!("word{}", 199)));
    }

    #[test]
    fn nested_containers_do_not_confuse_the_matching() {
        let html = format!(
            "<body><div class='outer'><div class='inner'><p>{}</p></div></div></body>",
            prose(100)
        );
        let found = extract(&html).expect("an article");
        assert!(found.contains("word0"));
        // The outer div must not swallow the closing tag of the inner one.
        assert!(!found.contains("<body"), "{found}");
    }

    #[test]
    fn a_page_with_no_article_is_left_alone() {
        // Better to keep the feed's summary than to replace it with a banner.
        let html = "<body><nav>Home About</nav><div>Cookies?</div></body>";
        assert_eq!(extract(html), None);
    }

    #[test]
    fn an_empty_or_broken_page_is_not_an_article() {
        assert_eq!(extract(""), None);
        assert_eq!(extract("<html><body>"), None);
        assert_eq!(extract("not html at all"), None);
    }

    #[test]
    fn a_plain_page_with_no_containers_falls_back_to_the_body() {
        let html = format!("<html><body><p>{}</p></body></html>", prose(100));
        assert!(extract(&html).expect("the body").contains("word0"));
    }

    #[test]
    fn an_unclosed_script_does_not_eat_the_whole_page_silently() {
        // It does eat the rest, as a browser would — the point is that it
        // returns rather than hanging or panicking.
        let html = format!("<body><article><p>{}</p></article><script>", prose(80));
        assert!(extract(&html).is_some());
    }

    #[test]
    fn hostile_input_terminates() {
        for html in [
            &"<div>".repeat(5000),
            &"<article>".repeat(2000),
            &"<a href='x'>".repeat(5000),
            &format!("<body>{}</body>", "<<<>>>".repeat(2000)),
        ] {
            let _ = extract(html);
        }
    }
}
