//! The sidebar's folder tree.
//!
//! A feed carries a list of tags — `["Rust", "Core"]` — which is a path, not a
//! label. Reading only the first of them, as the flat sidebar did, throws away
//! every level below the top and makes two different folders look like one.

use std::collections::BTreeMap;

/// A folder, and what is directly inside it.
#[derive(Debug, Default)]
struct Node {
    /// Child folders, in the order they first appear in the config.
    children: Vec<(String, Node)>,
    /// Feeds directly in this folder, by index into the feed list.
    feeds: Vec<usize>,
}

impl Node {
    fn child(&mut self, name: &str) -> &mut Node {
        if let Some(position) = self.children.iter().position(|(n, _)| n == name) {
            return &mut self.children[position].1;
        }
        self.children.push((name.to_string(), Node::default()));
        &mut self.children.last_mut().expect("just pushed").1
    }
}

/// One drawn row of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Folder {
        /// The full path, so two folders of the same name in different places
        /// are different folders.
        path: Vec<String>,
        collapsed: bool,
        /// Unread in everything beneath it, folders included.
        unread: usize,
        /// For each ancestor level, whether that ancestor has more siblings
        /// below it — which is what decides whether a guide is drawn.
        guides: Vec<bool>,
    },
    Feed {
        index: usize,
        guides: Vec<bool>,
    },
}

impl Row {
    /// How deep this row sits.
    pub fn depth(&self) -> usize {
        match self {
            Row::Folder { guides, .. } | Row::Feed { guides, .. } => guides.len(),
        }
    }
}

/// Builds the visible rows for a set of feeds.
///
/// `paths` is each feed's folder path, `unread` its unread count, and
/// `collapsed` the set of folder paths that are folded shut.
pub fn rows(
    paths: &[Vec<String>],
    unread: &dyn Fn(usize) -> usize,
    collapsed: &dyn Fn(&[String]) -> bool,
) -> Vec<Row> {
    let mut root = Node::default();
    for (index, path) in paths.iter().enumerate() {
        let mut node = &mut root;
        for name in path {
            node = node.child(name);
        }
        node.feeds.push(index);
    }

    let mut rows = Vec::new();
    walk(
        &root,
        &mut Vec::new(),
        &mut Vec::new(),
        unread,
        collapsed,
        &mut rows,
    );
    rows
}

/// Flattens the tree depth-first.
///
/// Folders come before the feeds at the same level, which is the convention
/// everywhere else a tree is drawn, and the guides make membership clear
/// without needing to put loose feeds first to avoid confusion.
fn walk(
    node: &Node,
    path: &mut Vec<String>,
    guides: &mut Vec<bool>,
    unread: &dyn Fn(usize) -> usize,
    collapsed: &dyn Fn(&[String]) -> bool,
    out: &mut Vec<Row>,
) {
    let total = node.children.len() + node.feeds.len();

    for (position, (name, child)) in node.children.iter().enumerate() {
        path.push(name.clone());
        let folded = collapsed(path);
        out.push(Row::Folder {
            path: path.clone(),
            collapsed: folded,
            unread: unread_below(child, unread),
            guides: guides.clone(),
        });

        if !folded {
            // A guide is drawn beside this child for as long as something
            // still follows it at this level.
            guides.push(position + 1 < total);
            walk(child, path, guides, unread, collapsed, out);
            guides.pop();
        }
        path.pop();
    }

    for index in &node.feeds {
        out.push(Row::Feed {
            index: *index,
            guides: guides.clone(),
        });
    }
}

fn unread_below(node: &Node, unread: &dyn Fn(usize) -> usize) -> usize {
    node.feeds.iter().map(|index| unread(*index)).sum::<usize>()
        + node
            .children
            .iter()
            .map(|(_, child)| unread_below(child, unread))
            .sum::<usize>()
}

