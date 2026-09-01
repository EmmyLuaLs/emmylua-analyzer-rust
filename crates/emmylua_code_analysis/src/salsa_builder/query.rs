//! # Node-keyed derived query layer
//!
//! Salsa's memo table acts as the index. Cross-file references use interned `TypeName`; recursive cycles converge via the native `cycle_fn`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::def::{
    ConstructorAttribute, DeclKind, MemberRef, ModuleExport, ModuleInfo, ModuleNode, ModuleNodeId,
    ModuleVisibility, SalsaGenericParam, SemanticId, TypeDef, TypeDefKind,
};
use super::exports::{file_exports, shard_of};
use super::facts::{FactsBuilder, FileFacts};
use super::inputs::{ConfigInput, SourceFileInput, TypeName, WorkspaceInput};
use super::types::{LiteralShell, PrimitiveType, TableId, TypeCandidate, TypeShell};
use super::{DocumentView, SalsaDatabase, SalsaDb};
use crate::FileId;
use emmylua_parser::{
    BinaryOperator, LineIndex, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaDocType, LuaExpr,
    LuaIndexExpr, LuaLiteralExpr, LuaLiteralToken, LuaParser, LuaReturnStat, LuaSyntaxId,
    LuaSyntaxTree, LuaTypeBinaryOperator, LuaVersionCondition, UnaryOperator,
};
use rowan::{NodeCache, TextSize};

/// Parse. `LuaSyntaxTree` isn't a SalsaValue, so use the `no_eq` + `non_salsa_values` escape hatch
/// (equivalent to rust-analyzer's `parse`: unchanged text -> unchanged input -> no recomputation; `no_eq` has no side effects).
#[salsa::tracked(returns(ref), lru = 2048, no_eq, unsafe(non_salsa_values))]
pub(crate) fn parse(db: &dyn SalsaDb, file: SourceFileInput, config: ConfigInput) -> LuaSyntaxTree {
    let text = file.text(db);
    let mut node_cache = NodeCache::default();
    let parse_config = config.to_parse_config(db, &mut node_cache);
    LuaParser::parse(text, parse_config)
}

/// Per-file line index. Cached as a salsa derived query; no recomputation when the text is unchanged.
#[salsa::tracked(returns(ref), lru = 2048, no_eq, unsafe(non_salsa_values))]
pub(crate) fn line_index(db: &dyn SalsaDb, file: SourceFileInput) -> Arc<LineIndex> {
    let text = file.text(db);
    Arc::new(LineIndex::parse(text))
}

/// Per-file document view. Cached as a salsa derived query; contains URI/Path/Text/LineIndex.
#[salsa::tracked(returns(ref), lru = 2048, no_eq, unsafe(non_salsa_values))]
pub(crate) fn document(db: &dyn SalsaDb, file: SourceFileInput) -> Arc<DocumentView> {
    let file_id = file.file_id(db);
    let path = file.path(db).clone();
    let text: Arc<str> = Arc::from(file.text(db));
    let line_index = line_index(db, file).clone();
    let uri = file.uri(db).clone();
    Arc::new(DocumentView {
        file_id,
        path,
        uri,
        text,
        line_index,
    })
}

/// Per-file minimum fact arena (declarations + scopes + type definitions).
#[salsa::tracked(returns(ref), lru = 2048)]
pub(crate) fn file_facts(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
) -> FileFacts {
    let tree = parse(db, file, config);
    let chunk = tree.get_chunk_node();
    let workspace_id = db
        .workspace_input()
        .and_then(|workspace| file_workspace_id(db, workspace, file.file_id(db)))
        .unwrap_or(WorkspaceId::MAIN);
    FactsBuilder::new(file.file_id(db), workspace_id).build(&chunk, file.text(db))
}

// ──────────────────────────────────────────────
// Cross-file: interned TypeName (types are scoped! identity = (scope, full_name))
// ──────────────────────────────────────────────

use super::def::TypeScope;
use super::def::TypeVisibility;
use super::index::{Bucket, build_buckets, find_bucket};
use crate::WorkspaceId;
use salsa::plumbing::AsId;
use smol_str::SmolStr;

/// Workspace type index: interned `TypeName` (scope+full_name) internal index -> definition.
/// Interning happens at build time; queries binary-search by internal index (interned ids have no `Ord`, so sort by `as_id().index()`).
/// Type index scoped to a single workspace.
///
/// Like `workspace_decl_index_for`, this keeps std / library / main type indexes
/// independent so editing one workspace does not rebuild another.
#[salsa::tracked(returns(ref), lru = 16)]
pub(crate) fn workspace_type_index_for(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    ws_id: WorkspaceId,
) -> WorkspaceTypeIndex {
    let mut entries: Vec<(u32, TypeDef)> = Vec::new();
    for file_id in workspace.file_ids(db).iter().copied() {
        let file_ws = file_workspace_id(db, workspace, file_id);
        let matches = if ws_id == WorkspaceId::REMOTE {
            file_ws.is_none()
        } else {
            file_ws == Some(ws_id)
        };
        if !matches {
            continue;
        }
        let Some(file) = db.file_input(file_id) else {
            continue;
        };
        let exports = file_exports(db, file, config);
        for def in &exports.types {
            let scope = match def.visibility {
                TypeVisibility::Public => TypeScope::Global,
                TypeVisibility::Internal => TypeScope::Internal(ws_id),
                TypeVisibility::Private => TypeScope::File(def.file_id),
            };
            let id = TypeName::new(db, scope, def.full_name.clone());
            entries.push((id.as_id().index(), def.clone()));
        }
    }
    entries.sort_by_key(|(index, _)| *index);
    let mut grouped: Vec<(u32, Vec<TypeDef>)> = Vec::new();
    for (index, def) in entries {
        if let Some(last) = grouped.last_mut()
            && last.0 == index
        {
            last.1.push(def);
        } else {
            grouped.push((index, vec![def]));
        }
    }
    let mut buckets: Vec<Bucket<u32>> = Vec::new();
    let mut bucket_values: Vec<Arc<[TypeDef]>> = Vec::new();
    for (index, defs) in grouped {
        let len = defs.len() as u32;
        bucket_values.push(Arc::from(defs));
        buckets.push(Bucket {
            key: index,
            indices: (0..len).collect(),
        });
    }
    WorkspaceTypeIndex {
        by_index: buckets,
        bucket_values,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct WorkspaceTypeIndex {
    by_index: Vec<Bucket<u32>>,
    bucket_values: Vec<Arc<[TypeDef]>>,
}

impl WorkspaceTypeIndex {
    /// **All** definitions in the bucket (same-name definitions in multiple places, used by duplicate-type checks).
    fn find_all(&self, db: &dyn SalsaDb, scope: TypeScope, full_name: &str) -> Arc<[TypeDef]> {
        let id = TypeName::new(db, scope, SmolStr::new(full_name));
        let key = id.as_id().index();
        let Ok(index) = self
            .by_index
            .binary_search_by(|bucket| bucket.key.cmp(&key))
        else {
            return Arc::from([]);
        };
        self.bucket_values[index].clone()
    }
}

pub(crate) fn all_workspace_ids(db: &dyn SalsaDb, workspace: WorkspaceInput) -> Vec<WorkspaceId> {
    let roots = workspace.roots(db).to_vec();
    let mut ids: Vec<WorkspaceId> = if roots.is_empty() {
        vec![WorkspaceId::MAIN]
    } else {
        roots.iter().map(|root| root.id).collect()
    };
    if !ids.contains(&WorkspaceId::REMOTE) {
        ids.push(WorkspaceId::REMOTE);
    }
    ids
}

fn find_global_types(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    full_name: &str,
) -> Arc<[TypeDef]> {
    let mut out: Vec<TypeDef> = Vec::new();
    for ws_id in all_workspace_ids(db, workspace) {
        let index = workspace_type_index_for(db, workspace, config, ws_id);
        out.extend(index.find_all(db, TypeScope::Global, full_name).iter().cloned());
    }
    Arc::from(out)
}

fn find_internal_types(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    ws: WorkspaceId,
    full_name: &str,
) -> Arc<[TypeDef]> {
    workspace_type_index_for(db, workspace, config, ws).find_all(db, TypeScope::Internal(ws), full_name)
}

fn find_file_types(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    file_id: FileId,
    full_name: &str,
) -> Arc<[TypeDef]> {
    let ws = file_workspace_id(db, workspace, file_id).unwrap_or(WorkspaceId::MAIN);
    workspace_type_index_for(db, workspace, config, ws).find_all(db, TypeScope::File(file_id), full_name)
}

/// Resolve **all definition locations** of a named type in the current file scope
/// (mirrors `resolve_type_def` resolution order, but returns every same-name definition in the bucket; for duplicate-type checks).
#[salsa::tracked(returns(clone))]
pub(crate) fn resolve_type_def_locations(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    file: SourceFileInput,
    bare_name: SmolStr,
) -> Arc<[TypeDef]> {
    let file_id = file.file_id(db);
    let facts = file_facts(db, file, config);
    let ws = file_workspace_id(db, workspace, file_id).unwrap_or(WorkspaceId::MAIN);

    if let Some(ns) = &facts.namespace {
        let full = SmolStr::new(format!("{}.{}", ns, bare_name));
        let defs = find_internal_types(db, workspace, config, ws, &full);
        if !defs.is_empty() {
            return defs;
        }
        let defs = find_global_types(db, workspace, config, &full);
        if !defs.is_empty() {
            return defs;
        }
    }
    for us in &facts.usings {
        let full = SmolStr::new(format!("{}.{}", us, bare_name));
        let defs = find_internal_types(db, workspace, config, ws, &full);
        if !defs.is_empty() {
            return defs;
        }
        let defs = find_global_types(db, workspace, config, &full);
        if !defs.is_empty() {
            return defs;
        }
    }

    let defs = find_file_types(db, workspace, config, file_id, &bare_name);
    if !defs.is_empty() {
        return defs;
    }
    let defs = find_internal_types(db, workspace, config, ws, &bare_name);
    if !defs.is_empty() {
        return defs;
    }
    find_global_types(db, workspace, config, &bare_name)
}

/// Resolve a named type in the current file scope (mirrors the old `find_type_decl` order):
/// 1. file namespace qualification (Internal -> Global); 2. `@using` qualification (Internal -> Global);
/// 3. bare name (**same-file Private** -> Internal -> Global).
#[salsa::tracked(returns(clone))]
pub(crate) fn resolve_type_def(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    file: SourceFileInput,
    bare_name: SmolStr,
) -> Option<TypeDef> {
    resolve_type_def_locations(db, workspace, config, file, bare_name)
        .first()
        .cloned()
}

/// Constructor attribute associated with a type definition.
///
/// Class tables are usually created by factory functions like `meta("ClassName")` with `---@[constructor("init")]`;
/// the attribute belongs to the factory function signature. Here we trace back from the runtime value declaration
/// bound to the type definition to that factory call, so class tables required across files keep constructor-call semantics.
#[salsa::tracked(returns(clone))]
pub(crate) fn constructor_attribute_of_type(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    type_def: SemanticId,
) -> Option<ConstructorAttribute> {
    for resolved in resolve_owner_set(db, workspace, config, type_def.clone()) {
        if matches!(resolved, SemanticId::Decl(_)) {
            if let Some(attribute) = constructor_attribute_of_decl(db, config, resolved.clone()) {
                return Some(attribute);
            }
        }
    }
    None
}

fn constructor_attribute_of_decl(
    db: &dyn SalsaDb,
    config: ConfigInput,
    decl: SemanticId,
) -> Option<ConstructorAttribute> {
    let key = match &decl {
        SemanticId::Decl(key) => key,
        _ => return None,
    };
    let file = db.file_input(key.file_id)?;
    let facts = file_facts(db, file, config);
    let decl = facts.decl_by_id(&decl)?;
    let value_syntax = decl.value_expr_syntax?;
    let tree = parse(db, file, config);
    let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
    let call = LuaCallExpr::cast(node)?;
    let prefix = call.get_prefix_expr()?;
    let LuaExpr::NameExpr(name_expr) = &prefix else {
        return None;
    };
    let callee_decl = resolve_name(db, file, config, name_expr.get_position())?;
    let callee_file = match &callee_decl {
        SemanticId::Decl(key) => key.file_id,
        _ => return None,
    };
    let callee_input = db.file_input(callee_file)?;
    let callee_facts = file_facts(db, callee_input, config);
    let callee = callee_facts.decl_by_id(&callee_decl)?;
    let callee_closure = callee.value_expr_syntax?;
    let signature = callee_facts.signature_by_closure(callee_closure)?;
    let docs = signature.docs.as_ref()?;
    docs.constructor_params
        .first()
        .map(|(_, attribute)| attribute.clone())
}

// ──────────────────────────────────────────────
// Phase 2: workspace member association (after full analysis)
// ──────────────────────────────────────────────

/// Workspace-level member index: owner -> member references.
///
/// Only aggregates member identities from `file_exports` (a small cross-file surface); does not include local structures.
/// Editing one file only recomputes the `export_shard` of that file's shard; this index only re-aggregates exported members, not all local facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMemberIndex {
    by_owner: HashMap<SemanticId, Arc<[MemberRef]>>,
}

/// Member index scoped to a single workspace.
#[salsa::tracked(returns(ref), lru = 16, no_eq, unsafe(non_salsa_values))]
pub(crate) fn workspace_member_index_for(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    ws_id: WorkspaceId,
) -> WorkspaceMemberIndex {
    let mut by_owner: HashMap<SemanticId, Vec<MemberRef>> = HashMap::new();
    for file_id in workspace.file_ids(db).iter().copied() {
        let file_ws = file_workspace_id(db, workspace, file_id);
        let matches = if ws_id == WorkspaceId::REMOTE {
            file_ws.is_none()
        } else {
            file_ws == Some(ws_id)
        };
        if !matches {
            continue;
        }
        let Some(file) = db.file_input(file_id) else {
            continue;
        };
        let exports = file_exports(db, file, config);
        for member in &exports.members {
            by_owner
                .entry(member.owner.clone())
                .or_default()
                .push(MemberRef {
                    file_id: member.file_id,
                    id: member.member.clone(),
                    name: member.key.to_path().into(),
                });
        }
    }
    WorkspaceMemberIndex {
        by_owner: by_owner
            .into_iter()
            .map(|(owner, members)| (owner, Arc::<[MemberRef]>::from(members)))
            .collect(),
    }
}

/// Per-file reference index: only collects reference points in this file that can resolve to cross-file identities.
///
/// This is the L1 layer of the reference index: each file computes independently and is memoized; editing one file recomputes one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileReferences {
    pub decl_refs: HashMap<SemanticId, Vec<rowan::TextRange>>,
    pub member_refs: HashMap<SemanticId, Vec<rowan::TextRange>>,
    /// Member definition sites (`T.x = v` / `@field x` / table field keys / method names).
    pub member_defs: HashMap<SemanticId, Vec<rowan::TextRange>>,
}

