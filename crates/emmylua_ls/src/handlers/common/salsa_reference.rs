//! # Salsa reference engine (shared by references / rename / document_highlight)
//!
//! Reference ranges for declarations / members / type definitions / labels, **only through the salsa analysis layer** (cross-file, queried file by file).
//! Unlike the old `reference_searcher` (DbIndex reference index):
//! - decl references: `decl_references` (name use sites, scope-aware) + declaration name range;
//! - member references: `resolve_member` per index expression + `facts.members` definition sites;
//! - type references: `resolve_type_def` per doc name type + `type_defs_in_scope` definition sites;
//! - label references: same-file pure syntax (same-named goto/label in the same closure).

use emmylua_code_analysis::{FileId, SalsaDatabase, SalsaSemanticModel, SemanticId, TypeDef};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaDocNameType, LuaExpr, LuaGotoStat, LuaIndexExpr,
    LuaLabelStat, LuaLiteralToken, LuaSyntaxKind, LuaSyntaxToken,
};
use rowan::TextRange;

/// All reference positions for a declaration (Decl), cross-file, plus the declaration name.
pub fn decl_reference_ranges(
    salsa: &SalsaDatabase,
    decl: &SemanticId,
    include_declaration: bool,
) -> Vec<(FileId, TextRange)> {
    let mut out = Vec::new();
    for range in salsa.decl_reference_ranges(decl) {
        push_unique(&mut out, range);
    }
    if include_declaration && let SemanticId::Decl(key) = decl {
        push_unique(&mut out, (key.file_id, key.name_range));
    }
    out
}

/// All reference positions for a member (Member), cross-file: definition sites (sharded reference index) + index use sites.
pub fn member_reference_ranges(
    salsa: &SalsaDatabase,
    member: &SemanticId,
    include_declaration: bool,
) -> Vec<(FileId, TextRange)> {
    let mut out = Vec::new();
    // Use sites: sharded reference index.
    for range in salsa.member_reference_ranges(member) {
        push_unique(&mut out, range);
    }
    // Definition sites: the sharded reference index already contains all member key declaration sites.
    for range in salsa.member_definition_ranges(member) {
        push_unique(&mut out, range);
    }
    // `---@[constructor("init")]` meta-class functions: after `local A = meta("Name")` creates a class,
    // `A()` is equivalent to `A:init()`, so the call prefix is also counted as a reference to the `init` member.
    for range in constructor_call_ranges(salsa, member) {
        push_unique(&mut out, range);
    }
    // Declaration site (the target member own key; already covered by sharded definition sites, kept defensively).
    if include_declaration
        && let SemanticId::Member(key) = member
        && let Some(key_range) = member.member_key_range()
    {
        push_unique(&mut out, (key.file_id, key_range));
    }
    out
}

/// `---@[constructor("init")]` class call sites: when a member is its type constructor,
/// call prefixes on the type runtime value (`A()`) are counted as references to that constructor member.
fn constructor_call_ranges(salsa: &SalsaDatabase, member: &SemanticId) -> Vec<(FileId, TextRange)> {
    let Some((def, runtime_owner)) = member_constructor(salsa, member) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for file_id in salsa.file_ids() {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        let Some(chunk) = model.chunk() else {
            continue;
        };
        for call in chunk.descendants::<LuaCallExpr>() {
            let Some(LuaExpr::NameExpr(name_expr)) = call.get_prefix_expr() else {
                continue;
            };
            let resolved = model.resolve_name(name_expr.get_position());
            if resolved.as_ref() == Some(&def.id)
                || runtime_owner.is_some() && resolved == runtime_owner
                || call_prefix_type_is_def(&model, name_expr.get_syntax_id(), &def)
            {
                push_unique(&mut out, (file_id, name_expr.get_range()));
            }
        }
    }
    out
}

/// Member → its owning type + runtime value identity (only when the member is a constructor).
fn member_constructor(
    salsa: &SalsaDatabase,
    member: &SemanticId,
) -> Option<(TypeDef, Option<SemanticId>)> {
    let SemanticId::Member(key) = member else {
        return None;
    };
    let model = SalsaSemanticModel::new(salsa, key.file_id)?;
    let members = model.members()?;
    let member_def = members.iter().find(|m| &m.id == member)?;
    let member_name = member_def.key.to_path();
    let facts = model.file_facts()?;

    let (def, runtime_owner) = match &member_def.owner {
        SemanticId::Decl(_) => {
            let owner_decl = facts.decl_by_id(&member_def.owner)?;
            let def = facts
                .type_defs
                .iter()
                .find(|def| {
                    def.owner_syntax.is_some() && def.owner_syntax == owner_decl.owner_syntax
                })?
                .clone();
            (def, Some(member_def.owner.clone()))
        }
        SemanticId::TypeDef(type_key) => {
            let def = model
                .type_defs_in_scope(type_key.scope, &type_key.full_name)
                .into_iter()
                .find(|def| def.id == member_def.owner)?;
            let runtime = runtime_decl_of_type_def(&model, &def);
            (def, runtime)
        }
        SemanticId::Name(name) => {
            let def = model.resolve_type_def(name)?;
            let runtime = runtime_decl_of_type_def(&model, &def);
            (def, runtime)
        }
        _ => return None,
    };

    if constructor_name_for_type_def(salsa, &def, runtime_owner.as_ref())?.as_str() != member_name {
        return None;
    }
    Some((def, runtime_owner))
}

