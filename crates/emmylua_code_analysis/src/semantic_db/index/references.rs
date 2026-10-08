//! Per-file and workspace reference indexes.

use hashbrown::HashMap;
use rowan::TextRange;
use smol_str::SmolStr;
use std::sync::Arc;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaLiteralToken};

use crate::semantic_db::SemanticDatabase;
use crate::semantic_db::def::{
    DeclKind, DependencyKey, FileDependencies, LuaMemberKey, OwnerId, SemanticId,
};
use crate::semantic_db::query::{
    file_facts, file_matches_workspace_id, member_ref_from_index_expr, module_file_of,
    resolve_member_id, resolve_name, resolve_owner_ids, resolve_type_def, syntax_tree,
};
use crate::{FileId, WorkspaceId};

/// Per-file reference index: only collects reference points in this file that can resolve to cross-file identities.
///
/// This is the L1 layer of the reference index: each file computes independently and is memoized; editing one file recomputes one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileReferences {
    pub decl_refs: HashMap<SemanticId, Vec<TextRange>>,
    pub member_refs: HashMap<SemanticId, Vec<TextRange>>,
    /// Member definition sites (`T.x = v` / `@field x` / table field keys / method names).
    pub member_defs: HashMap<SemanticId, Vec<TextRange>>,
    /// Canonical identity-level dependencies of this file (P7).
    pub deps: FileDependencies,
}

/// Per-file reference index. Pure lookup in the write-time built cache.
pub(crate) fn build_file_references(db: &SemanticDatabase, file: FileId) -> FileReferences {
    let facts = file_facts(db, file);
    let tree = syntax_tree(db, file);
    let mut out = FileReferences::default();

    // Name use sites -> declarations (and identity-level dependencies).
    for name_use in &facts.name_uses {
        if let Some(decl) = resolve_name(db, file, name_use.syntax.get_range().start()) {
            push_target_dependencies(db, &decl, &mut out.deps);
            out.decl_refs
                .entry(decl)
                .or_default()
                .push(name_use.syntax.get_range());
        } else if let Some(type_def) = resolve_type_def(db, file, name_use.name.clone()) {
            // A bare name may denote a named type without a runtime declaration.
            // Record the type identity so type-surface edits refresh this file.
            push_target_dependencies(db, &type_def.id, &mut out.deps);
        } else {
            // Unresolved global may be defined by a later file write.
            out.deps
                .insert(DependencyKey::Global(name_use.name.clone()));
            // Negative type dependency: a later type with this name may
            // change how this unresolved use resolves.
            out.deps
                .insert(DependencyKey::TypeName(name_use.name.clone()));
        }
    }

    // Member definition sites (so the workspace reference index can give declaration ranges directly without re-scanning members per file).
    for member in &facts.members {
        if let Some(range) = member.id.member_key_range() {
            out.member_defs
                .entry(member.id.clone())
                .or_default()
                .push(range);
        }
    }

    // Index expression use sites -> members.
    for &syntax in &facts.member_uses {
        let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
            continue;
        };
        let Some(index_expr) = LuaIndexExpr::cast(node) else {
            continue;
        };
        let owner_and_name = member_ref_from_index_expr(&facts, &index_expr);
        if let Some((owner, name)) = &owner_and_name {
            insert_member_owner_dependencies(
                db,
                owner,
                &LuaMemberKey::Name(name.clone()),
                &mut out.deps,
            );
        }
        if let Some(member_id) = resolve_member_id(db, &facts, &index_expr) {
            push_target_dependencies(db, &member_id, &mut out.deps);
            let Some(key) = index_expr.get_index_key() else {
                continue;
            };
            let Some(range) = key.get_range() else {
                continue;
            };
            out.member_refs.entry(member_id).or_default().push(range);
        }
    }

    // require("mod") -> module identity dependency.
    if let Some(config) = db.config_input() {
        for call in tree
            .get_red_root()
            .descendants()
            .filter_map(LuaCallExpr::cast)
        {
            if !call.is_require() {
                continue;
            }
            let Some(module_name) = require_module_name_from_call(&call) else {
                continue;
            };
            // Negative dependency: keep the literal (and its last segment)
            // even when resolution fails, so a later module add or rename can
            // refresh this consumer require alias and reference index.
            out.deps
                .insert(DependencyKey::ModuleName(module_name.clone()));
            let last_segment = module_name
                .rsplit(['.', '/', '\\'])
                .next()
                .unwrap_or(module_name.as_str());
            if last_segment != module_name.as_str() {
                out.deps
                    .insert(DependencyKey::ModuleName(SmolStr::new(last_segment)));
            }
            if let Some(module_file) = module_file_of(db, config, module_name) {
                out.deps.insert(DependencyKey::Module(module_file));
            }
        }
    }

    out
}

/// `require("mod")` literal argument -> module name.
fn require_module_name_from_call(call: &LuaCallExpr) -> Option<SmolStr> {
    let arg = call.get_args_list()?.get_args().next()?;
    let LuaExpr::LiteralExpr(literal) = arg else {
        return None;
    };
    let LuaLiteralToken::String(token) = literal.get_literal()? else {
        return None;
    };
    Some(SmolStr::new(token.get_value()))
}

