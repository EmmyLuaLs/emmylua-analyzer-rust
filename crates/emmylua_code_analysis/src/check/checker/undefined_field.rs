//! # undefined_field: accesses fields that do not exist on a named type
//!
//! M0+: when `resolve_member` returns no result, decide whether to allow based on the prefix type:
//! - Skip Any/Unknown/Table/TableConst (unbound)/Userdata/Global/unconstrained TplRef;
//! - Allow array integer keys; `table<K,V>` checks key type compatibility with K;
//! - Named types: report statically missing members; allow `m[expr]` (string keys) / `[]` inside conditional expressions (except enum);
//! - Enum bound to a table literal: static keys must match an enum member name/value, otherwise report;
//! - Class runtime value bound to a table literal: check against the class member surface.

use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaSyntaxKind, LuaTokenKind};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::salsa_builder::def::{TypeDefKind, TypeVisibility};
use crate::semantic_model::SemanticModel;
use crate::{LuaMemberKey, LuaType, LuaTypeDeclId};

use super::{CheckContext, Checker};

pub struct UndefinedFieldChecker;

impl Checker for UndefinedFieldChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UndefinedField];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for index_expr in root.descendants().filter_map(LuaIndexExpr::cast) {
            check_index(context, semantic_model, &index_expr);
        }
    }
}

fn check_index(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    index_expr: &LuaIndexExpr,
) {
    let Some(resolved) = semantic_model.resolve_member(index_expr) else {
        return;
    };
    if resolved.member_id.is_some() {
        return;
    }
    let Some(prefix) = index_expr.get_prefix_expr() else {
        return;
    };
    if let LuaExpr::NameExpr(name_expr) = &prefix
        && name_expr.get_name_text().is_some_and(|name| {
            matches!(
                name.as_str(),
                "table" | "string" | "math" | "io" | "os" | "coroutine" | "utf8" | "bit32"
            )
        })
    {
        return;
    }
    let prefix_ty = match &prefix {
        LuaExpr::NameExpr(name_expr) => semantic_model
            .resolve_name(name_expr.get_position())
            .map(|decl| semantic_model.type_of_decl_at(&decl, name_expr.get_position()))
            .unwrap_or_else(|| semantic_model.type_of_expr(prefix.get_syntax_id())),
        _ => semantic_model.type_of_expr(prefix.get_syntax_id()),
    };
    if is_invalid_prefix_type(&prefix_ty) {
        return;
    }
    let Some(index_key) = index_expr.get_index_key() else {
        return;
    };
    if is_valid_member(semantic_model, &prefix_ty, &prefix, index_expr, &index_key) {
        return;
    }
    context.add_diagnostic(
        DiagnosticCode::UndefinedField,
        index_key
            .get_range()
            .unwrap_or_else(|| index_expr.get_range()),
        t!(
            "Undefined field: `%{name}`",
            name = index_key.get_path_part()
        ),
    );
}

fn is_invalid_prefix_type(ty: &LuaType) -> bool {
    match ty {
        LuaType::Any
        | LuaType::Unknown
        | LuaType::Table
        | LuaType::Userdata
        | LuaType::Global
        | LuaType::String
        | LuaType::Integer
        | LuaType::Number
        | LuaType::Boolean
        | LuaType::Function
        | LuaType::Thread
        | LuaType::StringConst(_)
        | LuaType::IntegerConst(_)
        | LuaType::FloatConst(_)
        | LuaType::BooleanConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::DocIntegerConst(_)
        | LuaType::DocBooleanConst(_) => true,
        LuaType::TplRef(tpl) => tpl.get_constraint().is_none(),
        LuaType::Instance(instance) => is_invalid_prefix_type(instance.get_base()),
        _ => false,
    }
}