/// Per-file reference index (salsa tracked).
#[salsa::tracked(returns(ref), lru = 2048, no_eq, unsafe(non_salsa_values))]
pub(crate) fn file_references(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
) -> FileReferences {
    let facts = file_facts(db, file, config);
    let tree = parse(db, file, config);
    let workspace = db.workspace_input();
    let mut out = FileReferences::default();

    // Name use sites -> declarations.
    for name_use in &facts.name_uses {
        if let Some(decl) = resolve_name(db, file, config, name_use.syntax.get_range().start()) {
            out.decl_refs
                .entry(decl)
                .or_default()
                .push(name_use.syntax.get_range());
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
        if let Some(member_id) = resolve_member_id(db, workspace, config, &facts, &index_expr) {
            let Some(key) = index_expr.get_index_key() else {
                continue;
            };
            let Some(range) = key.get_range() else {
                continue;
            };
            out.member_refs.entry(member_id).or_default().push(range);
        }
    }

    out
}

/// A shard's reference index: only files in the stable shard; editing one file recomputes one shard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceShard {
    pub decl_refs: HashMap<SemanticId, Vec<(FileId, rowan::TextRange)>>,
    pub member_refs: HashMap<SemanticId, Vec<(FileId, rowan::TextRange)>>,
    pub member_defs: HashMap<SemanticId, Vec<(FileId, rowan::TextRange)>>,
}

/// Workspace-level reference index: aggregates `EXPORT_SHARDS` shards.
///
/// Each shard only depends on `file_references` from files in that shard; editing a file only recomputes its shard.
/// This layer just merges a few shard results rather than scanning every file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceReferenceIndex {
    pub decl_refs: HashMap<SemanticId, Vec<(FileId, rowan::TextRange)>>,
    pub member_refs: HashMap<SemanticId, Vec<(FileId, rowan::TextRange)>>,
    pub member_defs: HashMap<SemanticId, Vec<(FileId, rowan::TextRange)>>,
}

#[salsa::tracked(returns(ref), no_eq, unsafe(non_salsa_values))]
pub(crate) fn reference_shard(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    shard: u8,
) -> ReferenceShard {
    let mut out = ReferenceShard::default();
    for file_id in workspace.file_ids(db).iter().copied() {
        if shard_of(file_id) != shard {
            continue;
        }
        let Some(file) = db.file_input(file_id) else {
            continue;
        };
        let refs = file_references(db, file, config);
        for (decl, ranges) in &refs.decl_refs {
            out.decl_refs
                .entry(decl.clone())
                .or_default()
                .extend(ranges.iter().map(|range| (file_id, *range)));
        }
        for (member, ranges) in &refs.member_refs {
            out.member_refs
                .entry(member.clone())
                .or_default()
                .extend(ranges.iter().map(|range| (file_id, *range)));
        }
        for (member, ranges) in &refs.member_defs {
            out.member_defs
                .entry(member.clone())
                .or_default()
                .extend(ranges.iter().map(|range| (file_id, *range)));
        }
    }
    out
}

/// Reference index scoped to a single workspace.
#[salsa::tracked(returns(ref), lru = 16, no_eq, unsafe(non_salsa_values))]
pub(crate) fn workspace_reference_index_for(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    ws_id: WorkspaceId,
) -> WorkspaceReferenceIndex {
    let mut out = WorkspaceReferenceIndex::default();
    for file_id in workspace.file_ids(db).iter().copied() {
        let file_ws = file_workspace_id(db, workspace, file_id);
        let matches = if ws_id == WorkspaceId::REMOTE {
            file_ws.is_none()
        } else {
            file_ws == Some(ws_id)
        };
        if !matches {
            continue;
        }
        let Some(file) = db.file_input(file_id) else {
            continue;
        };
        let refs = file_references(db, file, config);
        for (decl, ranges) in &refs.decl_refs {
            out.decl_refs
                .entry(decl.clone())
                .or_default()
                .extend(ranges.iter().map(|range| (file_id, *range)));
        }
        for (member, ranges) in &refs.member_refs {
            out.member_refs
                .entry(member.clone())
                .or_default()
                .extend(ranges.iter().map(|range| (file_id, *range)));
        }
        for (member, ranges) in &refs.member_defs {
            out.member_defs
                .entry(member.clone())
                .or_default()
                .extend(ranges.iter().map(|range| (file_id, *range)));
        }
    }
    out
}

/// Query-level member resolution: owner/name -> concrete member id.
fn resolve_member_id(
    db: &dyn SalsaDb,
    workspace: Option<WorkspaceInput>,
    config: ConfigInput,
    facts: &FileFacts,
    index_expr: &LuaIndexExpr,
) -> Option<SemanticId> {
    let (owner, name) = member_ref_from_index_expr(facts, index_expr)?;
    let workspace = workspace?;
    for resolved in resolve_owner_set(db, workspace, config, owner) {
        for member in members_of_owner(db, workspace, config, resolved)
            .iter()
            .cloned()
        {
            if member.name == name {
                return Some(member.id);
            }
        }
    }
    None
}

/// Members of an owner `SemanticId` (cross-file; directly scans 64 shard references; body no longer accesses facts per file).
#[salsa::tracked(returns(clone))]
pub(crate) fn members_of_owner(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    owner: SemanticId,
) -> Arc<[MemberRef]> {
    // An owner with a file-local identity (Decl/Member) only has members in its declaring file:
    // read that file's facts directly, avoiding a 64-shard scan and narrowing invalidation to a single file.
    let owner_file = match &owner {
        SemanticId::Decl(key) => Some(key.file_id),
        SemanticId::Member(key) => Some(key.file_id),
        _ => None,
    };
    let Some(owner_file) = owner_file else {
        return return_members_of_owner_scan(db, workspace, config, owner);
    };
    if let Some(file) = db.file_input(owner_file) {
        let facts = file_facts(db, file, config);
        let members = facts
            .members_of_owner(&owner)
            .map(|member| MemberRef {
                file_id: owner_file,
                id: member.id.clone(),
                name: member.key.to_path().into(),
            })
            .collect::<Vec<_>>();
        return Arc::from(members);
    }

    return_members_of_owner_scan(db, workspace, config, owner)
}

fn return_members_of_owner_scan(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    owner: SemanticId,
) -> Arc<[MemberRef]> {
    let mut out: Vec<MemberRef> = Vec::new();
    for ws_id in all_workspace_ids(db, workspace) {
        let index = workspace_member_index_for(db, workspace, config, ws_id);
        if let Some(members) = index.by_owner.get(&owner) {
            out.extend(members.iter().cloned());
        }
    }
    Arc::from(out)
}

/// Member keys of an owner `SemanticId` (cross-file, completion candidates).
/// Union: owner key (runtime `M.x`) + resolved concrete id key (`@field` etc.).
#[salsa::tracked(returns(clone))]
pub(crate) fn member_keys_of_owner(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    owner: SemanticId,
) -> Vec<SmolStr> {
    let mut keys: Vec<SmolStr> = Vec::new();
    // Union of dual identities: same-name type (@field) + runtime value (member declaration).
    for resolved in resolve_owner_set(db, workspace, config, owner.clone()) {
        keys.extend(
            members_of_owner(db, workspace, config, resolved)
                .iter()
                .cloned()
                .map(|member| member.name),
        );
    }
    keys.sort();
    keys.dedup();
    keys
}

/// All type definitions for a given scope + full name (cross-file, reuses the workspace type index).
#[salsa::tracked(returns(clone))]
pub(crate) fn type_defs_in_scope(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    scope: TypeScope,
    full_name: SmolStr,
) -> Arc<[TypeDef]> {
    match scope {
        TypeScope::Global => find_global_types(db, workspace, config, &full_name),
        TypeScope::Internal(ws) => find_internal_types(db, workspace, config, ws, &full_name),
        TypeScope::File(file_id) => find_file_types(db, workspace, config, file_id, &full_name),
    }
}

/// Look up a global type (`@class` etc.) by full name (cross-file, reuses the workspace type index).
#[salsa::tracked(returns(clone))]
pub(crate) fn global_type_by_name(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    full_name: SmolStr,
) -> Option<SemanticId> {
    find_global_types(db, workspace, config, &full_name)
        .first()
        .map(|def| def.id.clone())
}