/// Record canonical dependencies for a resolved semantic target.
fn push_target_dependencies(
    db: &SemanticDatabase,
    target: &SemanticId,
    deps: &mut FileDependencies,
) {
    match target {
        SemanticId::Name(name) => deps.insert(DependencyKey::Global(SmolStr::new(name.as_str()))),
        SemanticId::TypeDef(key) => {
            deps.insert(DependencyKey::Type(key.scope, key.full_name.clone()));
            deps.insert(DependencyKey::RuntimeValue(key.full_name.clone()));
        }
        SemanticId::Decl(key) => {
            if let Some(facts) = db.file_facts_of(key.file_id)
                && let Some(decl) = facts.decl_by_id(target)
                && matches!(decl.kind, DeclKind::Global)
            {
                deps.insert(DependencyKey::Global(decl.name.clone()));
            }
        }
        SemanticId::Member(key) => {
            if let Some(facts) = db.file_facts_of(key.file_id)
                && let Some(member) = facts.member_by_id(target)
            {
                insert_member_owner_dependencies(db, &member.owner, &member.key, deps);
            }
        }
        SemanticId::Signature(_) => {}
    }
}

/// Record member-key dependencies for every canonical owner identity that may
/// denote the raw owner (global name, named type, module export, local/table owner).
fn insert_member_owner_dependencies(
    db: &SemanticDatabase,
    owner: &SemanticId,
    key: &LuaMemberKey,
    deps: &mut FileDependencies,
) {
    let mut inserted = false;
    let mut saw_type_owner = false;
    for owner_id in resolve_owner_ids(db, owner) {
        if matches!(&owner_id, OwnerId::Type(..)) {
            saw_type_owner = true;
        }
        deps.insert(DependencyKey::Member(owner_id, key.clone()));
        inserted = true;
    }
    if !inserted {
        deps.insert(DependencyKey::Member(
            OwnerId::Concrete(owner.clone()),
            key.clone(),
        ));
    }
    if !saw_type_owner && let SemanticId::Name(name) = owner {
        deps.insert(DependencyKey::TypeName(SmolStr::new(name.as_str())));
    }
}

/// Workspace-level reference index: aggregates per-file references and keeps a
/// `FileId -> Arc<FileReferences>` map for incremental remove/add.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceReferenceIndex {
    pub decl_refs: HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    pub member_refs: HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    pub member_defs: HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    by_file: HashMap<FileId, Arc<FileReferences>>,
}

impl WorkspaceReferenceIndex {
    pub(crate) fn remove_file(&mut self, file_id: FileId) {
        let Some(refs) = self.by_file.remove(&file_id) else {
            return;
        };
        retain_file_ranges(&mut self.decl_refs, file_id, &refs.decl_refs);
        retain_file_ranges(&mut self.member_refs, file_id, &refs.member_refs);
        retain_file_ranges(&mut self.member_defs, file_id, &refs.member_defs);
    }

    pub(crate) fn add_file(&mut self, file_id: FileId, refs: Arc<FileReferences>) {
        extend_file_ranges(&mut self.decl_refs, file_id, &refs.decl_refs);
        extend_file_ranges(&mut self.member_refs, file_id, &refs.member_refs);
        extend_file_ranges(&mut self.member_defs, file_id, &refs.member_defs);
        self.by_file.insert(file_id, refs);
    }
}

fn extend_file_ranges(
    aggregate: &mut HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    file_id: FileId,
    ranges: &HashMap<SemanticId, Vec<TextRange>>,
) {
    for (target, ranges) in ranges {
        aggregate
            .entry(target.clone())
            .or_default()
            .extend(ranges.iter().map(|range| (file_id, *range)));
    }
}

fn retain_file_ranges(
    aggregate: &mut HashMap<SemanticId, Vec<(FileId, TextRange)>>,
    file_id: FileId,
    ranges: &HashMap<SemanticId, Vec<TextRange>>,
) {
    for target in ranges.keys() {
        if let Some(entries) = aggregate.get_mut(target) {
            entries.retain(|(entry_file, _)| *entry_file != file_id);
            if entries.is_empty() {
                aggregate.remove(target);
            }
        }
    }
}

/// Reference index scoped to a single workspace.
pub(crate) fn build_workspace_reference_index(
    db: &SemanticDatabase,
    ws_id: WorkspaceId,
) -> WorkspaceReferenceIndex {
    #[cfg(test)]
    db.rebuild_metrics
        .full_index_source_scans
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let mut index = WorkspaceReferenceIndex::default();
    for file_id in db.vfs().file_ids() {
        let Some(cache) = db.file_cache(file_id) else {
            continue;
        };
        if !file_matches_workspace_id(db, file_id, ws_id) {
            continue;
        }
        index.add_file(file_id, Arc::clone(&cache.references));
    }
    index
}