/// Runtime value decl for a type definition (`---@class A` followed by `local A = ...`).
fn runtime_decl_of_type_def(model: &SalsaSemanticModel<'_>, def: &TypeDef) -> Option<SemanticId> {
    let facts = model.file_facts_of(def.file_id)?;
    let owner_syntax = def.owner_syntax?;
    facts
        .decls
        .iter()
        .find(|decl| decl.owner_syntax == Some(owner_syntax))
        .map(|decl| decl.id.clone())
}

/// Get the constructor name from the type runtime initialization call:
/// `---@[constructor("init")]` is attached to a parameter doc of the `meta` signature,
/// and `local A = meta("A")` binds the string argument to the type definition.
fn constructor_name_for_type_def(
    salsa: &SalsaDatabase,
    def: &TypeDef,
    runtime_owner: Option<&SemanticId>,
) -> Option<String> {
    let model = SalsaSemanticModel::new(salsa, def.file_id)?;
    let facts = model.file_facts()?;
    let decl_id = runtime_owner
        .cloned()
        .or_else(|| runtime_decl_of_type_def(&model, def))?;
    let decl = facts.decl_by_id(&decl_id)?;
    let call_syntax = decl.value_expr_syntax?;
    let chunk = model.chunk()?;
    let call = chunk
        .descendants::<LuaCallExpr>()
        .find(|call| call.get_syntax_id() == call_syntax)?;
    let args: Vec<LuaExpr> = call.get_args_list()?.get_args().collect();
    let LuaExpr::NameExpr(name_expr) = &call.get_prefix_expr()? else {
        return None;
    };
    let callee = model.resolve_name(name_expr.get_position())?;
    let SemanticId::Decl(callee_key) = &callee else {
        return None;
    };
    let callee_facts = model.file_facts_of(callee_key.file_id)?;
    let callee_decl = callee_facts.decl_by_id(&callee)?;
    let signature = callee_facts.signature_by_closure(callee_decl.value_expr_syntax?)?;
    let docs = signature.docs.as_ref()?;

    // Constructor attributes are stored by parameter name; arguments align in parameter order, and the string value must be this type name.
    for (param_idx, param_name) in signature.param_names.iter().enumerate() {
        let Some((_, attribute)) = docs
            .constructor_params
            .iter()
            .find(|(attr_param, _)| attr_param == param_name)
        else {
            continue;
        };
        let Some(arg) = args.get(param_idx) else {
            continue;
        };
        let Some(literal) = string_literal_of_expr(arg) else {
            continue;
        };
        if literal == def.name.as_str() || literal == def.full_name.as_str() {
            return Some(attribute.name.to_string());
        }
    }
    None
}

fn string_literal_of_expr(expr: &LuaExpr) -> Option<String> {
    let token = expr
        .syntax()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .find_map(LuaLiteralToken::cast)?;
    match token {
        LuaLiteralToken::String(string) => Some(string.get_value()),
        _ => None,
    }
}

fn call_prefix_type_is_def(
    model: &SalsaSemanticModel<'_>,
    prefix_syntax: emmylua_parser::LuaSyntaxId,
    def: &TypeDef,
) -> bool {
    model.type_of_expr(prefix_syntax) == model.type_def_ref(def)
}