/// Workspace declaration index: global declarations (by bare-name bucket) + type runtime values + type definition locations.
/// Built by traversing workspace files once; other cross-file queries only consume this index instead of looping over files.
/// Declaration index scoped to a single workspace.
///
/// This is the key isolation layer: `std` / library / main workspaces are indexed
/// separately, so editing main workspace files does not rebuild the std/library
/// indexes.
#[salsa::tracked(returns(ref), lru = 16)]
pub(crate) fn workspace_decl_index_for(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    ws_id: WorkspaceId,
) -> WorkspaceDeclIndex {
    let mut entries: Vec<(SmolStr, FileId, SemanticId)> = Vec::new();
    let mut runtime_values: Vec<(FileId, SmolStr, SemanticId)> = Vec::new();
    let mut type_def_files: Vec<(SemanticId, FileId, SmolStr)> = Vec::new();
    for file_id in workspace.file_ids(db).iter().copied() {
        let file_ws = file_workspace_id(db, workspace, file_id);
        let matches = if ws_id == WorkspaceId::REMOTE {
            file_ws.is_none()
        } else {
            file_ws == Some(ws_id)
        };
        if !matches {
            continue;
        }
        let Some(file) = db.file_input(file_id) else {
            continue;
        };
        let exports = file_exports(db, file, config);
        for global in &exports.globals {
            entries.push((global.name.clone(), global.file_id, global.decl.clone()));
        }
        for def in &exports.types {
            type_def_files.push((def.id.clone(), def.file_id, def.name.clone()));
        }
        runtime_values.extend(
            exports
                .runtime_values
                .iter()
                .map(|(name, decl)| (file_id, name.clone(), decl.clone())),
        );
    }
    let mut entries = entries;
    entries.sort_by(|a, b| (a.0.as_str(), a.1.id).cmp(&(b.0.as_str(), b.1.id)));
    let mut by_name_entries: Vec<(SmolStr, u32)> = entries
        .iter()
        .enumerate()
        .map(|(i, (name, _, _))| (name.clone(), i as u32))
        .collect();
    by_name_entries.sort_by(|a, b| (a.0.as_str(), a.1).cmp(&(b.0.as_str(), b.1)));
    WorkspaceDeclIndex {
        by_name: build_buckets(by_name_entries),
        decls: entries
            .into_iter()
            .map(|(_, file_id, id)| (file_id, id))
            .collect(),
        runtime_values,
        type_def_files,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDeclIndex {
    /// Bare-name bucket -> global declaration index.
    by_name: Vec<Bucket<SmolStr>>,
    /// Global declarations `(file_id, decl_id)`.
    decls: Vec<(FileId, SemanticId)>,
    /// Type runtime values: `(file_id, bare type name, same-name declaration id)` (`local M = {}` implementing `@class M` pattern).
    runtime_values: Vec<(FileId, SmolStr, SemanticId)>,
    /// Type definition -> `(file_id, bare name)`.
    type_def_files: Vec<(SemanticId, FileId, SmolStr)>,
}

impl WorkspaceDeclIndex {
    fn global_decl_named(&self, name: &SmolStr) -> Option<SemanticId> {
        let indices = find_bucket(&self.by_name, name)?;
        indices.first().map(|&i| self.decls[i as usize].1.clone())
    }

    fn runtime_value_in(&self, file_id: FileId, bare_name: &SmolStr) -> Option<SemanticId> {
        self.runtime_values
            .iter()
            .find(|(fid, name, _)| *fid == file_id && name == bare_name)
            .map(|(_, _, id)| id.clone())
    }
}

/// Look up a global variable/function declaration by name (cross-file, reuses the workspace declaration index).
#[salsa::tracked(returns(clone))]
pub(crate) fn global_decl_by_name(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    name: SmolStr,
) -> Option<SemanticId> {
    let roots = workspace.roots(db).to_vec();
    if roots.is_empty() {
        return workspace_decl_index_for(db, workspace, config, WorkspaceId::MAIN)
            .global_decl_named(&name);
    }
    for root in roots {
        if let Some(decl) = workspace_decl_index_for(db, workspace, config, root.id)
            .global_decl_named(&name)
        {
            return Some(decl);
        }
    }
    None
}

// ──────────────────────────────────────────────
// require / module resolution (M0: path-derived module name + suffix matching)
// ──────────────────────────────────────────────

/// Per-file module information (equivalent to a salsa ModuleIndex entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleEntry {
    pub file_id: FileId,
    pub full_module_name: SmolStr,
    pub name: SmolStr,
    pub workspace_id: WorkspaceId,
    pub visible: ModuleVisibility,
    pub is_meta: bool,
    pub version_conds: Vec<LuaVersionCondition>,
}

/// Workspace module index: module name (relative to workspace root) -> file.
/// Module index scoped to a single workspace.
#[salsa::tracked(returns(ref), lru = 16)]
pub(crate) fn workspace_module_index_for(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    ws_id: WorkspaceId,
) -> ModuleIndex {
    let roots = workspace.roots(db).to_vec();
    let ws_roots: Vec<PathBuf> = roots
        .iter()
        .filter(|root| root.id == ws_id)
        .map(|root| root.root.clone())
        .collect();
    let paths: Vec<PathBuf> = workspace
        .file_ids(db)
        .iter()
        .filter_map(|&file_id| db.file_input(file_id))
        .filter_map(|file| file.path(db).clone())
        .collect();
    let fallback_root = if roots.is_empty() {
        config
            .main_root(db)
            .clone()
            .or_else(|| common_path_root(&paths))
    } else {
        None
    };

    let mut entries: Vec<ModuleEntry> = Vec::new();
    let mut by_path: HashMap<PathBuf, FileId> = HashMap::new();
    let mut module_name_to_file_ids: HashMap<SmolStr, Vec<FileId>> = HashMap::new();

    for file_id in workspace.file_ids(db).iter().copied() {
        let file_ws = file_workspace_id(db, workspace, file_id);
        let matches = if ws_id == WorkspaceId::REMOTE {
            file_ws.is_none()
        } else {
            file_ws == Some(ws_id)
        };
        if !matches {
            continue;
        }
        let Some(file) = db.file_input(file_id) else {
            continue;
        };
        let Some(path) = file.path(db) else {
            continue;
        };
        by_path.insert(normalize_path(&path), file_id);

        let root_path = if !roots.is_empty() {
            roots.iter().find(|root| root.id == ws_id).map(|root| root.root.clone())
        } else {
            fallback_root.clone()
        };
        let Some(root_path) = root_path else {
            continue;
        };
        let Some(full_module_name) = module_name_from_path(&path, Some(&root_path)) else {
            continue;
        };
        let facts = file_facts(db, file, config);
        let name = SmolStr::new(
            full_module_name
                .rsplit('.')
                .next()
                .unwrap_or(&full_module_name),
        );
        module_name_to_file_ids
            .entry(name.clone())
            .or_default()
            .push(file_id);
        entries.push(ModuleEntry {
            file_id,
            full_module_name,
            name,
            workspace_id: ws_id,
            visible: facts.module_visibility,
            is_meta: facts.is_meta,
            version_conds: facts.version_conds.clone(),
        });
    }

    entries.sort_by(|a, b| {
        a.full_module_name
            .as_str()
            .cmp(b.full_module_name.as_str())
            .then_with(|| a.workspace_id.id.cmp(&b.workspace_id.id))
            .then_with(|| a.file_id.id.cmp(&b.file_id.id))
    });
    for file_ids in module_name_to_file_ids.values_mut() {
        file_ids.sort_unstable();
        file_ids.dedup();
    }

    let (nodes, root) = build_module_tree(&entries, ws_id);

    ModuleIndex {
        entries,
        by_path,
        module_name_to_file_ids,
        roots: ws_roots,
        nodes,
        root,
    }
}

fn build_module_tree(entries: &[ModuleEntry], ws_id: WorkspaceId) -> (HashMap<ModuleNodeId, ModuleNode>, ModuleNodeId) {
    let root = ModuleNodeId { id: 0, workspace_id: ws_id };
    let mut nodes = HashMap::new();
    nodes.insert(
        root,
        ModuleNode {
            children: Vec::new(),
            file_ids: Vec::new(),
            parent: None,
        },
    );
    let mut next_id = 1u32;

    for entry in entries {
        let parts: Vec<&str> = entry.full_module_name.split('.').collect();
        let mut current = root;
        for (index, part) in parts.iter().enumerate() {
            let is_last = index + 1 == parts.len();
            let child_id = {
                let node = nodes.get_mut(&current).expect("module node exists");
                if let Some(position) = node
                    .children
                    .iter()
                    .position(|(name, _)| name.as_str() == *part)
                {
                    node.children[position].1
                } else {
                    let id = ModuleNodeId { id: next_id, workspace_id: ws_id };
                    next_id += 1;
                    node.children.push((SmolStr::new(*part), id));
                    id
                }
            };
            nodes.entry(child_id).or_insert_with(|| ModuleNode {
                children: Vec::new(),
                file_ids: Vec::new(),
                parent: Some(current),
            });
            if is_last {
                nodes
                    .get_mut(&child_id)
                    .expect("module node just inserted")
                    .file_ids
                    .push(entry.file_id);
            }
            current = child_id;
        }
    }

    (nodes, root)
}

/// Choose the workspace root that contains `path`.
///
/// Prefer the most specific (shortest relative path); tie-break in favor of non-main roots (old LuaModuleIndex semantics).
pub(crate) fn find_workspace_root(
    roots: &[crate::salsa_builder::inputs::WorkspaceRoot],
    path: &Path,
) -> Option<(WorkspaceId, PathBuf)> {
    let mut best: Option<(usize, WorkspaceId, PathBuf)> = None;
    for root in roots {
        let Ok(rel) = path.strip_prefix(&root.root) else {
            continue;
        };
        if !root.import.includes_path(rel) {
            continue;
        }
        let rel_len = rel.components().count();
        let replace = match &best {
            None => true,
            Some((best_len, best_id, _)) => {
                rel_len < *best_len
                    || (rel_len == *best_len && root.id.is_main() && !best_id.is_main())
            }
        };
        if replace {
            best = Some((rel_len, root.id, root.root.clone()));
        }
    }
    best.map(|(_, id, root)| (id, root))
}

/// File -> its workspace.
#[salsa::tracked(returns(copy))]
pub(crate) fn file_workspace_id(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    file_id: FileId,
) -> Option<WorkspaceId> {
    let file = db.file_input(file_id)?;
    let path = file.path(db).clone()?;
    let roots = workspace.roots(db).to_vec();
    if roots.is_empty() {
        return Some(WorkspaceId::MAIN);
    }
    find_workspace_root(&roots, &path).map(|(id, _)| id)
}

/// Module name -> file. Resolution order: module_map rewrite -> exact match -> require pattern (`?.lua`/`?/init.lua`).
#[salsa::tracked(returns(clone))]
pub(crate) fn module_file_of(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    module_name: SmolStr,
) -> Option<FileId> {
    let mut name = module_name.replace('\\', ".");
    // module_map rewrite rules (config order; every matching rule is applied, consistent with the old replace_module_path).
    for (pattern, replace) in config.module_replace(db) {
        if let Ok(regex) = regex::Regex::new(pattern.as_str())
            && regex.is_match(&name)
        {
            name = regex.replace(&name, replace.as_str()).into_owned();
        }
    }
    let patterns = config.module_patterns(db).to_vec();
    for ws_id in all_workspace_ids(db, workspace) {
        let index = workspace_module_index_for(db, workspace, config, ws_id);
        // Paths still containing `/` after module_map rewriting (`signalstrings/signalstrings.lua`)
        // first try exact literal relative-path matching, then `?` pattern resolution.
        if name.contains('/') {
            if let Some(file_id) = index.resolve_literal_path(&name) {
                return Some(file_id);
            }
        }
        let normalized = name.replace('/', ".");
        if let Some(file_id) = index.exact(&normalized) {
            return Some(file_id);
        }
        if let Some(file_id) = index.fuzzy(&normalized) {
            return Some(file_id);
        }
        if let Some(file_id) = index.resolve_by_pattern(&normalized, &patterns) {
            return Some(file_id);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleIndex {
    /// Module entries sorted by `(full_module_name, workspace_id, file_id)`.
    entries: Vec<ModuleEntry>,
    /// Normalized path -> file (for pattern / literal path resolution).
    by_path: HashMap<PathBuf, FileId>,
    /// Module last segment -> files (for fuzzy search).
    module_name_to_file_ids: HashMap<SmolStr, Vec<FileId>>,
    /// All workspace root paths (for pattern resolution).
    roots: Vec<PathBuf>,
    /// Module tree nodes.
    nodes: HashMap<ModuleNodeId, ModuleNode>,
    /// Module tree root node.
    root: ModuleNodeId,
}

impl ModuleIndex {
    fn exact(&self, name: &str) -> Option<FileId> {
        let mut first = None;
        for entry in &self.entries {
            if entry.full_module_name.as_str() != name {
                continue;
            }
            if first.is_none() {
                first = Some(entry.file_id);
            }
            if !entry.visible.is_hidden() {
                return Some(entry.file_id);
            }
        }
        first
    }

    /// Match a literal relative path containing `/` (`signalstrings/signalstrings.lua`).
    fn resolve_literal_path(&self, name: &str) -> Option<FileId> {
        let rel = name.replace('\\', "/");
        for root in &self.roots {
            let candidate = root.join(&rel);
            if let Some(file_id) = self.by_path.get(&normalize_path(&candidate)) {
                return Some(*file_id);
            }
        }
        None
    }

    /// Suffix fuzzy matching (old `fuzzy_find_module`): `event` matches `lua.cmp.utils.event`;
    /// prefer the fewest leading segments, then take one stably in module-name lexicographic order.
    fn fuzzy(&self, name: &str) -> Option<FileId> {
        let last_name = name.rsplit('.').next().unwrap_or(name);
        let file_ids = self.module_name_to_file_ids.get(last_name)?;
        let suffix = format!(".{name}");
        file_ids
            .iter()
            .filter_map(|&file_id| {
                let entry = self.entries.iter().find(|e| e.file_id == file_id)?;
                let full_module_name = entry.full_module_name.as_str();
                let leading_segment_count = if full_module_name == name {
                    Some(0)
                } else {
                    full_module_name
                        .strip_suffix(&suffix)
                        .map(|prefix| prefix.split('.').count())
                }?;
                Some((leading_segment_count, entry))
            })
            .min_by(|(left_count, left), (right_count, right)| {
                left_count
                    .cmp(right_count)
                    .then_with(|| left.full_module_name.cmp(&right.full_module_name))
            })
            .map(|(_, entry)| entry.file_id)
    }

    /// Exact-match candidate paths under each root using require patterns (`?` -> `a/b`).
    fn resolve_by_pattern(&self, name: &str, patterns: &[SmolStr]) -> Option<FileId> {
        let rel = name.replace('.', "/");
        for root in &self.roots {
            for pattern in patterns {
                let candidate = pattern.replace('?', &rel);
                let candidate = root.join(candidate);
                if let Some(file_id) = self.by_path.get(&normalize_path(&candidate)) {
                    return Some(*file_id);
                }
            }
        }
        None
    }

    pub(crate) fn find_module_node(&self, module_path: &str) -> Option<ModuleNodeId> {
        if module_path.is_empty() {
            return Some(self.root);
        }
        let mut current = self.root;
        for part in module_path.replace(['\\', '/'], ".").split('.') {
            let node = self.nodes.get(&current)?;
            let child_id = node
                .children
                .iter()
                .find(|(name, _)| name.as_str() == part)
                .map(|(_, id)| *id)?;
            current = child_id;
        }
        Some(current)
    }

    pub(crate) fn module_node(&self, id: ModuleNodeId) -> Option<&ModuleNode> {
        self.nodes.get(&id)
    }

    pub(crate) fn module_file_ids(&self, id: ModuleNodeId) -> Option<&[FileId]> {
        self.nodes.get(&id).map(|node| node.file_ids.as_slice())
    }

    pub(crate) fn module_info(&self, file_id: FileId) -> Option<ModuleInfo> {
        let entry = self.entries.iter().find(|entry| entry.file_id == file_id)?;
        Some(ModuleInfo {
            file_id: entry.file_id,
            full_module_name: entry.full_module_name.clone(),
            name: entry.name.clone(),
            visible: entry.visible,
            workspace_id: entry.workspace_id,
            is_meta: entry.is_meta,
            version_conds: entry.version_conds.clone(),
            export_type: None,
        })
    }
}

/// File path -> module name: relative to workspace root, strip `.lua`, `init.lua` -> parent dir, `/` -> `.`.
pub(crate) fn module_name_from_path(path: &Path, root: Option<&Path>) -> Option<SmolStr> {
    let rel = match root {
        Some(root) => path.strip_prefix(root).unwrap_or(path),
        None => path,
    };
    let text = rel.to_string_lossy().replace('\\', "/");
    let text = text.strip_suffix(".lua").unwrap_or(&text);
    let text = text.strip_suffix("/init").unwrap_or(&text);
    if text.is_empty() {
        return None;
    }
    Some(SmolStr::new(text.replace('/', ".")))
}

/// Common prefix of all files' parent directories as the fallback workspace root.
fn common_path_root(paths: &[PathBuf]) -> Option<PathBuf> {
    let parents: Vec<PathBuf> = paths
        .iter()
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect();
    let first = parents.first()?;
    let mut common = first.clone();
    for p in &parents[1..] {
        let a: Vec<_> = common.components().collect();
        let b: Vec<_> = p.components().collect();
        let n = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
        let mut next = PathBuf::new();
        for comp in &a[..n] {
            next.push(comp.as_os_str());
        }
        common = next;
        if common.as_os_str().is_empty() {
            break;
        }
    }
    (!common.as_os_str().is_empty()).then_some(common)
}

/// Normalize a path (strip trailing separators, unify slashes) for exact by_path matching.
fn normalize_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        // Windows filesystems are case-insensitive: normalize module-name index to lowercase so require("Module") can match module.lua.
        let lowered = path.to_string_lossy().to_lowercase();
        Path::new(&lowered).components().collect()
    }
    #[cfg(not(windows))]
    {
        path.components().collect()
    }
}

/// Phase 2 association: resolve `Name("a.b")` to a real definition (type/variable/member chain).
/// `Decl`/`TypeDef`/`Member` are already concrete and returned as-is.
#[salsa::tracked(returns(clone), cycle_initial = owner_cycle_initial, cycle_fn = owner_cycle_recover)]
pub(crate) fn resolve_owner(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    owner: SemanticId,
) -> Option<SemanticId> {
    let SemanticId::Name(name) = &owner else {
        return Some(owner.clone());
    };
    if let Some(dot) = name.rfind('.') {
        // "a.b" -> resolve "a" first, then take its member "b".
        let head = SmolStr::new(&name[..dot]);
        let tail = &name[dot + 1..];
        let resolved = resolve_owner(db, workspace, config, SemanticId::name(head.clone()))?;
        // Members are looked up by union: declared under the head's name key (same-file `M.N = {}`) or under a concrete id key (`@field` etc.).
        let mut members =
            members_of_owner(db, workspace, config, SemanticId::name(head.clone())).to_vec();
        members.extend(
            members_of_owner(db, workspace, config, resolved)
                .iter()
                .cloned(),
        );
        members
            .into_iter()
            .find(|member| member.name.as_str() == tail)
            .map(|member| member.id)
    } else {
        global_type_by_name(db, workspace, config, SmolStr::new(name.as_str()))
            .or_else(|| global_decl_by_name(db, workspace, config, SmolStr::new(name.as_str())))
    }
}

fn owner_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _workspace: WorkspaceInput,
    _config: ConfigInput,
    _owner: SemanticId,
) -> Option<SemanticId> {
    None
}

fn owner_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &Option<SemanticId>,
    _value: Option<SemanticId>,
    _workspace: WorkspaceInput,
    _config: ConfigInput,
    _owner: SemanticId,
) -> Option<SemanticId> {
    None
}