/// Follows alias definitions until a non-alias type is reached. Returns `None` when the
/// input is not an alias (or when an alias cycle/unresolved target prevents expansion).
fn expand_alias_type(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<LuaType> {
    let mut current = ty.clone();
    let mut visited = Vec::new();
    loop {
        let id = match &current {
            LuaType::Ref(id) | LuaType::Def(id) => id.clone(),
            _ => {
                return if visited.is_empty() {
                    None
                } else {
                    Some(current)
                };
            }
        };
        let def = crate::semantic_model::member::type_def_of(semantic_model, &id)?;
        if def.kind != TypeDefKind::Alias {
            return if visited.is_empty() {
                None
            } else {
                Some(current)
            };
        }
        if visited.contains(&id) {
            return None;
        }
        visited.push(id.clone());
        current = semantic_model.alias_target(&def)?;
    }
}

fn is_valid_member(
    semantic_model: &SemanticModel<'_>,
    prefix_ty: &LuaType,
    prefix: &LuaExpr,
    index_expr: &LuaIndexExpr,
    index_key: &LuaIndexKey,
) -> bool {
    // Aliases do not have their own member surface; expand before checking so an alias to
    // `unknown`/`any`/`table`/a class/object behaves like the target type.
    if let Some(expanded) = expand_alias_type(semantic_model, prefix_ty) {
        return is_valid_member(semantic_model, &expanded, prefix, index_expr, index_key);
    }
    match prefix_ty {
        LuaType::TableConst(table) => {
            let Some((decl, def)) = table_binding(semantic_model, table.file_id, table.value)
            else {
                // Unbound table: a local export table is still checked against the export surface (old check_export semantics).
                return !is_export_surface_missing(semantic_model, table, index_expr, index_key);
            };
            // Members of std built-in tables (table/string/...) are not checked for undefined fields.
            if matches!(
                def.name.as_str(),
                "table" | "string" | "math" | "io" | "os" | "coroutine" | "utf8" | "bit32"
            ) {
                return true;
            }
            match def.kind {
                TypeDefKind::Enum => enum_table_access(semantic_model, &decl, &def, index_key),
                TypeDefKind::Class | TypeDefKind::Alias => {
                    let class_ty = class_lua_type(&def);
                    valid_named_member(semantic_model, &class_ty, prefix, index_expr, index_key)
                }
            }
        }
        LuaType::Array(_) => {
            let key_ty = key_type(semantic_model, index_key);
            numeric_key_expanded(semantic_model, &key_ty)
                || is_unknown_key(&key_ty)
                || in_conditional_statement(index_expr)
        }
        LuaType::Generic(generic) => {
            let base = generic.get_base_type_id();
            if base.get_name() != "table" {
                return valid_named_member(
                    semantic_model,
                    prefix_ty,
                    prefix,
                    index_expr,
                    index_key,
                );
            }
            let params = generic.get_params();
            let mut key_ty = key_type(semantic_model, index_key);
            key_ty = resolve_generic_param(semantic_model, &key_ty);
            if is_unknown_key(&key_ty) || is_unconstrained_generic_name(semantic_model, &key_ty) {
                return true;
            }
            params
                .first()
                .is_some_and(|table_key| key_matches(semantic_model, &key_ty, table_key))
                || in_conditional_statement(index_expr)
        }
        LuaType::Ref(_) | LuaType::Def(_) => {
            valid_named_member(semantic_model, prefix_ty, prefix, index_expr, index_key)
        }
        LuaType::Intersection(intersection) => intersection.get_types().iter().any(|component| {
            is_valid_member(semantic_model, component, prefix, index_expr, index_key)
        }),
        LuaType::Union(union) => union.into_vec().iter().any(|component| {
            is_valid_member(semantic_model, component, prefix, index_expr, index_key)
        }),
        LuaType::Object(_) => {
            let key_ty = key_type(semantic_model, index_key);
            if is_unknown_key(&key_ty) || matches!(index_key, LuaIndexKey::Expr(_)) {
                return true;
            }
            if let Some(key) = static_member_key(semantic_model, index_key) {
                return semantic_model.member_info(prefix_ty, &key).is_some();
            }
            false
        }
        _ => true,
    }
}

fn valid_named_member(
    semantic_model: &SemanticModel<'_>,
    prefix_ty: &LuaType,
    prefix: &LuaExpr,
    index_expr: &LuaIndexExpr,
    index_key: &LuaIndexKey,
) -> bool {
    // Generic parameter (projected as Ref("T") in M0 scenarios): decide using the constraint type.
    let prefix_ty = resolve_generic_param(semantic_model, prefix_ty);
    if is_unconstrained_generic_name(semantic_model, &prefix_ty) {
        return true;
    }

    // Mapped type instance (`Required<T> = { [K in keyof T]: ... }`): allow any static key.
    if let Some(def) = named_type_def(semantic_model, &prefix_ty)
        && is_mapped_alias(semantic_model, &def)
    {
        return true;
    }

    // Accessing an enum variable member via `.` is reported (enum is a value, not a table).
    if let Some(def) = named_type_def(semantic_model, &prefix_ty)
        && def.kind == TypeDefKind::Enum
    {
        let direct_enum_table = prefix_directly_names_decl(semantic_model, prefix);
        if !direct_enum_table {
            return false;
        }
    }

    let key_ty = key_type(semantic_model, index_key);
    // Index signatures on named types: `@field [string]` / `@field [integer]`.
    if let Some(def) = named_type_def(semantic_model, &prefix_ty)
        && has_index_signature(semantic_model, &def, &key_ty)
    {
        return true;
    }

    // Static members are checked directly against the member surface.
    if let Some(key) = static_member_key(semantic_model, index_key)
        && semantic_model.member_info(&prefix_ty, &key).is_some()
    {
        return true;
    }
    if is_unknown_key(&key_ty) {
        return true;
    }
    // `m[expr]`: dynamic string / enum alias / enum member table identity keys are allowed broadly on named types (keyof/enum parameter scenarios).
    if matches!(index_key, LuaIndexKey::Expr(_))
        && (is_string_key(&key_ty)
            || is_enum_like_key(semantic_model, &key_ty)
            || matches!(key_ty, LuaType::TableConst(_)))
    {
        return true;
    }
    // `[]` inside a conditional expression is allowed broadly (old check_field semantics).
    if in_conditional_statement(index_expr) && has_bracket(index_expr) {
        return true;
    }
    false
}

fn enum_table_access(
    semantic_model: &SemanticModel<'_>,
    decl: &crate::salsa_builder::def::Decl,
    _def: &crate::salsa_builder::def::TypeDef,
    index_key: &LuaIndexKey,
) -> bool {
    // Dynamic keys (parameter enum type / expression) are allowed broadly.
    if matches!(index_key, LuaIndexKey::Expr(_)) {
        return true;
    }
    let key_text = index_key.get_path_part();
    let mut accepted: Vec<String> = Vec::new();
    for member_ref in semantic_model.members_of_owner(&decl.id) {
        accepted.push(member_ref.name.to_string());
        if let Some(facts) = semantic_model.file_facts_of(member_ref.file_id)
            && let Some(member) = facts.member_by_id(&member_ref.id)
            && let Some(value_syntax) = member.value_syntax
            && let Some(node) = semantic_model
                .syntax_tree()
                .and_then(|tree| value_syntax.to_node_from_root(&tree.get_red_root()))
        {
            let raw = node.text().to_string();
            accepted.push(raw.trim_matches(['"', '\'']).to_string());
        }
    }
    accepted.iter().any(|text| text == &key_text)
}

/// Finds the declaration and same-named type definition bound to a TableConst.
fn table_binding(
    semantic_model: &SemanticModel<'_>,
    file_id: crate::FileId,
    range: TextRange,
) -> Option<(
    crate::salsa_builder::def::Decl,
    crate::salsa_builder::def::TypeDef,
)> {
    let facts = semantic_model.file_facts_of(file_id)?;
    let decl = facts.decls.iter().find(|decl| {
        decl.value_expr_syntax
            .is_some_and(|syntax| syntax.get_range() == range)
    })?;
    let def = facts
        .type_defs
        .iter()
        .find(|def| {
            def.name.eq_ignore_ascii_case(decl.name.as_str())
                || def
                    .full_name
                    .rsplit('.')
                    .next()
                    .is_some_and(|bare| bare.eq_ignore_ascii_case(decl.name.as_str()))
        })
        .or_else(|| {
            // `---@enum K3` + `local apiAlias = {...}`: when names differ, associate with the nearest preceding type definition by position.
            facts
                .type_defs
                .iter()
                .filter(|def| def.name_range.end() <= decl.name_range.start())
                .min_by_key(|def| decl.name_range.start() - def.name_range.end())
        })?
        .clone();
    Some((decl.clone(), def))
}

fn class_lua_type(def: &crate::salsa_builder::def::TypeDef) -> LuaType {
    match def.visibility {
        TypeVisibility::Public => LuaType::Ref(LuaTypeDeclId::global(&def.full_name)),
        _ => LuaType::Def(LuaTypeDeclId::file(def.file_id, &def.full_name)),
    }
}

fn key_type(semantic_model: &SemanticModel<'_>, key: &LuaIndexKey) -> LuaType {
    match key {
        LuaIndexKey::Name(_) | LuaIndexKey::String(_) => LuaType::String,
        LuaIndexKey::Integer(_) | LuaIndexKey::Idx(_) => LuaType::Integer,
        LuaIndexKey::Expr(expr) => semantic_model.type_of_expr(expr.get_syntax_id()),
    }
}

fn is_unknown_key(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Any | LuaType::Unknown | LuaType::Never | LuaType::Table
    ) || matches!(ty, LuaType::Generic(_))
        || matches!(ty, LuaType::TplRef(tpl) if tpl.get_constraint().is_none())
}

