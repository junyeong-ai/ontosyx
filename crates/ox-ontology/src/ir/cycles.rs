//! Cycle detection for IR reference graphs.
//!
//! Several IR collections form directed graphs that the editor must
//! keep acyclic for traversal to terminate: the glossary
//! `related_terms` Broader/Narrower hierarchy and the
//! `lifecycle.replaced_by` deprecation chain are the canonical
//! examples. This module exposes a generic DFS-based detector that
//! returns a representative cycle path so the diagnostic can name
//! the offending nodes.

use std::collections::HashMap;
use std::hash::Hash;

/// Find a cycle reachable from any of the supplied roots in the
/// directed graph defined by `edges`.
///
/// Returns `Some(path)` where `path[0] → path[1] → … → path[n-1] →
/// path[0]` is a cycle. Returns `None` when the graph is acyclic.
///
/// `edges` is consulted lazily on each visit; callers typically close
/// over the IR collection they want to traverse.
pub(crate) fn find_cycle<N, I, F>(roots: I, mut edges: F) -> Option<Vec<N>>
where
    N: Eq + Hash + Clone,
    I: IntoIterator<Item = N>,
    F: FnMut(&N) -> Vec<N>,
{
    let mut color: HashMap<N, Color> = HashMap::new();
    for root in roots {
        let mut path: Vec<N> = Vec::new();
        if let Some(cycle) = dfs(&root, &mut color, &mut path, &mut edges) {
            return Some(cycle);
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    /// Currently on the DFS stack — encountering this colour again is a
    /// back-edge and identifies a cycle.
    Gray,
    /// Fully explored — re-entry returns immediately without traversal.
    Black,
}

fn dfs<N, F>(
    node: &N,
    color: &mut HashMap<N, Color>,
    path: &mut Vec<N>,
    edges: &mut F,
) -> Option<Vec<N>>
where
    N: Eq + Hash + Clone,
    F: FnMut(&N) -> Vec<N>,
{
    match color.get(node) {
        Some(Color::Black) => return None,
        Some(Color::Gray) => {
            let start = path.iter().position(|n| n == node)?;
            return Some(path[start..].to_vec());
        }
        None => {}
    }
    color.insert(node.clone(), Color::Gray);
    path.push(node.clone());
    for next in edges(node) {
        if let Some(cycle) = dfs(&next, color, path, edges) {
            return Some(cycle);
        }
    }
    path.pop();
    color.insert(node.clone(), Color::Black);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph<'a>(adj: &'a [(&'a str, &'a str)]) -> impl FnMut(&String) -> Vec<String> + 'a {
        move |n: &String| {
            adj.iter()
                .filter(|(from, _)| *from == n)
                .map(|(_, to)| (*to).to_owned())
                .collect()
        }
    }

    #[test]
    fn detects_self_loop() {
        let adj = [("a", "a")];
        let cycle = find_cycle(["a".to_owned()], graph(&adj));
        assert_eq!(cycle, Some(vec!["a".to_owned()]));
    }

    #[test]
    fn detects_two_cycle() {
        let adj = [("a", "b"), ("b", "a")];
        let cycle = find_cycle(["a".to_owned()], graph(&adj));
        assert_eq!(cycle, Some(vec!["a".to_owned(), "b".to_owned()]));
    }

    #[test]
    fn detects_longer_cycle() {
        let adj = [("a", "b"), ("b", "c"), ("c", "d"), ("d", "b")];
        let cycle = find_cycle(["a".to_owned()], graph(&adj));
        let path = cycle.expect("cycle expected");
        assert!(path.contains(&"b".to_owned()));
        assert!(path.contains(&"c".to_owned()));
        assert!(path.contains(&"d".to_owned()));
    }

    #[test]
    fn acyclic_returns_none() {
        let adj = [("a", "b"), ("b", "c"), ("c", "d")];
        assert_eq!(find_cycle(["a".to_owned()], graph(&adj)), None);
    }

    #[test]
    fn unreachable_cycle_found_by_alternative_root() {
        let adj = [("a", "b"), ("c", "d"), ("d", "c")];
        let cycle = find_cycle(["a".to_owned(), "c".to_owned()], graph(&adj));
        let path = cycle.expect("cycle expected");
        assert_eq!(path.len(), 2);
        assert!(path.contains(&"c".to_owned()));
        assert!(path.contains(&"d".to_owned()));
    }
}