/// Phase 2 association: resolve an owner to an **identity set** (dual identity: same-name type + runtime value).
/// `Name("M")` -> `{TypeDef(M), Decl(M)}`; member lookup uses the union across sets.
/// For name chains (`a.b`), recursively take members along each head identity.
#[salsa::tracked(returns(clone))]
pub(crate) fn resolve_owner_set(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    owner: SemanticId,
) -> Vec<SemanticId> {
    match &owner {
        SemanticId::Name(name) => {
            // Keep the original Name: global runtime members are declared under Name keys.
            let mut out: Vec<SemanticId> = vec![owner.clone()];
            let name_str = SmolStr::new(name.as_str());
            // Type (global, cross-file).
            if let Some(type_def) = global_type_by_name(db, workspace, config, name_str.clone()) {
                push_unique(&mut out, type_def);
            }
            // Global variable.
            if let Some(decl) = global_decl_by_name(db, workspace, config, name_str.clone()) {
                push_unique(&mut out, decl);
            }
            // Type runtime value: same-name decl in the file declaring the same-name type (`local M = {}` pattern).
            let roots = workspace.roots(db).to_vec();
            let ws_ids: Vec<WorkspaceId> = if roots.is_empty() {
                vec![WorkspaceId::MAIN]
            } else {
                roots.iter().map(|root| root.id).collect()
            };
            let mut runtime_decls: Vec<SemanticId> = Vec::new();
            for ws_id in ws_ids {
                let index = workspace_decl_index_for(db, workspace, config, ws_id);
                runtime_decls.extend(
                    index
                        .runtime_values
                        .iter()
                        .filter(|(_, bare_name, _)| bare_name == name.as_str())
                        .map(|(_, _, decl_id)| decl_id.clone()),
                );
            }
            if let Some(decl) = global_decl_by_name(db, workspace, config, name_str.clone()) {
                runtime_decls.push(decl);
            }
            // `---@class MyClass` + `x = {}`: differently-named runtime tables are also associated with the class definition by owner_syntax.
            for decl_id in runtime_decls {
                push_unique(&mut out, decl_id.clone());
                let SemanticId::Decl(decl_key) = &decl_id else {
                    continue;
                };
                let Some(file) = db.file_input(decl_key.file_id) else {
                    continue;
                };
                let facts = file_facts(db, file, config);
                let Some(decl) = facts.decl_by_id(&decl_id) else {
                    continue;
                };
                let Some(owner_syntax) = decl.owner_syntax else {
                    continue;
                };
                for def in facts.type_defs.iter().filter(|def| {
                    def.owner_syntax == Some(owner_syntax)
                        && matches!(def.kind, TypeDefKind::Class | TypeDefKind::Enum)
                }) {
                    push_unique(&mut out, def.id.clone());
                }
            }
            // Name chain: recursively take members along each head identity.
            if let Some(dot) = name.rfind('.') {
                let head = SmolStr::new(&name[..dot]);
                let tail = &name[dot + 1..];
                for head_owner in resolve_owner_set(db, workspace, config, SemanticId::name(head)) {
                    for member in members_of_owner(db, workspace, config, head_owner)
                        .iter()
                        .cloned()
                    {
                        if member.name.as_str() == tail {
                            push_unique(&mut out, member.id);
                        }
                    }
                }
            }
            out
        }
        SemanticId::TypeDef(_) => {
            let mut out = vec![owner.clone()];
            // Type runtime value: same-name decl in the file that declares this type.
            let roots = workspace.roots(db).to_vec();
            let ws_ids: Vec<WorkspaceId> = if roots.is_empty() {
                vec![WorkspaceId::MAIN]
            } else {
                roots.iter().map(|root| root.id).collect()
            };
            let mut found: Option<(FileId, SmolStr)> = None;
            for ws_id in ws_ids {
                let index = workspace_decl_index_for(db, workspace, config, ws_id);
                if let Some((_, file_id, bare_name)) =
                    index.type_def_files.iter().find(|(id, _, _)| id == &owner)
                {
                    found = Some((*file_id, bare_name.clone()));
                    if let Some(decl_id) = index.runtime_value_in(*file_id, bare_name) {
                        push_unique(&mut out, decl_id);
                    }
                    break;
                }
            }
            // `---@class A` followed by `local m = {}`: associate the runtime-value decl by owner_syntax.
            if let Some((file_id, bare_name)) = found
                && let Some(member_file) = db.file_input(file_id)
            {
                let facts = file_facts(db, member_file, config);
                let def = facts.type_defs.iter().find(|def| def.name == bare_name);
                if let Some(def) = def
                    && let Some(owner_syntax) = def.owner_syntax
                {
                    for decl in &facts.decls {
                        if decl.owner_syntax == Some(owner_syntax) {
                            push_unique(&mut out, decl.id.clone());
                        }
                    }
                }
            }
            out
        }
        other => vec![other.clone()],
    }
}

fn push_unique(out: &mut Vec<SemanticId>, id: SemanticId) {
    if !out.contains(&id) {
        out.push(id);
    }
}

// ──────────────────────────────────────────────
// L3 semantics: declaration types (with cycle convergence)
// ──────────────────────────────────────────────

/// Type of a declaration. Recursive dependencies (mutual references) converge via salsa's native fixed point.
/// Priority: `---@type` annotation -> initializer expression.
/// Keyed by file: when an initializer references cross-file members, dependence on `workspace_input` keeps memoization stable.
#[salsa::tracked(returns(clone), lru = 4096, cycle_initial = type_cycle_initial, cycle_fn = type_cycle_recover)]
pub(crate) fn decl_type(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    decl: SemanticId,
) -> TypeShell {
    let facts = file_facts(db, file, config);
    let workspace = db.workspace_input();
    let Some(decl) = facts.decl_by_id(&decl) else {
        return TypeShell::unknown();
    };

    if let Some(type_syntax) = decl.doc_type_syntax {
        let shell = lower_doc_type(db, workspace, file, config, type_syntax, &[]);
        if !shell.is_unknown() {
            return shell;
        }
    }

    // `---@module "name"`: project directly to a module reference.
    if let Some(module_path) = &decl.module_path
        && let Some(workspace) = workspace
        && let Some(module_file) = module_file_of(db, workspace, config, module_path.clone())
    {
        return TypeShell::from_module_ref(module_file);
    }

    // Parameter declaration: `---@param` annotation (belongs to the closure signature, matched by name).
    if matches!(decl.kind, DeclKind::Param)
        && let Some(sig) = facts
            .signatures
            .iter()
            .find(|sig| sig.param_names.contains(&decl.name))
        && let Some(docs) = &sig.docs
        && let Some((_, type_syntax)) = docs.param_types.iter().find(|(name, _)| name == &decl.name)
    {
        let generics = docs.generic_params.as_slice();
        let mut shell = lower_doc_type(db, workspace, file, config, *type_syntax, generics);
        if !shell.is_unknown() {
            // `---@param name? T`: the parameter type must include nil.
            if docs.nullable_params.iter().any(|n| n == &decl.name) {
                shell.merge(&TypeShell::from_primitive(PrimitiveType::Nil));
            }
            return shell;
        }
    }

    // `for k, v in pairs(x)`: take types from the iterator function's return slots (owner is ForRangeStat).
    if matches!(decl.kind, DeclKind::Local { is_iter: true, .. })
        && let Some(shell) = iter_slot_type(db, &facts, workspace, file, config, &decl)
        && !shell.is_unknown()
    {
        return shell;
    }

    if let Some(value_expr_syntax) = decl.value_expr_syntax {
        let tree = parse(db, file, config);
        if let Some(expr) = find_expr_by_syntax_id(&tree, &value_expr_syntax) {
            let shell = expr_type(db, &facts, workspace, file, config, expr);
            if !shell.is_unknown() {
                // Globals cannot carry generic parameters not instantiated inside the function body (the `a` in `function f(x) a = x end` is unknown outside).
                if matches!(decl.kind, DeclKind::Global) {
                    let generic_names: std::collections::HashSet<&str> = facts
                        .signatures
                        .iter()
                        .filter_map(|sig| sig.docs.as_ref())
                        .flat_map(|docs| docs.generic_params.iter().map(|g| g.name.as_str()))
                        .collect();
                    if shell.candidates.iter().any(|candidate| {
                        matches!(
                            candidate,
                            TypeCandidate::Generic(name) if generic_names.contains(name.as_str())
                        )
                    }) {
                        return TypeShell::unknown();
                    }
                }
                return shell;
            }
        }
    }

    TypeShell::unknown()
}

