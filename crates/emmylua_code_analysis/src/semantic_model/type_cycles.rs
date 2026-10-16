//! Shared cycle detection for alias parameters and type comparisons.

use hashbrown::{HashMap, HashSet};
use std::hash::Hash;

/// Assign one component to each group of mutually reachable nodes.
/// Both passes use work lists so long finite paths do not use the call stack.
pub(super) fn strongly_connected_components<K: Clone + Eq + Hash>(
    edges: &[(K, K)],
) -> HashMap<K, usize> {
    let mut forward: HashMap<&K, Vec<&K>> = HashMap::new();
    let mut reverse: HashMap<&K, Vec<&K>> = HashMap::new();
    for (source, target) in edges {
        forward.entry(source).or_default().push(target);
        reverse.entry(target).or_default().push(source);
    }

    let mut seen = HashSet::new();
    let mut order = Vec::new();
    for source in forward.keys() {
        let mut pending = vec![(*source, false)];
        while let Some((node, leaving)) = pending.pop() {
            if leaving {
                order.push(node);
            } else if seen.insert(node) {
                pending.push((node, true));
                if let Some(children) = forward.get(node) {
                    pending.extend(children.iter().map(|child| (*child, false)));
                }
            }
        }
    }

    let mut components = HashMap::new();
    while let Some(node) = order.pop() {
        if components.contains_key(node) {
            continue;
        }
        let component = components.len();
        let mut pending = vec![node];
        while let Some(node) = pending.pop() {
            if components.contains_key(node) {
                continue;
            }
            components.insert(node.clone(), component);
            if let Some(parents) = reverse.get(node) {
                pending.extend(parents);
            }
        }
    }
    components
}