/// Every folder path in the tree, deepest-last, for a picker to offer.
pub fn folders(paths: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut seen: BTreeMap<Vec<String>, ()> = BTreeMap::new();
    for path in paths {
        for depth in 1..=path.len() {
            seen.insert(path[..depth].to_vec(), ());
        }
    }
    seen.into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(specs: &[&[&str]]) -> Vec<Vec<String>> {
        specs
            .iter()
            .map(|path| path.iter().map(|s| (*s).to_string()).collect())
            .collect()
    }

    fn build(specs: &[&[&str]]) -> Vec<Row> {
        rows(&paths(specs), &|_| 0, &|_| false)
    }

    /// A compact picture of the tree, for asserting shape.
    fn shape(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Folder { path, guides, .. } => {
                    format!("{}[{}]", "  ".repeat(guides.len()), path.join("/"))
                }
                Row::Feed { index, guides } => {
                    format!("{}{index}", "  ".repeat(guides.len()))
                }
            })
            .collect()
    }

    #[test]
    fn feeds_with_no_tags_sit_at_the_top_level() {
        assert_eq!(shape(&build(&[&[], &[]])), ["0", "1"]);
    }

    #[test]
    fn a_tag_path_nests_arbitrarily_deep() {
        let rows = build(&[&["Rust", "Core"], &["Rust", "Ecosystem"], &["Rust"]]);
        assert_eq!(
            shape(&rows),
            [
                "[Rust]",
                "  [Rust/Core]",
                "    0",
                "  [Rust/Ecosystem]",
                "    1",
                "  2",
            ]
        );
    }

    #[test]
    fn two_folders_of_the_same_name_in_different_places_are_different() {
        let rows = build(&[&["Rust", "News"], &["Web", "News"]]);
        let folders: Vec<_> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Folder { path, .. } => Some(path.join("/")),
                _ => None,
            })
            .collect();
        assert_eq!(folders, ["Rust", "Rust/News", "Web", "Web/News"]);
    }

    #[test]
    fn folders_come_before_loose_feeds_at_the_same_level() {
        let rows = build(&[&[], &["Rust"]]);
        assert_eq!(shape(&rows), ["[Rust]", "  1", "0"]);
    }

    #[test]
    fn folding_a_folder_hides_everything_beneath_it() {
        let specs = paths(&[&["Rust", "Core"], &["Rust"], &["Other"]]);
        let rows = rows(&specs, &|_| 0, &|path| path == ["Rust"]);
        assert_eq!(shape(&rows), ["[Rust]", "[Other]", "  2"]);
    }

    #[test]
    fn folding_an_inner_folder_leaves_its_parent_open() {
        let specs = paths(&[&["Rust", "Core"], &["Rust"]]);
        let rows = rows(&specs, &|_| 0, &|path| path == ["Rust", "Core"]);
        assert_eq!(shape(&rows), ["[Rust]", "  [Rust/Core]", "  1"]);
    }

    #[test]
    fn a_folder_counts_everything_beneath_it_including_nested_folders() {
        let specs = paths(&[&["Rust", "Core"], &["Rust", "Core"], &["Rust"]]);
        let rows = rows(&specs, &|index| index + 1, &|_| false);
        match &rows[0] {
            Row::Folder { path, unread, .. } => {
                assert_eq!(path, &["Rust"]);
                // 1 + 2 from Rust/Core, plus 3 directly in Rust.
                assert_eq!(*unread, 6);
            }
            other => panic!("expected a folder, got {other:?}"),
        }
    }

    #[test]
    fn a_collapsed_folder_still_reports_its_count() {
        // Otherwise folding a folder would hide the only reason to open it.
        let specs = paths(&[&["Rust"]]);
        let rows = rows(&specs, &|_| 7, &|_| true);
        match &rows[0] {
            Row::Folder {
                unread, collapsed, ..
            } => {
                assert!(collapsed);
                assert_eq!(*unread, 7);
            }
            other => panic!("expected a folder, got {other:?}"),
        }
    }

    #[test]
    fn a_guide_is_drawn_only_while_something_follows_at_that_level() {
        // Rust has a sibling below it, so its children carry a guide; Last
        // does not, so its children carry none.
        let specs = paths(&[&["Rust", "Core"], &["Last", "Inner"]]);
        let rows = rows(&specs, &|_| 0, &|_| false);
        let guides: Vec<_> = rows
            .iter()
            .map(|row| match row {
                Row::Folder { guides, .. } | Row::Feed { guides, .. } => guides.clone(),
            })
            .collect();
        // 0 Rust · 1 Rust/Core · 2 feed · 3 Last · 4 Last/Inner · 5 feed
        assert_eq!(guides[0], Vec::<bool>::new(), "Rust");
        assert_eq!(guides[1], vec![true], "Rust/Core, more follows Rust");
        assert_eq!(guides[3], Vec::<bool>::new(), "Last");
        assert_eq!(guides[4], vec![false], "Last/Inner, nothing follows Last");
    }

    #[test]
    fn depth_matches_the_guides() {
        let rows = build(&[&["A", "B", "C"]]);
        assert_eq!(
            rows.iter().map(Row::depth).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn every_folder_on_a_path_is_offered_for_moving_into() {
        assert_eq!(
            folders(&paths(&[&["Rust", "Core"], &["Web"]])),
            vec![
                vec!["Rust".to_string()],
                vec!["Rust".to_string(), "Core".to_string()],
                vec!["Web".to_string()],
            ]
        );
    }

    #[test]
    fn a_tree_with_no_folders_offers_none() {
        assert!(folders(&paths(&[&[], &[]])).is_empty());
    }
}