/// Iterator slot types for `for k, v in pairs(x)`.
///
/// Only recognizes `pairs/ipairs/next(x)` where x is a named type / generic instance: reads the returned list from
/// the `__pairs`/`__ipairs` function signature's `---@return fun(): K, V` and projects it by slot (preserving order,
/// not lost through TypeShell's candidate set).
fn iter_slot_type(
    db: &dyn SalsaDb,
    facts: &FileFacts,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    decl: &crate::salsa_builder::def::Decl,
) -> Option<TypeShell> {
    let owner = decl.owner_syntax?;
    let tree = parse(db, file, config);
    let node = owner.to_node_from_root(&tree.get_red_root())?;
    let stat = emmylua_parser::LuaForRangeStat::cast(node)?;
    let vars = stat.get_var_name_list().collect::<Vec<_>>();
    let index = vars
        .iter()
        .position(|var| var.get_name_text() == decl.name.as_str())?;
    let iter_expr = stat.get_expr_list().next()?;
    let LuaExpr::CallExpr(call) = iter_expr else {
        return None;
    };
    let member_name = match call.get_prefix_expr()? {
        LuaExpr::NameExpr(name) => match name.get_name_text()?.as_str() {
            "pairs" | "next" => "__pairs",
            "ipairs" => "__ipairs",
            _ => return None,
        },
        _ => return None,
    };
    let arg = call.get_args_list()?.get_args().next()?;
    let arg_shell = expr_type(db, facts, workspace, file, config, arg);

    for candidate in &arg_shell.candidates {
        let (def, generic_args) = match candidate {
            TypeCandidate::Named(name) => {
                let def =
                    resolve_type_def(db, workspace?, config, file, SmolStr::new(name.as_str()))?;
                (def, Vec::new())
            }
            TypeCandidate::GenericInstance(ins) => {
                let def = resolve_type_def(
                    db,
                    workspace?,
                    config,
                    file,
                    SmolStr::new(ins.name.as_str()),
                )?;
                (def, ins.args.clone())
            }
            _ => continue,
        };
        let mut member_refs = members_of_owner(db, workspace?, config, def.id.clone()).to_vec();
        for resolved in resolve_owner_set(db, workspace?, config, def.id.clone()) {
            member_refs.extend(
                members_of_owner(db, workspace?, config, resolved)
                    .iter()
                    .cloned(),
            );
        }
        for member_ref in member_refs
            .iter()
            .filter(|member| member.name == member_name)
        {
            let Some(member_file) = db.file_input(member_ref.file_id) else {
                continue;
            };
            let member_facts = file_facts(db, member_file, config);
            let Some(member) = member_facts.member_by_id(&member_ref.id) else {
                continue;
            };
            let Some(closure_syntax) = member.value_syntax else {
                continue;
            };
            let Some(signature) = member_facts.signature_by_closure(closure_syntax) else {
                continue;
            };
            let Some(docs) = &signature.docs else {
                continue;
            };
            // Generator signature: `---@return fun(): integer, T` (returns stores the fun type node).
            let Some(return_syntax) = docs.returns.first() else {
                continue;
            };
            let member_tree = parse(db, member_file, config);
            let Some(return_node) = return_syntax.to_node_from_root(&member_tree.get_red_root())
            else {
                continue;
            };
            let Some(LuaDocType::Func(func)) = LuaDocType::cast(return_node) else {
                continue;
            };
            let Some(return_list) = func.get_return_type_list() else {
                continue;
            };
            let mut slots: Vec<TypeShell> = Vec::new();
            for ret in return_list.get_return_type_list() {
                if let (_, Some(ret_type)) = ret.get_name_and_type() {
                    slots.push(lower_doc_type_node(
                        db,
                        workspace,
                        member_file,
                        config,
                        &def.generic_params,
                        &ret_type,
                    ));
                }
            }
            if let Some(slot) = slots.get(index) {
                let substituted = substitute_generics(slot, &def.generic_params, &generic_args);
                if !substituted.is_unknown() {
                    return Some(substituted);
                }
            }
        }
    }
    None
}

/// Lower a doc type node to `TypeShell`.
/// `generics` = generic params in the current scope (`T` -> `Generic(T)`); named types resolve to TypeDef (cross-file).
pub(crate) fn lower_doc_type(
    db: &dyn SalsaDb,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    type_syntax: LuaSyntaxId,
    generics: &[SalsaGenericParam],
) -> TypeShell {
    let tree = parse(db, file, config);
    let root = tree.get_red_root();
    let Some(node) = type_syntax.to_node_from_root(&root) else {
        return TypeShell::unknown();
    };
    let Some(doc_type) = LuaDocType::cast(node) else {
        return TypeShell::unknown();
    };
    lower_doc_type_node(db, workspace, file, config, generics, &doc_type)
}

fn lower_doc_type_node(
    db: &dyn SalsaDb,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    generics: &[SalsaGenericParam],
    doc_type: &LuaDocType,
) -> TypeShell {
    match doc_type {
        LuaDocType::Name(name_type) => match name_type.get_name_text() {
            Some(name) => {
                // Generic parameters take precedence (shadow same-name types).
                if generics.iter().any(|g| g.name == name) {
                    TypeShell::from_generic(&name)
                } else if let Some(primitive) = primitive_from_name(&name) {
                    primitive
                } else if let Some(workspace) = workspace
                    && let Some(def) =
                        resolve_type_def(db, workspace, config, file, SmolStr::new(&name))
                {
                    TypeShell::from_name(def.full_name.as_str())
                } else {
                    TypeShell::from_name(&name)
                }
            }
            None => TypeShell::unknown(),
        },
        LuaDocType::Literal(literal) => match literal.get_literal() {
            Some(LuaLiteralToken::String(str)) => {
                TypeShell::from_literal(LiteralShell::String(SmolStr::new(str.get_value())))
            }
            Some(LuaLiteralToken::Number(number)) => match number.get_number_value() {
                emmylua_parser::NumberResult::Int(i) => {
                    TypeShell::from_literal(LiteralShell::Integer(i))
                }
                emmylua_parser::NumberResult::Uint(u) => {
                    TypeShell::from_literal(LiteralShell::Integer(u as i64))
                }
                // Float constants preserve bit patterns (f64 has no `Ord`; `LiteralShell::Float(u64)`).
                emmylua_parser::NumberResult::Float(f) => {
                    TypeShell::from_literal(LiteralShell::Float(f.to_bits()))
                }
                emmylua_parser::NumberResult::Number => {
                    TypeShell::from_primitive(PrimitiveType::Number)
                }
            },
            Some(LuaLiteralToken::Bool(bool_token)) => {
                TypeShell::from_literal(LiteralShell::Boolean(bool_token.is_true()))
            }
            Some(LuaLiteralToken::Nil(_)) => TypeShell::from_literal(LiteralShell::Nil),
            _ => TypeShell::unknown(),
        },
        LuaDocType::Array(array) => array
            .get_type()
            .map(|base| {
                TypeShell::from_array(lower_doc_type_node(
                    db, workspace, file, config, generics, &base,
                ))
            })
            .unwrap_or_else(|| TypeShell::from_primitive(PrimitiveType::Table)),
        LuaDocType::Variadic(variadic) => variadic
            .get_type()
            .map(|inner| {
                TypeShell::from_variadic(lower_doc_type_node(
                    db, workspace, file, config, generics, &inner,
                ))
            })
            .unwrap_or_else(TypeShell::unknown),
        LuaDocType::Tuple(tuple) => {
            let types = tuple
                .get_types()
                .map(|item| lower_doc_type_node(db, workspace, file, config, generics, &item))
                .collect();
            TypeShell::from_tuple(types)
        }
        LuaDocType::Object(object) => {
            // An empty object literal `{}` is a structural type and must not be lowered to broad `table`;
            // otherwise flow analysis would narrow `myenum|{}`'s table branch to `Table` and lose `{}`.
            if object.get_fields().next().is_none() {
                TypeShell::from_primitive(PrimitiveType::EmptyObject)
            } else {
                TypeShell::from_primitive(PrimitiveType::Table)
            }
        }
        LuaDocType::Generic(generic_type) => {
            // `Box<number>`: base type name + arguments (generic instantiation).
            if let Some(name) = generic_type.get_name_type().and_then(|n| n.get_name_text()) {
                let mut args = Vec::new();
                if let Some(list) = generic_type.get_generic_types() {
                    for arg in list.get_types() {
                        args.push(lower_doc_type_node(
                            db, workspace, file, config, generics, &arg,
                        ));
                    }
                }
                TypeShell::from_generic_instance(&name, args)
            } else {
                TypeShell::unknown()
            }
        }
        LuaDocType::StrTpl(str_tpl) => {
            // `` `T` ``: string argument replaces the placeholder name (`xxx.`T`` -> prefix "xxx.", `` `T`.xxx `` -> suffix ".xxx").
            let (prefix, name, suffix) = str_tpl.get_name();
            let tpl_index = name
                .as_deref()
                .and_then(|n| generics.iter().position(|g| g.name == n))
                .map(|idx| idx as u32);
            TypeShell::from_str_tpl(
                &prefix.unwrap_or_default(),
                &name.unwrap_or_default(),
                tpl_index,
                &suffix.unwrap_or_default(),
            )
        }
        LuaDocType::Func(func_type) => {
            // `fun<T, U>(...)`: merge generic declarations into scope; `T` in params/returns -> `Generic("T")`.
            let mut local_generics: Vec<SalsaGenericParam> = generics.to_vec();
            let mut fun_generics: Vec<SmolStr> = Vec::new();
            if let Some(decl_list) = func_type.get_generic_decl_list() {
                for decl in decl_list.get_generic_decl() {
                    if let Some(token) = decl.get_name_token() {
                        let name = token.get_name_text().to_string();
                        local_generics.push(SalsaGenericParam::new(
                            SmolStr::new(&name),
                            None,
                            None,
                            false,
                            false,
                        ));
                        fun_generics.push(SmolStr::new(&name));
                    }
                }
            }
            let mut params = Vec::new();
            let mut param_names = Vec::new();
            let mut is_variadic = false;
            for param in func_type.get_params() {
                if param.is_dots() {
                    is_variadic = true;
                }
                param_names.push(
                    param
                        .get_name_token()
                        .map(|token| SmolStr::new(token.get_name_text()))
                        .or_else(|| param.is_dots().then(|| SmolStr::new("...")))
                        .unwrap_or_default(),
                );
                if let Some(param_type) = param.get_type() {
                    let mut shell = lower_doc_type_node(
                        db,
                        workspace,
                        file,
                        config,
                        &local_generics,
                        &param_type,
                    );
                    // `fun(i?: integer)` params are nullable: the nullable marker is not on the type node.
                    if param.is_nullable() {
                        shell.merge(&TypeShell::from_primitive(PrimitiveType::Nil));
                    }
                    params.push(shell);
                } else {
                    params.push(TypeShell::unknown());
                }
            }
            let mut returns_multi = Vec::new();
            let mut returns = TypeShell::unknown();
            if let Some(list) = func_type.get_return_type_list() {
                for ret in list.get_return_type_list() {
                    if let (_, Some(ret_type)) = ret.get_name_and_type() {
                        let shell = lower_doc_type_node(
                            db,
                            workspace,
                            file,
                            config,
                            &local_generics,
                            &ret_type,
                        );
                        returns.merge(&shell);
                        returns_multi.push(shell);
                    }
                }
            }
            let async_state = if func_type.is_async() {
                1
            } else if func_type.is_sync() {
                2
            } else {
                0
            };
            let is_variadic = is_variadic;
            TypeShell::from_function(
                params,
                param_names,
                returns,
                returns_multi,
                fun_generics,
                async_state,
                false,
                is_variadic,
            )
        }
        LuaDocType::Binary(binary) => {
            let op = binary.get_op_token().map(|token| token.get_op());
            if op == Some(LuaTypeBinaryOperator::Union) {
                if let Some((left, right)) = binary.get_types() {
                    let mut shell =
                        lower_doc_type_node(db, workspace, file, config, generics, &left);
                    shell.merge(&lower_doc_type_node(
                        db, workspace, file, config, generics, &right,
                    ));
                    return shell;
                }
            }
            TypeShell::unknown()
        }
        LuaDocType::Nullable(nullable) => nullable
            .get_type()
            .map(|inner| {
                // `T?` = T | nil.
                let mut shell = lower_doc_type_node(db, workspace, file, config, generics, &inner);
                shell.merge(&TypeShell::from_primitive(PrimitiveType::Nil));
                shell
            })
            .unwrap_or_else(TypeShell::unknown),
        LuaDocType::MultiLineUnion(multi) => {
            let mut shell = TypeShell::unknown();
            for field in multi.get_fields() {
                if let Some(item) = field.get_type() {
                    shell.merge(&lower_doc_type_node(
                        db, workspace, file, config, generics, &item,
                    ));
                }
            }
            shell
        }
        _ => TypeShell::unknown(),
    }
}

/// Base type name -> `PrimitiveType`.
pub(crate) fn primitive_from_name(name: &str) -> Option<TypeShell> {
    let primitive = match name {
        "string" => PrimitiveType::String,
        "number" => PrimitiveType::Number,
        "integer" | "int" => PrimitiveType::Integer,
        "boolean" | "bool" => PrimitiveType::Boolean,
        "nil" | "void" => PrimitiveType::Nil,
        "table" => PrimitiveType::Table,
        "function" => PrimitiveType::Function,
        _ => return None,
    };
    Some(TypeShell::from_primitive(primitive))
}