fn is_string_key(ty: &LuaType) -> bool {
    match ty {
        LuaType::String
        | LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::StrTplRef(_) => true,
        LuaType::Union(union) => union.into_vec().iter().any(is_string_key),
        _ => false,
    }
}

fn numeric_key(ty: &LuaType) -> bool {
    match ty {
        LuaType::Number
        | LuaType::Integer
        | LuaType::IntegerConst(_)
        | LuaType::FloatConst(_)
        | LuaType::DocIntegerConst(_) => true,
        LuaType::Union(union) => union.into_vec().iter().any(numeric_key),
        LuaType::MultiLineUnion(union) => match union.to_union() {
            LuaType::Union(union) => union.into_vec().iter().any(numeric_key),
            _ => false,
        },
        LuaType::Ref(_) | LuaType::Def(_) => false,
        _ => false,
    }
}

/// Numeric key after alias expansion (`IntegerPartIndex = 1|2`, `NewKey: integer`).
fn numeric_key_expanded(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> bool {
    numeric_key_expanded_inner(semantic_model, ty, &mut Vec::new())
}

fn numeric_key_expanded_inner(
    semantic_model: &SemanticModel<'_>,
    ty: &LuaType,
    visited: &mut Vec<LuaTypeDeclId>,
) -> bool {
    if numeric_key(ty) {
        return true;
    }
    let Some(def) = named_type_def(semantic_model, ty) else {
        return false;
    };
    let id = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return false,
    };
    if visited.contains(id) {
        return false;
    }
    visited.push(id.clone());
    match def.kind {
        TypeDefKind::Alias => semantic_model
            .alias_target(&def)
            .is_some_and(|target| numeric_key_expanded_inner(semantic_model, &target, visited)),
        TypeDefKind::Class => def.super_names.iter().any(|super_name| {
            matches!(super_name.as_str(), "integer" | "number" | "int")
                || semantic_model
                    .type_defs_in_scope(crate::TypeScope::Global, super_name.as_str())
                    .into_iter()
                    .next()
                    .is_some_and(|super_def| {
                        matches!(super_def.name.as_str(), "integer" | "number" | "int")
                    })
        }),
        TypeDefKind::Enum => false,
    }
}

