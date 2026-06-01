//! Cross-file dependency resolution for the Salsa summary layer.
//!
//! Builds a topological order of files based on cross-file type references.
//! Files that define types used by other files are resolved first, ensuring
//! that when the projection layer resolves a cross-file type reference, the
//! target file's summary has already been computed.
//!
//! This replaces the legacy analyzer's O(n²) retry loop with O(V+E) Kahn's
//! algorithm on the file dependency graph.

use crate::{DbIndex, FileId, SalsaDocTypeLoweredKind};
use hashbrown::{HashMap, HashSet};
use smol_str::SmolStr;

fn collect_names(node_kind: &SalsaDocTypeLoweredKind, names: &mut Vec<SmolStr>) {
    if let SalsaDocTypeLoweredKind::Name { name } = node_kind {
        names.push(name.clone())
    }
}

/// Collects the set of FileIds that a file's type definitions depend on.
fn collect_file_type_dependencies(db: &DbIndex, file_id: FileId) -> HashSet<FileId> {
    let mut deps = HashSet::new();
    let index = db.get_type_def_reverse_index();
    let summary = db.get_summary_db();

    let Some(lowered) = summary.doc().lowered_types(file_id) else {
        return deps;
    };

    let mut all_names = Vec::new();
    for node in &lowered.types {
        collect_names(&node.kind, &mut all_names);
    }

    for name in &all_names {
        if let Some(defs) = index.by_name.get(name) {
            for (def_file_id, _) in defs {
                if *def_file_id != file_id {
                    deps.insert(*def_file_id);
                }
            }
        }
    }

    deps
}

/// Returns files in cross-file dependency order using Kahn's algorithm.
/// Files with no cross-file dependencies come first.
/// Files that depend on other files come after their dependencies.
/// Mutual dependencies (cycles) are grouped at the end.
pub(crate) fn get_cross_file_resolve_order(db: &DbIndex) -> Vec<FileId> {
    let summary = db.get_summary_db();
    let file_ids = summary.file_ids();
    if file_ids.len() <= 1 {
        return file_ids;
    }

    // Build dependency graph: file → files it depends on
    let mut dep_graph: HashMap<FileId, HashSet<FileId>> = HashMap::new();
    for &file_id in &file_ids {
        dep_graph.insert(file_id, collect_file_type_dependencies(db, file_id));
    }

    // Build reverse edges and in-degrees
    let mut reverse_deps: HashMap<FileId, Vec<FileId>> = HashMap::new();
    let mut in_degree: HashMap<FileId, usize> = HashMap::new();
    for &file_id in &file_ids {
        in_degree.entry(file_id).or_insert(0);
        reverse_deps.entry(file_id).or_default();
    }
    for (&file_id, deps) in &dep_graph {
        for dep in deps {
            reverse_deps.entry(*dep).or_default().push(file_id);
            *in_degree.entry(file_id).or_insert(0) += 1;
        }
    }

    // Kahn's algorithm: nodes with in-degree 0 first
    let mut result = Vec::with_capacity(file_ids.len());
    let mut queue: Vec<FileId> = file_ids
        .iter()
        .copied()
        .filter(|fid| dep_graph.get(fid).is_none_or(|d| d.is_empty()))
        .collect();

    while let Some(file_id) = queue.pop() {
        result.push(file_id);
        if let Some(dependents) = reverse_deps.get(&file_id) {
            for &dependent in dependents {
                if let Some(entry) = in_degree.get_mut(&dependent) {
                    *entry = entry.saturating_sub(1);
                    if *entry == 0 {
                        queue.push(dependent);
                    }
                }
            }
        }
    }

    // Append remaining files in dependency cycles
    for &file_id in &file_ids {
        if !result.contains(&file_id) {
            result.push(file_id);
        }
    }

    result
}