fn type_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
) -> TypeShell {
    TypeShell::unknown()
}

fn type_cycle_recover(
    _db: &dyn SalsaDb,
    cycle: &salsa::Cycle,
    last: &TypeShell,
    value: TypeShell,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
) -> TypeShell {
    let _ = (cycle, last);
    value
}

// ──────────────────────────────────────────────
// L3 semantics: member types (with cycle convergence)
// ──────────────────────────────────────────────

/// Declared type of a member. Members can be mutually recursive (`T.foo = T.bar`), also converged by salsa's native fixed point.
#[salsa::tracked(returns(clone), lru = 4096, cycle_initial = member_cycle_initial, cycle_fn = member_cycle_recover)]
pub(crate) fn member_type(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    member: SemanticId,
) -> TypeShell {
    let facts = file_facts(db, file, config);
    let workspace = db.workspace_input();
    let Some(member) = facts.member_by_id(&member) else {
        return TypeShell::unknown();
    };

    // `---@module "name"`: project directly to a module reference.
    if let Some(module_path) = &member.module_path
        && let Some(workspace) = workspace
        && let Some(module_file) = module_file_of(db, workspace, config, module_path.clone())
    {
        return TypeShell::from_module_ref(module_file);
    }

    // Value: owner is `TypeDef` (@field) -> doc type node; otherwise -> expression.
    if let Some(value_syntax) = member.value_syntax {
        match &member.owner {
            SemanticId::TypeDef(_) => {
                let generics = facts
                    .type_def_by_id(&member.owner)
                    .map(|def| def.generic_params.as_slice())
                    .unwrap_or(&[]);
                let shell = lower_doc_type(db, workspace, file, config, value_syntax, generics);
                if !shell.is_unknown() {
                    return shell;
                }
            }
            _ => {
                let tree = parse(db, file, config);
                if let Some(expr) = find_expr_by_syntax_id(&tree, &value_syntax) {
                    let shell = expr_type(db, &facts, workspace, file, config, expr);
                    if !shell.is_unknown() {
                        return shell;
                    }
                }
            }
        }
    }

    if member.is_method {
        return TypeShell::from_primitive(PrimitiveType::Function);
    }

    TypeShell::unknown()
}

fn member_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _member: SemanticId,
) -> TypeShell {
    TypeShell::unknown()
}

fn member_cycle_recover(
    _db: &dyn SalsaDb,
    cycle: &salsa::Cycle,
    last: &TypeShell,
    value: TypeShell,
    _file: SourceFileInput,
    _config: ConfigInput,
    _member: SemanticId,
) -> TypeShell {
    let _ = (cycle, last);
    value
}

/// Direct member names of a local declaration (completion candidates).
#[salsa::tracked(returns(clone))]
pub(crate) fn member_keys_of_decl(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    decl: SemanticId,
) -> Vec<SmolStr> {
    let facts = file_facts(db, file, config);
    let mut keys = facts
        .members
        .iter()
        .filter(|member| member.owner == decl)
        .map(|member| member.key.to_path().into())
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys
}

/// Member keys of a named type (including parent types, completion candidates). `type_def` is a global type id.
#[salsa::tracked(returns(clone))]
pub(crate) fn member_keys_of_type(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    type_def: SemanticId,
) -> Vec<SmolStr> {
    let facts = file_facts(db, file, config);
    let Some(def) = facts.type_def_by_id(&type_def) else {
        return Vec::new();
    };
    let mut keys = Vec::new();
    let mut visited = Vec::new();
    collect_type_keys(&facts, def.id.clone(), &mut visited, &mut keys);
    keys.sort();
    keys.dedup();
    keys
}

/// Type of a named type's member (including parent types). Pure function; caller must be in a tracked context.
pub(crate) fn type_member(
    db: &dyn SalsaDb,
    facts: &FileFacts,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    type_def: SemanticId,
    name: &str,
    visited: &mut Vec<SemanticId>,
) -> Option<TypeShell> {
    if visited.contains(&type_def) {
        return None;
    }
    visited.push(type_def.clone());

    if let Some(member) = facts.field_members_of_type(&type_def, name) {
        if let Some(value_syntax) = member.value_syntax {
            let generics = facts
                .type_def_by_id(&type_def)
                .map(|def| def.generic_params.as_slice())
                .unwrap_or(&[]);
            let shell = lower_doc_type(db, workspace, file, config, value_syntax, generics);
            if !shell.is_unknown() {
                return Some(shell);
            }
        }
    }

    let def = facts.type_def_by_id(&type_def)?;
    for super_name in &def.super_names {
        if let Some(super_def) = facts.type_def_by_full_name(super_name) {
            if let Some(shell) = type_member(
                db,
                facts,
                workspace,
                file,
                config,
                super_def.id.clone(),
                name,
                visited,
            ) {
                return Some(shell);
            }
        }
    }
    None
}

fn collect_type_keys(
    facts: &FileFacts,
    type_def: SemanticId,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<SmolStr>,
) {
    if visited.contains(&type_def) {
        return;
    }
    visited.push(type_def.clone());
    for member in facts
        .members
        .iter()
        .filter(|member| member.owner == type_def)
    {
        out.push(member.key.to_path().into());
    }
    if let Some(def) = facts.type_def_by_id(&type_def) {
        for super_name in &def.super_names {
            if let Some(super_def) = facts.type_def_by_full_name(super_name) {
                collect_type_keys(facts, super_def.id.clone(), visited, out);
            }
        }
    }
}

// ──────────────────────────────────────────────
// L3 semantics: name resolution and references
// ──────────────────────────────────────────────

/// Name use site -> global id of declaration (scope-aware; falls back to a workspace global declaration when local lookup misses).
#[salsa::tracked(returns(clone), lru = 4096)]
pub(crate) fn resolve_name(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    offset: TextSize,
) -> Option<SemanticId> {
    let facts = file_facts(db, file, config);
    let name_use = facts.name_use_at_offset(offset)?;
    facts
        .find_visible_decl_before_offset(&name_use.name, offset)
        .map(|decl| decl.id.clone())
        .or_else(|| {
            let workspace = db.workspace_input()?;
            global_decl_by_name(db, workspace, config, name_use.name.clone())
        })
}

/// All references to a declaration (name use sites).
#[salsa::tracked(returns(clone))]
pub(crate) fn decl_references(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    decl: SemanticId,
) -> Vec<LuaSyntaxId> {
    let facts = file_facts(db, file, config);
    let Some(decl) = facts.decl_by_id(&decl) else {
        return Vec::new();
    };
    facts
        .name_uses
        .iter()
        .filter(|use_| use_.name == decl.name)
        .filter(|use_| {
            let offset = use_.syntax.get_range().start();
            facts
                .find_visible_decl_before_offset(&use_.name, offset)
                .is_some_and(|candidate| candidate.id == decl.id)
        })
        .map(|use_| use_.syntax)
        .collect()
}

// ──────────────────────────────────────────────
// L3 semantics: signature returns (with cycle convergence)
// ──────────────────────────────────────────────