/// Whether `key` can be a key of `table<K,V>` (M0: basic expansion of string/integer and unions/aliases).
fn key_matches(semantic_model: &SemanticModel<'_>, key: &LuaType, table_key: &LuaType) -> bool {
    if table_key.is_unknown() || table_key.is_any() {
        return true;
    }
    match table_key {
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            is_string_key(key)
        }
        LuaType::Integer
        | LuaType::Number
        | LuaType::IntegerConst(_)
        | LuaType::DocIntegerConst(_) => numeric_key_expanded(semantic_model, key),
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .any(|k| key_matches(semantic_model, key, k)),
        LuaType::Ref(_) | LuaType::Def(_) => {
            // Alias / class inheritance: recursive target or parent chain is handled by type_check.
            crate::semantic_model::type_check::is_compatible(semantic_model, key, table_key)
        }
        _ => true,
    }
}

/// Whether the type definition has an index signature matching the key type (`@field [string]` / `@field [integer]`).
fn has_index_signature(
    semantic_model: &SemanticModel<'_>,
    def: &crate::salsa_builder::def::TypeDef,
    key_ty: &LuaType,
) -> bool {
    for member_ref in semantic_model.members_of_owner(&def.id) {
        let Some(facts) = semantic_model.file_facts_of(member_ref.file_id) else {
            continue;
        };
        let Some(member) = facts.member_by_id(&member_ref.id) else {
            continue;
        };
        match member.key.to_path().as_str() {
            "string" => {
                if is_string_key(key_ty) {
                    return true;
                }
            }
            "integer" | "number" => {
                if numeric_key_expanded(semantic_model, key_ty) {
                    return true;
                }
            }
            "any" | "unknown" => return true,
            _ => {}
        }
    }
    false
}

