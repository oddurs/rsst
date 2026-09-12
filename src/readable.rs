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

/// Finds the article in a page, returning its markup.
///
/// `None` when the page has nothing that reads like an article — better to keep
/// showing the feed's own summary than to replace it with a cookie notice.
pub fn extract(html: &str) -> Option<String> {
    let cleaned = strip(html, NOISE);
    let best = CANDIDATES
        .iter()
        .flat_map(|name| containers(&cleaned, name))
        .map(|inner| {
            let score = score(&inner);
            (score, inner)
        })
        .filter(|(score, _)| *score >= MINIMUM as isize)
        .max_by_key(|(score, _)| *score)
        .map(|(_, inner)| inner);

    match best {
        Some(inner) => Some(inner),
        // No container stood out, but the page may simply be plain: fall back
        // to the whole body if it has enough prose to be worth showing.
        None => {
            let body = containers(&cleaned, "body").into_iter().next()?;
            (score(&body) >= MINIMUM as isize).then_some(body)
        }
    }
}

/// How much this looks like prose rather than navigation.
fn score(html: &str) -> isize {
    let text = crate::feed::to_plain_text(html).chars().count() as isize;
    let linked = link_text(html).chars().count() as isize;
    // Navigation is plenty of words, almost all of them anchors.
    text - linked * 3
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

/// The inner markup of every element with this name, outermost first.
fn containers(html: &str, name: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{name}");
    let close = format!("</{name}");
    let mut out = Vec::new();
    let mut cursor = 0;

    while let Some(start) = lower[cursor..].find(&open).map(|at| at + cursor) {
        let after = lower[start + open.len()..].chars().next();
        if !matches!(after, Some(c) if c.is_whitespace() || c == '>' || c == '/') {
            cursor = start + open.len();
            continue;
        }
        let Some(open_end) = lower[start..].find('>').map(|at| at + start) else {
            break;
        };

        // Walk to the matching close, counting nested opens of the same name.
        let mut depth = 1usize;
        let mut at = open_end + 1;
        let mut end = None;
        while at < lower.len() {
            let next_open = lower[at..].find(&open).map(|i| i + at);
            let next_close = lower[at..].find(&close).map(|i| i + at);
            match (next_open, next_close) {
                (Some(o), Some(c)) if o < c => {
                    depth += 1;
                    at = o + open.len();
                }
                (_, Some(c)) => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(c);
                        break;
                    }
                    at = c + close.len();
                }
                _ => break,
            }
        }

        let end = end.unwrap_or(lower.len());
        out.push(html[open_end + 1..end].to_string());
        cursor = open_end + 1;
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