/// Per-slot function return types. Doc annotations take priority (one slot per `---@return`);
/// otherwise scan the function body's `return` statements and merge by slot. Mutual recursion converges via salsa fixed point.
#[salsa::tracked(
    returns(clone), lru = 4096,
    cycle_initial = sig_returns_cycle_initial,
    cycle_fn = sig_returns_cycle_recover
)]
pub(crate) fn signature_returns(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    closure_syntax: LuaSyntaxId,
) -> Vec<TypeShell> {
    let facts = file_facts(db, file, config);
    let workspace = db.workspace_input();
    let Some(sig) = facts.signature_by_closure(closure_syntax) else {
        return Vec::new();
    };

    if let Some(docs) = &sig.docs
        && !docs.returns.is_empty()
    {
        let generics = docs.generic_params.as_slice();
        let mut slots = Vec::with_capacity(docs.returns.len());
        for &type_syntax in &docs.returns {
            slots.push(lower_doc_type(
                db,
                workspace,
                file,
                config,
                type_syntax,
                generics,
            ));
        }
        if slots.iter().any(|slot| !slot.is_unknown()) {
            return slots;
        }
    }

    // `---@return_overload`: expand rows into multi-return slots, union across rows, fill missing slots with nil.
    if let Some(docs) = &sig.docs
        && !docs.return_overload_rows.is_empty()
    {
        let generics = docs.generic_params.as_slice();
        let mut rows: Vec<Vec<LuaSyntaxId>> = Vec::new();
        let mut index = 0;
        for &len in &docs.return_overload_rows {
            let end = (index + len).min(docs.return_overloads.len());
            rows.push(
                docs.return_overloads[index..end]
                    .iter()
                    .map(|(_, syntax)| *syntax)
                    .collect(),
            );
            index = end;
        }
        let max_len = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        let mut slots = Vec::with_capacity(max_len);
        for slot in 0..max_len {
            let mut shell = TypeShell::unknown();
            for row in &rows {
                if let Some(&type_syntax) = row.get(slot) {
                    shell.merge(&lower_doc_type(
                        db,
                        workspace,
                        file,
                        config,
                        type_syntax,
                        generics,
                    ));
                } else {
                    shell.merge(&TypeShell::from_primitive(PrimitiveType::Nil));
                }
            }
            slots.push(shell);
        }
        if slots.iter().any(|slot| !slot.is_unknown()) {
            return slots;
        }
    }

    let tree = parse(db, file, config);
    let root = tree.get_red_root();
    let Some(node) = closure_syntax.to_node_from_root(&root) else {
        return Vec::new();
    };
    let Some(closure) = LuaClosureExpr::cast(node) else {
        return Vec::new();
    };
    let multi_path_returns = closure.descendants::<LuaReturnStat>().count() > 1;
    let mut slots: Vec<TypeShell> = Vec::new();
    for ret in closure.descendants::<LuaReturnStat>() {
        for (index, expr) in ret.get_expr_list().enumerate() {
            let shell = match &expr {
                LuaExpr::NameExpr(name) if name.get_name_text().as_deref() == Some("self") => {
                    method_self_return_shell(&facts, closure_syntax)
                }
                // Preserve integer literal precision when merging multi-path returns (a `return 1` branch should not be widened to number).
                LuaExpr::LiteralExpr(literal)
                    if multi_path_returns
                        && let Some(LuaLiteralToken::Number(number)) = literal.get_literal() =>
                {
                    match number.get_number_value() {
                        emmylua_parser::NumberResult::Int(_)
                        | emmylua_parser::NumberResult::Uint(_) => {
                            Some(TypeShell::from_primitive(PrimitiveType::Integer))
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
            .unwrap_or_else(|| expr_type(db, &facts, workspace, file, config, expr));
            if let Some(slot) = slots.get_mut(index) {
                slot.merge(&shell);
            } else {
                slots.push(shell);
            }
        }
    }
    // When there is no explicit return annotation and the body cannot infer a type, keep member returns from class-field declarations.
    if slots.is_empty() || slots.iter().all(TypeShell::is_unknown) {
        if let Some(expected) = member_expected_returns(db, &facts, file, config, closure_syntax) {
            return expected;
        }
    }
    slots
}

/// When a member implementation function (`function Test.e()`) has no `---@return`, use the return type
/// of the same-named `---@field e fun(): ...` as this implementation's signature return.
fn member_expected_returns(
    db: &dyn SalsaDb,
    facts: &FileFacts,
    file: SourceFileInput,
    config: ConfigInput,
    closure_syntax: LuaSyntaxId,
) -> Option<Vec<TypeShell>> {
    let member = facts
        .members
        .iter()
        .find(|member| member.value_syntax == Some(closure_syntax))?;
    for field in facts
        .members
        .iter()
        .filter(|field| field.key == member.key && field.owner != member.owner)
        .filter(|field| matches!(&field.owner, SemanticId::TypeDef(_)))
    {
        let shell = member_type(db, file, config, field.id.clone());
        for candidate in shell.candidates {
            if let TypeCandidate::Function(fun) = candidate {
                if fun.returns_multi.len() > 1 {
                    return Some(fun.returns_multi.clone());
                } else {
                    return Some(vec![fun.returns.clone()]);
                }
            }
        }
    }
    None
}

/// Self return type for `function T:method() return self end`:
/// first find the type definition associated with the method (`---@class T` comment's owner statement); fall back to the owner table identity if no type is found.
fn method_self_return_shell(facts: &FileFacts, closure_syntax: LuaSyntaxId) -> Option<TypeShell> {
    let member = facts
        .members
        .iter()
        .find(|member| member.is_method && member.value_syntax == Some(closure_syntax))?;
    let type_def = match &member.owner {
        SemanticId::TypeDef(type_def) => {
            facts.type_def_by_id(&SemanticId::TypeDef(type_def.clone()))
        }
        SemanticId::Decl(owner_decl) => {
            let owner_decl = facts.decl_by_id(&SemanticId::Decl(owner_decl.clone()))?;
            facts.type_defs.iter().find(|def| {
                def.owner_syntax.is_some() && def.owner_syntax == owner_decl.owner_syntax
            })
        }
        _ => None,
    };
    if let Some(def) = type_def {
        return Some(TypeShell::from_name(def.full_name.as_str()));
    }

    // No type definition: fall back to the owner declaration's initializer identity (usually a table literal).
    let owner_decl = match &member.owner {
        SemanticId::Decl(owner_decl) => facts.decl_by_id(&SemanticId::Decl(owner_decl.clone()))?,
        _ => return None,
    };
    let value_syntax = owner_decl.value_expr_syntax?;
    Some(TypeShell::from_table(TableId::from_range(
        facts.file_id,
        value_syntax.get_range(),
    )))
}

/// Function return type (merged view, compatible with old consumers). Doc annotations take priority; otherwise scan the function body's `return` statements.
/// Mutual recursion (`foo`->`bar`->`foo`) converges via salsa's native fixed point.
#[salsa::tracked(returns(clone), lru = 4096, cycle_initial = sig_cycle_initial, cycle_fn = sig_cycle_recover)]
pub(crate) fn signature_return(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    closure_syntax: LuaSyntaxId,
) -> TypeShell {
    let mut shell = TypeShell::unknown();
    for slot in signature_returns(db, file, config, closure_syntax) {
        shell.merge(&slot);
    }
    shell
}

/// Type of the function's `param_index`-th parameter (`---@param` annotation + generic binding).
#[salsa::tracked(returns(clone), lru = 4096)]
pub(crate) fn param_type(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    closure_syntax: LuaSyntaxId,
    param_index: usize,
) -> TypeShell {
    let facts = file_facts(db, file, config);
    let workspace = db.workspace_input();
    let Some(sig) = facts.signature_by_closure(closure_syntax) else {
        return TypeShell::unknown();
    };
    let Some(docs) = &sig.docs else {
        return TypeShell::unknown();
    };
    let Some(param_name) = sig.param_names.get(param_index) else {
        return TypeShell::unknown();
    };
    let Some((_, type_syntax)) = docs.param_types.iter().find(|(name, _)| name == param_name)
    else {
        return TypeShell::unknown();
    };
    lower_doc_type(
        db,
        workspace,
        file,
        config,
        *type_syntax,
        &docs.generic_params,
    )
}

fn sig_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _closure_syntax: LuaSyntaxId,
) -> TypeShell {
    TypeShell::unknown()
}

fn sig_cycle_recover(
    _db: &dyn SalsaDb,
    cycle: &salsa::Cycle,
    last: &TypeShell,
    value: TypeShell,
    _file: SourceFileInput,
    _config: ConfigInput,
    _closure_syntax: LuaSyntaxId,
) -> TypeShell {
    let _ = (cycle, last);
    value
}

fn sig_returns_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _closure_syntax: LuaSyntaxId,
) -> Vec<TypeShell> {
    Vec::new()
}

fn sig_returns_cycle_recover(
    _db: &dyn SalsaDb,
    cycle: &salsa::Cycle,
    last: &Vec<TypeShell>,
    value: Vec<TypeShell>,
    _file: SourceFileInput,
    _config: ConfigInput,
    _closure_syntax: LuaSyntaxId,
) -> Vec<TypeShell> {
    let _ = (cycle, last);
    value
}

/// Call target -> its function body (closure) `LuaSyntaxId`.
fn callee_closure_syntax(facts: &FileFacts, callee: LuaExpr) -> Option<LuaSyntaxId> {
    match callee {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            let offset = name_expr.get_position();
            let decl = facts.find_visible_decl_before_offset(&name, offset)?;
            decl.value_expr_syntax
        }
        LuaExpr::IndexExpr(index_expr) => {
            let (owner, name) = member_ref_from_index_expr(facts, &index_expr)?;
            let member = facts
                .members_of_owner(&owner)
                .find(|m| m.key.name() == Some(name.as_str()))?;
            member.value_syntax
        }
        LuaExpr::ClosureExpr(closure) => Some(closure.get_syntax_id()),
        _ => None,
    }
}

// ──────────────────────────────────────────────
// L3 semantics: module exports
// ──────────────────────────────────────────────

/// Value type exported by a module (type of `return M` / table literal, etc.).
#[salsa::tracked(returns(clone))]
pub(crate) fn module_export_type(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
) -> TypeShell {
    let facts = file_facts(db, file, config);
    let workspace = db.workspace_input();
    match &facts.module_export {
        // `return M`: declaration identity table (TableConst members reachable) + name identity (Named -> decl owner reachable).
        ModuleExport::Decl { decl, name } => {
            let mut shell = decl_type(db, file, config, decl.clone());
            shell.merge(&TypeShell::from_name(name.as_str()));
            shell
        }
        ModuleExport::Global { name } => {
            // For global exports, resolve by workspace declaration identity first, then fall back to the name.
            if let Some(workspace) = workspace {
                let decl = global_decl_by_name(db, workspace, config, SmolStr::new(name.as_str()));
                if let Some(decl) = decl {
                    let SemanticId::Decl(decl_key) = &decl else {
                        return TypeShell::from_name(name.as_str());
                    };
                    if let Some(decl_file) = db.file_input(decl_key.file_id) {
                        let shell = decl_type(db, decl_file, config, decl);
                        if !shell.is_unknown() {
                            return shell;
                        }
                    }
                }
            }
            TypeShell::from_name(name.as_str())
        }
        ModuleExport::Expr { value_syntax } => {
            let tree = parse(db, file, config);
            let Some(expr) = find_expr_by_syntax_id(&tree, value_syntax) else {
                return TypeShell::unknown();
            };
            expr_type(db, &facts, workspace, file, config, expr)
        }
        ModuleExport::None => TypeShell::unknown(),
    }
}

// ──────────────────────────────────────────────
// Expression types
// ──────────────────────────────────────────────

/// Locate an expression in the syntax tree by `LuaSyntaxId` (kind+range, unique).
pub(crate) fn find_expr_by_syntax_id(
    tree: &LuaSyntaxTree,
    syntax_id: &LuaSyntaxId,
) -> Option<LuaExpr> {
    let root = tree.get_red_root();
    let node = syntax_id.to_node_from_root(&root)?;
    LuaExpr::cast(node)
}

/// Type of an expression (by syntax position, node-keyed). Entry point for the semantic/infer layer.
#[salsa::tracked(returns(clone), lru = 4096, cycle_initial = expr_cycle_initial, cycle_fn = expr_cycle_recover)]
pub(crate) fn expr_type_of(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    config: ConfigInput,
    expr_syntax: LuaSyntaxId,
) -> TypeShell {
    let facts = file_facts(db, file, config);
    let workspace = db.workspace_input();
    let tree = parse(db, file, config);
    let Some(expr) = find_expr_by_syntax_id(&tree, &expr_syntax) else {
        return TypeShell::unknown();
    };
    expr_type(db, &facts, workspace, file, config, expr)
}

fn expr_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _expr_syntax: LuaSyntaxId,
) -> TypeShell {
    TypeShell::unknown()
}

fn expr_cycle_recover(
    _db: &dyn SalsaDb,
    cycle: &salsa::Cycle,
    last: &TypeShell,
    value: TypeShell,
    _file: SourceFileInput,
    _config: ConfigInput,
    _expr_syntax: LuaSyntaxId,
) -> TypeShell {
    let _ = (cycle, last);
    value
}

fn expr_type(
    db: &dyn SalsaDb,
    facts: &FileFacts,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    expr: LuaExpr,
) -> TypeShell {
    // Deep member/call chains (1500+ levels) use an explicit task stack: avoids exhausting the native stack by recursive prefix evaluation.
    if let Some(shell) = expr_type_chain(db, facts, workspace, file, config, expr.clone()) {
        return shell;
    }
    expr_type_node(db, facts, workspace, file, config, expr)
}

#[derive(Clone)]
enum ChainFrame {
    Call(LuaCallExpr, LuaExpr),
    Index(LuaIndexExpr),
    Paren,
}

fn expr_type_chain(
    db: &dyn SalsaDb,
    facts: &FileFacts,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    expr: LuaExpr,
) -> Option<TypeShell> {
    let mut current = expr;
    let mut frames: Vec<ChainFrame> = Vec::new();
    loop {
        match current {
            LuaExpr::CallExpr(call) => {
                let prefix = call.get_prefix_expr()?;
                frames.push(ChainFrame::Call(call, prefix.clone()));
                current = prefix;
            }
            LuaExpr::IndexExpr(index) => {
                let prefix = index.get_prefix_expr()?;
                frames.push(ChainFrame::Index(index));
                current = prefix;
            }
            LuaExpr::ParenExpr(paren) => {
                let inner = paren.get_expr()?;
                frames.push(ChainFrame::Paren);
                current = inner;
            }
            _ => break,
        }
    }
    let mut shell = expr_type_node(db, facts, workspace, file, config, current);
    for frame in frames.iter().rev() {
        match frame {
            ChainFrame::Call(call, prefix) => {
                shell = expr_type_call(
                    db,
                    facts,
                    workspace,
                    file,
                    config,
                    call.clone(),
                    prefix.clone(),
                    shell,
                );
            }
            ChainFrame::Index(index) => {
                shell = expr_type_index(db, facts, workspace, file, config, index.clone(), shell);
            }
            ChainFrame::Paren => {}
        }
    }
    Some(shell)
}

fn expr_type_node(
    db: &dyn SalsaDb,
    facts: &FileFacts,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    expr: LuaExpr,
) -> TypeShell {
    match expr {
        LuaExpr::LiteralExpr(literal) => literal_type(&literal),
        LuaExpr::TableExpr(table) => {
            TypeShell::from_table(TableId::from_range(file.file_id(db), table.get_range()))
        }
        LuaExpr::ClosureExpr(_) => TypeShell::from_primitive(PrimitiveType::Function),
        LuaExpr::NameExpr(name_expr) => {
            let Some(name) = name_expr.get_name_text() else {
                return TypeShell::unknown();
            };
            if name == "nil" {
                return TypeShell::from_primitive(PrimitiveType::Nil);
            }
            let offset = name_expr.get_position();
            if let Some(decl) = facts.find_visible_decl_before_offset(&name, offset) {
                return decl_type(db, file, config, decl.id.clone());
            }
            // Cross-file global fallback: global variable -> global type name.
            if let Some(workspace) = workspace {
                let global_name = SmolStr::new(name.as_str());
                if let Some(decl) = global_decl_by_name(db, workspace, config, global_name.clone())
                {
                    if let SemanticId::Decl(key) = &decl
                        && let Some(decl_file) = db.file_input(key.file_id)
                    {
                        let shell = decl_type(db, decl_file, config, decl);
                        if !shell.is_unknown() {
                            return shell;
                        }
                    }
                }
                if let Some(type_def) = global_type_by_name(db, workspace, config, global_name) {
                    if let SemanticId::TypeDef(key) = &type_def {
                        return TypeShell::from_name(key.full_name.as_str());
                    }
                }
            }
            TypeShell::unknown()
        }
        LuaExpr::IndexExpr(index_expr) => {
            let Some(prefix) = index_expr.get_prefix_expr() else {
                return TypeShell::unknown();
            };
            let prefix_shell = expr_type(db, facts, workspace, file, config, prefix);
            expr_type_index(db, facts, workspace, file, config, index_expr, prefix_shell)
        }
        LuaExpr::CallExpr(call_expr) => {
            let Some(prefix) = call_expr.get_prefix_expr() else {
                return TypeShell::unknown();
            };
            let prefix_shell = expr_type(db, facts, workspace, file, config, prefix.clone());
            expr_type_call(
                db,
                facts,
                workspace,
                file,
                config,
                call_expr,
                prefix,
                prefix_shell,
            )
        }
        LuaExpr::BinaryExpr(binary) => {
            let op = binary.get_op_token().map(|token| token.get_op());
            match op {
                Some(BinaryOperator::OpOr) | Some(BinaryOperator::OpAnd) => {
                    if let Some((left, right)) = binary.get_exprs() {
                        let mut shell = expr_type(db, facts, workspace, file, config, left);
                        shell.merge(&expr_type(db, facts, workspace, file, config, right));
                        return shell;
                    }
                    TypeShell::unknown()
                }
                Some(BinaryOperator::OpConcat) => TypeShell::from_primitive(PrimitiveType::String),
                Some(
                    BinaryOperator::OpLt
                    | BinaryOperator::OpLe
                    | BinaryOperator::OpGt
                    | BinaryOperator::OpGe
                    | BinaryOperator::OpEq
                    | BinaryOperator::OpNe,
                ) => TypeShell::from_primitive(PrimitiveType::Boolean),
                Some(
                    BinaryOperator::OpAdd
                    | BinaryOperator::OpSub
                    | BinaryOperator::OpMul
                    | BinaryOperator::OpDiv
                    | BinaryOperator::OpIDiv
                    | BinaryOperator::OpMod
                    | BinaryOperator::OpPow,
                ) => TypeShell::from_primitive(PrimitiveType::Number),
                _ => TypeShell::unknown(),
            }
        }
        LuaExpr::UnaryExpr(unary) => {
            let op = unary.get_op_token().map(|token| token.get_op());
            if op == Some(UnaryOperator::OpNot) {
                TypeShell::from_primitive(PrimitiveType::Boolean)
            } else {
                unary
                    .get_expr()
                    .map(|expr| expr_type(db, facts, workspace, file, config, expr))
                    .unwrap_or_else(TypeShell::unknown)
            }
        }
        LuaExpr::ParenExpr(paren) => paren
            .get_expr()
            .map(|expr| expr_type(db, facts, workspace, file, config, expr))
            .unwrap_or_else(TypeShell::unknown),
        _ => TypeShell::unknown(),
    }
}