/// Unconstrained generic parameter name (`Ref("T")` with no constraint on T in the signature): allow any member access.
fn is_unconstrained_generic_name(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> bool {
    let id = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return false,
    };
    if named_type_def(semantic_model, ty).is_some() {
        return false;
    }
    let name = id.get_name();
    semantic_model.signatures().is_some_and(|signatures| {
        signatures.iter().any(|signature| {
            signature.docs.as_ref().is_some_and(|docs| {
                docs.generic_params
                    .iter()
                    .any(|param| param.name.as_str() == name && param.constraint.is_none())
            })
        })
    })
}

fn named_type_def(
    semantic_model: &SemanticModel<'_>,
    ty: &LuaType,
) -> Option<crate::salsa_builder::def::TypeDef> {
    let id = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        LuaType::Generic(generic) => {
            return named_type_def(semantic_model, &LuaType::Ref(generic.get_base_type_id()));
        }
        _ => return None,
    };
    crate::semantic_model::member::type_def_of(semantic_model, id)
}

/// Whether the alias target is a mapped type (`{ [K in keyof T]: ... }`).
fn is_mapped_alias(
    semantic_model: &SemanticModel<'_>,
    def: &crate::salsa_builder::def::TypeDef,
) -> bool {
    let Some(syntax) = def.alias_type else {
        return false;
    };
    let Some(tree) = semantic_model.syntax_tree_of(def.file_id) else {
        return false;
    };
    let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
        return false;
    };
    matches!(
        emmylua_parser::LuaDocType::cast(node),
        Some(emmylua_parser::LuaDocType::Mapped(_))
    )
}

/// Whether the prefix expression directly references an enum runtime table (`Enum.A`) rather than an enum-typed variable (`p.a`).
/// Both global and local enum runtime tables count as direct references; pure `---@type` variables do not.
fn prefix_directly_names_decl(semantic_model: &SemanticModel<'_>, prefix: &LuaExpr) -> bool {
    let LuaExpr::NameExpr(name_expr) = prefix else {
        return false;
    };
    let Some(decl_id) = semantic_model.resolve_name(name_expr.get_position()) else {
        return false;
    };
    semantic_model
        .file_facts()
        .and_then(|facts| facts.decl_by_id(&decl_id))
        .is_some_and(|decl| {
            decl.doc_type_syntax.is_none()
                && matches!(
                    decl.kind,
                    crate::salsa_builder::def::DeclKind::Global
                        | crate::salsa_builder::def::DeclKind::Local { .. }
                )
                && decl.value_expr_syntax.is_some_and(|syntax| {
                    let Some(tree) = semantic_model.syntax_tree_of(decl.file_id) else {
                        return false;
                    };
                    let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
                        return false;
                    };
                    matches!(LuaExpr::cast(node), Some(LuaExpr::TableExpr(_)))
                })
        })
}

/// Generic parameter projection fallback (`Ref("T")`): replace with the constraint type declared in the signature doc.
fn resolve_generic_param<'a>(semantic_model: &'a SemanticModel<'_>, ty: &'a LuaType) -> LuaType {
    let id = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return ty.clone(),
    };
    if named_type_def(semantic_model, ty).is_some() {
        return ty.clone();
    }
    let name = id.get_name();
    let Some(signatures) = semantic_model.signatures() else {
        return ty.clone();
    };
    for signature in signatures {
        let Some(docs) = &signature.docs else {
            continue;
        };
        if let Some(param) = docs
            .generic_params
            .iter()
            .find(|param| param.name.as_str() == name)
            && let Some(constraint) = param.constraint
        {
            return semantic_model.doc_type_lua(constraint);
        }
    }
    ty.clone()
}

/// Whether a dynamic key is an enum alias / enum union (`event[EventName]`).
fn is_enum_like_key(semantic_model: &SemanticModel<'_>, key_ty: &LuaType) -> bool {
    is_enum_like_key_inner(semantic_model, key_ty, &mut Vec::new())
}