/// Member key text rename ranges (for rename): **all** member definition sites with the same key text + index key sites (cross-file).
/// Key matching = `LuaMemberKey::to_path()` text equality (`Name("x")` ↔ `T.x`; `Integer(1)` ↔ `t[1]`).
pub fn member_key_rename_ranges(
    salsa: &SalsaDatabase,
    member: &SemanticId,
    new_name: &str,
) -> Vec<(FileId, TextRange, String)> {
    let mut out = Vec::new();
    let SemanticId::Member(key) = member else {
        return out;
    };
    let Some(key_text) = member_key_text_of(salsa, member) else {
        return out;
    };
    // Member rename is currently restricted to the declaring file (mirroring old origin-owner semantics), so only that file is scanned.
    let file_id = key.file_id;
    let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
        return out;
    };
    // Member definition sites (including `@field`, table fields, assignments, method names).
    if let Some(members) = model.members() {
        for m in members.iter() {
            if m.key.to_path() == key_text
                && let Some(key_range) = m.id.member_key_range()
            {
                push_unique_text(&mut out, (file_id, key_range, new_name.to_string()));
            }
        }
    }
    // Index key sites (`T.x` / `t[1]` / `obj:m()`).
    let Some(chunk) = model.chunk() else {
        return out;
    };
    for index_expr in chunk.descendants::<LuaIndexExpr>() {
        let Some(key) = index_expr.get_index_key() else {
            continue;
        };
        if key.get_path_part() == key_text
            && let Some(range) = key.get_range()
        {
            push_unique_text(&mut out, (file_id, range, new_name.to_string()));
        }
    }
    out
}

/// All reference positions for a type definition (TypeDef), cross-file, plus definition sites.
pub fn type_def_reference_ranges(
    salsa: &SalsaDatabase,
    def: &TypeDef,
    include_declaration: bool,
) -> Vec<(FileId, TextRange)> {
    let mut out = Vec::new();
    let scope = def_scope(def);
    // Definition sites: all definitions with the same scope and full name (including `@class` names).
    if include_declaration && let Some(model) = SalsaSemanticModel::new(salsa, def.file_id) {
        for d in model.type_defs_in_scope(scope, &def.full_name) {
            push_unique(&mut out, (d.file_id, d.name_range));
        }
    }
    // Use sites: doc name types in each file that resolve to this same type definition.
    for file_id in salsa.file_ids() {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        let Some(chunk) = model.chunk() else {
            continue;
        };
        for name_type in chunk.descendants::<LuaDocNameType>() {
            let Some(name) = name_type.get_name_text() else {
                continue;
            };
            if matches_type_def(&model, def, &name) {
                push_unique(&mut out, (file_id, name_type.get_range()));
            }
        }
    }
    out
}

/// Type rename ranges: definition sites + use sites; use sites replace the old name segment with the new name.
pub fn type_def_rename_ranges(
    salsa: &SalsaDatabase,
    def: &TypeDef,
    new_name: &str,
) -> Vec<(FileId, TextRange, String)> {
    let mut out = Vec::new();
    let scope = def_scope(def);
    // Definition sites: name token of `@class Foo` → new name.
    if let Some(model) = SalsaSemanticModel::new(salsa, def.file_id) {
        for d in model.type_defs_in_scope(scope, &def.full_name) {
            push_unique_text(&mut out, (d.file_id, d.name_range, new_name.to_string()));
        }
    }
    // Use sites: replace the display name (`Test.Abc` → `Abc`; full-name tail `Luakit.Test.Abc` → `Luakit.Abc`).
    for file_id in salsa.file_ids() {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        let Some(chunk) = model.chunk() else {
            continue;
        };
        for name_type in chunk.descendants::<LuaDocNameType>() {
            let Some(name) = name_type.get_name_text() else {
                continue;
            };
            if matches_type_def(&model, def, &name) {
                let new_text = if name.ends_with(def.name.as_str()) {
                    format!("{}{}", &name[..name.len() - def.name.len()], new_name)
                } else {
                    replace_last_segment(&name, new_name)
                };
                push_unique_text(&mut out, (file_id, name_type.get_range(), new_text));
            }
        }
    }
    out
}

/// Label references (same file, same-named goto/label name ranges within the same closure; pure syntax).
/// A label is a declaration: when `include_declaration == false`, label positions are excluded.
pub fn label_reference_ranges(
    model: &SalsaSemanticModel<'_>,
    token: &LuaSyntaxToken,
    include_declaration: bool,
) -> Option<Vec<TextRange>> {
    let parent = token.parent()?;
    let name_text = if LuaGotoStat::can_cast(parent.kind().into()) {
        LuaGotoStat::cast(parent.clone())?
            .get_label_name_token()?
            .get_name_text()
            .to_string()
    } else if LuaLabelStat::can_cast(parent.kind().into()) {
        LuaLabelStat::cast(parent.clone())?
            .get_label_name_token()?
            .get_name_text()
            .to_string()
    } else {
        return None;
    };

    let chunk = model.chunk()?;
    let scope_root = parent
        .ancestors()
        .find(|node| matches!(node.kind().into(), LuaSyntaxKind::ClosureExpr))
        .unwrap_or_else(|| chunk.syntax().clone());

    let mut ranges = Vec::new();
    for node_or_token in scope_root.descendants_with_tokens() {
        let rowan::NodeOrToken::Token(node_token) = node_or_token else {
            continue;
        };
        let Some(node_parent) = node_token.parent() else {
            continue;
        };
        if LuaGotoStat::can_cast(node_parent.kind().into()) {
            if LuaGotoStat::cast(node_parent.clone())
                .and_then(|s| s.get_label_name_token())
                .is_some_and(|t| {
                    t.get_name_text() == name_text && t.get_range() == node_token.text_range()
                })
            {
                ranges.push(node_token.text_range());
            }
        } else if LuaLabelStat::can_cast(node_parent.kind().into()) {
            if !include_declaration {
                continue;
            }
            if LuaLabelStat::cast(node_parent.clone())
                .and_then(|s| s.get_label_name_token())
                .is_some_and(|t| {
                    t.get_name_text() == name_text && t.get_range() == node_token.text_range()
                })
            {
                ranges.push(node_token.text_range());
            }
        }
    }
    Some(ranges)
}