fn expr_type_index(
    db: &dyn SalsaDb,
    facts: &FileFacts,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    index_expr: LuaIndexExpr,
    prefix_shell: TypeShell,
) -> TypeShell {
    let Some((owner, name)) = member_ref_from_index_expr(facts, &index_expr) else {
        return TypeShell::unknown();
    };
    // 1. In-file members (owner key).
    if let Some(member) = facts
        .members_of_owner(&owner)
        .find(|m| m.key.name() == Some(name.as_str()))
    {
        return member_type(db, file, config, member.id.clone());
    }
    // 2/3. Phase 2: cross-file merged lookup + type-member fallback (requires workspace).
    if let Some(workspace) = workspace {
        // 2. Cross-file members by owner key + resolved concrete id key.
        if let Some(shell) = member_type_via_owner(db, workspace, config, &owner, &name) {
            return shell;
        }
        // 3. If the prefix type is a named class/export name -> cross-file owner members (@field + runtime), then inherited @fields.
        for candidate in &prefix_shell.candidates {
            // Generic instantiation `Box<number>`: member types contain `Generic(T)`, substitute with actual args.
            if let TypeCandidate::GenericInstance(ins) = candidate {
                if let Some(def) =
                    resolve_type_def(db, workspace, config, file, SmolStr::new(ins.name.as_str()))
                {
                    if let Some(shell) =
                        member_type_via_owner(db, workspace, config, &def.id, &name)
                    {
                        let substituted =
                            substitute_generics(&shell, &def.generic_params, &ins.args);
                        if !substituted.is_unknown() {
                            return substituted;
                        }
                    }
                }
            }
            // Anonymous table literal: members are collected under a synthetic owner; look them up by that owner.
            if let TypeCandidate::Table(table_id) = candidate {
                let owner = SemanticId::member(
                    FileId::new(table_id.file_id),
                    rowan::TextRange::new(
                        TextSize::from(table_id.start),
                        TextSize::from(table_id.end),
                    ),
                );
                if let Some(shell) = member_type_via_owner(db, workspace, config, &owner, &name) {
                    return shell;
                }
            }
            if let TypeCandidate::Named(type_name) = candidate {
                // Cross-file: runtime members under Name(type_name) key + resolved @fields.
                // Direct class-table writes (`Foo.extra`) can see dot assignments;
                // instance access through a named type (`other.extra`) only inherits `:` methods,
                // not arbitrary members from the global class table as instance fields.
                let prefix_is_class_table = index_expr.get_prefix_expr().is_some_and(|prefix| {
                    matches!(
                        &prefix,
                        LuaExpr::NameExpr(name)
                            if name.get_name_text().as_deref() == Some(type_name.as_str())
                    )
                });
                let named_owner = SemanticId::name(type_name.clone());
                let is_class_instance =
                    resolve_type_def(db, workspace, config, file, type_name.clone())
                        .is_some_and(|def| def.kind == TypeDefKind::Class);
                if prefix_is_class_table || !is_class_instance {
                    if let Some(shell) =
                        member_type_via_owner(db, workspace, config, &named_owner, &name)
                    {
                        return shell;
                    }
                } else if let Some(shell) =
                    member_type_via_owner_method(db, workspace, config, &named_owner, &name)
                {
                    return shell;
                }
                // Inherited @fields (in-file class definitions).
                let def = facts
                    .type_def_by_name(type_name.as_str())
                    .or_else(|| facts.type_def_by_full_name(type_name.as_str()));
                if let Some(def) = def {
                    let mut visited = Vec::new();
                    if let Some(shell) = type_member(
                        db,
                        facts,
                        Some(workspace),
                        file,
                        config,
                        def.id.clone(),
                        &name,
                        &mut visited,
                    ) {
                        return shell;
                    }
                }
            }
        }
    }
    TypeShell::unknown()
}

fn expr_type_call(
    db: &dyn SalsaDb,
    facts: &FileFacts,
    workspace: Option<WorkspaceInput>,
    file: SourceFileInput,
    config: ConfigInput,
    call_expr: LuaCallExpr,
    prefix: LuaExpr,
    prefix_shell: TypeShell,
) -> TypeShell {
    // require special case: module name -> module file -> module export type.
    if let Some(workspace) = workspace {
        if call_expr.is_require()
            && let Some(module_name) = require_module_name(&call_expr)
            && let Some(module_file) =
                module_file_of(db, workspace, config, SmolStr::new(&module_name))
            && let Some(module_input) = db.file_input(module_file)
        {
            let shell = module_export_type(db, module_input, config);
            if !shell.is_unknown() {
                return shell;
            }
        }
    }
    match callee_closure_syntax(facts, prefix) {
        Some(closure_syntax) => signature_return(db, file, config, closure_syntax),
        None => {
            // Function-value call: if callee type is fun(...), take its return type (including generic substitution).
            for candidate in &prefix_shell.candidates {
                if let TypeCandidate::Function(fun) = candidate {
                    return fun.returns.clone();
                }
            }
            TypeShell::unknown()
        }
    }
}

/// Phase 2 member type: union of members by owner key + resolved concrete id key (cross-file).
/// Each member's type is resolved in its declaring file (`member_type` keyed by file input, so invalidation is file-precise).
fn member_type_via_owner(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    owner: &SemanticId,
    name: &str,
) -> Option<TypeShell> {
    // Union of dual identities: same-name type (@field) + runtime value (member declaration).
    for owner in resolve_owner_set(db, workspace, config, owner.clone()) {
        for member in members_of_owner(db, workspace, config, owner)
            .iter()
            .cloned()
        {
            if member.name != name {
                continue;
            }
            let Some(member_file_input) = db.file_input(member.file_id) else {
                continue;
            };
            let shell = member_type(db, member_file_input, config, member.id);
            if !shell.is_unknown() {
                return Some(shell);
            }
        }
    }
    None
}

/// Same as `member_type_via_owner`, but only accepts `:` method members.
/// Instance access inherits methods from the class table, not arbitrary dot-assignments on it.
fn member_type_via_owner_method(
    db: &dyn SalsaDb,
    workspace: WorkspaceInput,
    config: ConfigInput,
    owner: &SemanticId,
    name: &str,
) -> Option<TypeShell> {
    for resolved in resolve_owner_set(db, workspace, config, owner.clone()) {
        for member in members_of_owner(db, workspace, config, resolved)
            .iter()
            .cloned()
        {
            if member.name != name {
                continue;
            }
            let Some(member_file_input) = db.file_input(member.file_id) else {
                continue;
            };
            let member_facts = file_facts(db, member_file_input, config);
            let Some(member_def) = member_facts.member_by_id(&member.id) else {
                continue;
            };
            if !member_def.is_method {
                continue;
            }
            let shell = member_type(db, member_file_input, config, member.id);
            if !shell.is_unknown() {
                return Some(shell);
            }
        }
    }
    None
}

/// Generic substitution: replace `Generic(param)` candidates in a shell with argument types (recursing into function types).
fn substitute_generics(
    shell: &TypeShell,
    params: &[SalsaGenericParam],
    args: &[TypeShell],
) -> TypeShell {
    let mut out = TypeShell::unknown();
    for candidate in &shell.candidates {
        match candidate {
            TypeCandidate::Generic(gname) => {
                let index = params.iter().position(|p| &p.name == gname);
                if let Some(index) = index
                    && let Some(arg) = args.get(index)
                {
                    out.merge(arg);
                } else {
                    out.merge(&TypeShell {
                        candidates: vec![candidate.clone()],
                    });
                }
            }
            TypeCandidate::Array(base) => {
                out.merge(&TypeShell::from_array(substitute_generics(
                    base, params, args,
                )));
            }
            TypeCandidate::Variadic(base) => {
                out.merge(&TypeShell::from_variadic(substitute_generics(
                    base, params, args,
                )));
            }
            TypeCandidate::Tuple(types) => {
                out.merge(&TypeShell::from_tuple(
                    types
                        .iter()
                        .map(|ty| substitute_generics(ty, params, args))
                        .collect(),
                ));
            }
            TypeCandidate::Function(fun) => {
                let new_params = fun
                    .params
                    .iter()
                    .map(|p| substitute_generics(p, params, args))
                    .collect();
                let new_returns = substitute_generics(&fun.returns, params, args);
                let new_returns_multi = fun
                    .returns_multi
                    .iter()
                    .map(|r| substitute_generics(r, params, args))
                    .collect();
                out.merge(&TypeShell::from_function(
                    new_params,
                    fun.param_names.clone(),
                    new_returns,
                    new_returns_multi,
                    fun.generic_params.clone(),
                    fun.async_state,
                    fun.is_colon_define,
                    fun.is_variadic,
                ));
            }
            _ => out.merge(&TypeShell {
                candidates: vec![candidate.clone()],
            }),
        }
    }
    out
}

fn literal_type(literal: &LuaLiteralExpr) -> TypeShell {
    match literal.get_literal() {
        Some(LuaLiteralToken::String(_)) => TypeShell::from_primitive(PrimitiveType::String),
        Some(LuaLiteralToken::Number(number)) => match number.get_number_value() {
            emmylua_parser::NumberResult::Int(_) | emmylua_parser::NumberResult::Uint(_) => {
                TypeShell::from_primitive(PrimitiveType::Number)
            }
            emmylua_parser::NumberResult::Float(f) => {
                TypeShell::from_literal(LiteralShell::Float(f.to_bits()))
            }
            emmylua_parser::NumberResult::Number => {
                TypeShell::from_primitive(PrimitiveType::Number)
            }
        },
        Some(LuaLiteralToken::Bool(_)) => TypeShell::from_primitive(PrimitiveType::Boolean),
        Some(LuaLiteralToken::Nil(_)) => TypeShell::from_primitive(PrimitiveType::Nil),
        Some(LuaLiteralToken::Dots(_)) => TypeShell::unknown(),
        _ => TypeShell::unknown(),
    }
}

/// Module name for a `require` call (the first argument must be a string literal; dynamic arguments are left to the infer layer).
fn require_module_name(call_expr: &LuaCallExpr) -> Option<String> {
    let arg_list = call_expr.get_args_list()?;
    let first = arg_list.get_args().next()?;
    match first {
        LuaExpr::LiteralExpr(literal) => match literal.get_literal()? {
            LuaLiteralToken::String(token) => Some(token.get_value()),
            _ => None,
        },
        _ => None,
    }
}

/// file_id → (file input, config input).
pub(crate) fn file_and_config(
    db: &SalsaDatabase,
    file_id: FileId,
) -> Option<(SourceFileInput, ConfigInput)> {
    Some((db.file_input(file_id)?, db.config_input()?))
}

/// Index expression -> `(owner, name)` (member reference resolution).
pub(crate) fn member_ref_from_index_expr(
    facts: &FileFacts,
    index_expr: &LuaIndexExpr,
) -> Option<(SemanticId, SmolStr)> {
    let name = SmolStr::new(index_expr.get_index_key()?.get_path_part());
    let prefix = index_expr.get_prefix_expr()?;
    let mut segments = Vec::new();
    let owner = resolve_expr_root(facts, prefix, &mut segments)?;
    Some((owner, name))
}

fn resolve_expr_root(
    facts: &FileFacts,
    expr: LuaExpr,
    _segments: &mut Vec<SmolStr>,
) -> Option<SemanticId> {
    // Deep member chains: expand IndexExpr with an explicit task stack to avoid native stack recursion.
    let mut current = expr;
    let mut segments = Vec::new();
    loop {
        match current {
            LuaExpr::ParenExpr(paren) => {
                current = paren.get_expr()?;
            }
            LuaExpr::IndexExpr(parent) => {
                segments.push(SmolStr::new(parent.get_index_key()?.get_path_part()));
                current = parent.get_prefix_expr()?;
            }
            _ => break,
        }
    }
    let owner = match current {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            if name == "_ENV" || name == "_G" {
                SemanticId::name(SmolStr::new(name))
            } else {
                let offset = name_expr.get_position();
                if let Some(decl) = facts.find_visible_decl_before_offset(&name, offset)
                    && !matches!(decl.kind, DeclKind::Global)
                {
                    decl.id.clone()
                } else {
                    SemanticId::name(SmolStr::new(name))
                }
            }
        }
        _ => return None,
    };
    if let SemanticId::Name(root) = &owner {
        let mut path = root.as_str().to_string();
        for s in segments.iter().rev() {
            path.push('.');
            path.push_str(s);
        }
        return Some(SemanticId::name(SmolStr::new(path)));
    }
    Some(owner)
}