fn is_enum_like_key_inner(
    semantic_model: &SemanticModel<'_>,
    key_ty: &LuaType,
    visited: &mut Vec<LuaTypeDeclId>,
) -> bool {
    match key_ty {
        LuaType::Ref(_) | LuaType::Def(_) => {
            let Some(def) = named_type_def(semantic_model, key_ty) else {
                return false;
            };
            let id = match key_ty {
                LuaType::Ref(id) | LuaType::Def(id) => id,
                _ => return false,
            };
            if visited.contains(id) {
                return false;
            }
            visited.push(id.clone());
            match def.kind {
                TypeDefKind::Enum => true,
                TypeDefKind::Alias => semantic_model
                    .alias_target(&def)
                    .is_some_and(|target| is_enum_like_key_inner(semantic_model, &target, visited)),
                TypeDefKind::Class => false,
            }
        }
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .any(|component| is_enum_like_key_inner(semantic_model, component, visited)),
        _ => false,
    }
}

/// An unbound TableConst that is the current file's module export table should report UndefinedField for missing members.
fn is_export_surface_missing(
    semantic_model: &SemanticModel<'_>,
    table: &crate::InFiled<TextRange>,
    index_expr: &LuaIndexExpr,
    index_key: &LuaIndexKey,
) -> bool {
    let Some(facts) = semantic_model.file_facts_of(table.file_id) else {
        return false;
    };
    let crate::salsa_builder::def::ModuleExport::Decl { decl, .. } = &facts.module_export else {
        return false;
    };
    let Some(export_decl) = facts.decl_by_id(decl) else {
        return false;
    };
    if export_decl
        .value_expr_syntax
        .is_none_or(|syntax| syntax.get_range() != table.value)
    {
        return false;
    }
    // The export table's own member surface (`export.aaa()` scenario).
    let export_ty = LuaType::TableConst(table.clone());
    if let Some(key) = static_member_key(semantic_model, index_key) {
        return semantic_model.member_info(&export_ty, &key).is_none();
    }
    // Dynamic keys are not checked against the export surface.
    !in_conditional_statement(index_expr) && false
}

fn static_member_key(
    semantic_model: &SemanticModel<'_>,
    key: &LuaIndexKey,
) -> Option<LuaMemberKey> {
    match key {
        LuaIndexKey::Name(name) => Some(LuaMemberKey::Name(name.get_name_text().into())),
        LuaIndexKey::String(string) => Some(LuaMemberKey::Name(string.get_value().into())),
        LuaIndexKey::Integer(integer) => match integer.get_number_value() {
            emmylua_parser::NumberResult::Int(idx) => Some(LuaMemberKey::Integer(idx)),
            _ => None,
        },
        LuaIndexKey::Idx(idx) => Some(LuaMemberKey::Integer(*idx as i64)),
        LuaIndexKey::Expr(expr) => match semantic_model.type_of_expr(expr.get_syntax_id()) {
            LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
                Some(LuaMemberKey::Name(s.as_ref().clone()))
            }
            LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
                Some(LuaMemberKey::Integer(i))
            }
            _ => None,
        },
    }
}

fn has_bracket(index_expr: &LuaIndexExpr) -> bool {
    index_expr
        .syntax()
        .children_with_tokens()
        .any(|child| child.kind() == LuaTokenKind::TkLeftBracket.into())
}

/// Whether the node is in the condition expression of an if/while/for/repeat statement.
fn in_conditional_statement(index_expr: &LuaIndexExpr) -> bool {
    let range = index_expr.get_range();
    for ancestor in index_expr.syntax().ancestors() {
        let kind: LuaSyntaxKind = ancestor.kind().into();
        match kind {
            LuaSyntaxKind::IfStat => {
                if let Some(condition) = emmylua_parser::LuaIfStat::cast(ancestor)
                    .and_then(|stat| stat.get_condition_expr())
                    && condition.get_range().contains_range(range)
                {
                    return true;
                }
            }
            LuaSyntaxKind::WhileStat => {
                if let Some(condition) = emmylua_parser::LuaWhileStat::cast(ancestor)
                    .and_then(|stat| stat.get_condition_expr())
                    && condition.get_range().contains_range(range)
                {
                    return true;
                }
            }
            LuaSyntaxKind::ElseIfClauseStat => {
                if let Some(condition) = emmylua_parser::LuaElseIfClauseStat::cast(ancestor)
                    .and_then(|stat| stat.get_condition_expr())
                    && condition.get_range().contains_range(range)
                {
                    return true;
                }
            }
            LuaSyntaxKind::RepeatStat => {
                if let Some(condition) = emmylua_parser::LuaRepeatStat::cast(ancestor)
                    .and_then(|stat| stat.get_condition_expr())
                    && condition.get_range().contains_range(range)
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}