/// Label definition position (goto → name range of the same-named label in the same closure; pure syntax).
pub fn label_definition_range(
    model: &SalsaSemanticModel<'_>,
    token: &LuaSyntaxToken,
) -> Option<TextRange> {
    let parent = token.parent()?;
    let name_text = if LuaGotoStat::can_cast(parent.kind().into()) {
        LuaGotoStat::cast(parent.clone())?
            .get_label_name_token()?
            .get_name_text()
            .to_string()
    } else if LuaLabelStat::can_cast(parent.kind().into()) {
        LuaLabelStat::cast(parent.clone())?
            .get_label_name_token()?
            .get_name_text()
            .to_string()
    } else {
        return None;
    };

    let chunk = model.chunk()?;
    let scope_root = parent
        .ancestors()
        .find(|node| matches!(node.kind().into(), LuaSyntaxKind::ClosureExpr))
        .unwrap_or_else(|| chunk.syntax().clone());

    for node_or_token in scope_root.descendants_with_tokens() {
        let rowan::NodeOrToken::Token(node_token) = node_or_token else {
            continue;
        };
        let Some(node_parent) = node_token.parent() else {
            continue;
        };
        if LuaLabelStat::can_cast(node_parent.kind().into())
            && LuaLabelStat::cast(node_parent)
                .and_then(|s| s.get_label_name_token())
                .is_some_and(|t| {
                    t.get_name_text() == name_text && t.get_range() == node_token.text_range()
                })
        {
            return Some(node_token.text_range());
        }
    }
    None
}

/// Type definition identity → TypeDef (after `find_decl` returns `SemanticId::TypeDef`, get the definition details).
pub fn type_def_of_id(model: &SalsaSemanticModel<'_>, id: &SemanticId) -> Option<TypeDef> {
    let SemanticId::TypeDef(key) = id else {
        return None;
    };
    model
        .type_defs_in_scope(key.scope, &key.full_name)
        .into_iter()
        .find(|def| def.id == *id)
}

// ── Helpers ──

fn def_scope(def: &TypeDef) -> emmylua_code_analysis::TypeScope {
    match &def.id {
        SemanticId::TypeDef(key) => key.scope,
        _ => emmylua_code_analysis::TypeScope::Global,
    }
}

fn matches_type_def(model: &SalsaSemanticModel<'_>, def: &TypeDef, name: &str) -> bool {
    if model
        .resolve_type_def(name)
        .is_some_and(|resolved| resolved.id == def.id)
    {
        return true;
    }
    // Full name / short name / namespace-path tail fallback matching (when cross-file resolution does not reach the same id).
    name == def.full_name.as_str()
        || name == def.name.as_str()
        || name.ends_with(&format!(".{}", def.name))
}

fn member_key_text_of(salsa: &SalsaDatabase, member: &SemanticId) -> Option<String> {
    let SemanticId::Member(key) = member else {
        return None;
    };
    let model = SalsaSemanticModel::new(salsa, key.file_id)?;
    let members = model.members()?;
    members
        .iter()
        .find(|m| &m.id == member)
        .map(|m| m.key.to_path())
}

fn replace_last_segment(name: &str, new_name: &str) -> String {
    match name.rfind('.') {
        Some(dot) => format!("{}.{}", &name[..dot], new_name),
        None => new_name.to_string(),
    }
}

fn push_unique(out: &mut Vec<(FileId, TextRange)>, item: (FileId, TextRange)) {
    if !out.contains(&item) {
        out.push(item);
    }
}

fn push_unique_text(out: &mut Vec<(FileId, TextRange, String)>, item: (FileId, TextRange, String)) {
    let key = (item.0, item.1);
    if !out.iter().any(|(f, r, _)| (*f, *r) == key) {
        out.push(item);
    }
}
