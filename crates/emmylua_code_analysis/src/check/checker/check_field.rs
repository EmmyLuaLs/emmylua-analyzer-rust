use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr, LuaIndexKey,
    LuaLocalStat, LuaTableExpr, LuaVarExpr,
};

use crate::DiagnosticCode;
use crate::salsa_builder::def::SemanticId;
use crate::semantic_model::SemanticModel;
use crate::{LuaMemberKey, LuaType};

use super::{CheckContext, Checker};
use crate::semantic_model::render::humanize_type;

pub struct CheckFieldChecker;

impl Checker for CheckFieldChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::MissingFields, DiagnosticCode::InjectField];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for node in root.descendants().filter_map(LuaAst::cast) {
            match node {
                LuaAst::LuaLocalStat(local_stat) => {
                    check_local(context, semantic_model, &local_stat);
                }
                LuaAst::LuaAssignStat(assign_stat) => {
                    check_assign(context, semantic_model, &assign_stat);
                    check_index_assign(context, semantic_model, &assign_stat);
                }
                LuaAst::LuaCallExpr(call_expr) => {
                    check_call_args(context, semantic_model, &call_expr)
                }
                _ => {}
            }
        }
    }
}

/// `---@type C\nlocal c = { ... }`.
fn check_local(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    local_stat: &LuaLocalStat,
) {
    let name_list = local_stat.get_local_name_list().collect::<Vec<_>>();
    let value_exprs = local_stat.get_value_exprs().collect::<Vec<_>>();
    for (index, local_name) in name_list.iter().enumerate() {
        let Some(LuaExpr::TableExpr(table)) = value_exprs.get(index) else {
            continue;
        };
        let Some(decl_id) = semantic_model.decl_by_offset(local_name.get_range().start()) else {
            continue;
        };
        check_against(context, semantic_model, &decl_id, table);
    }
}

/// `c = { ... }` (c has a doc type).
fn check_assign(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    assign_stat: &LuaAssignStat,
) {
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    for (var, expr) in vars.iter().zip(exprs.iter()) {
        let LuaExpr::TableExpr(table) = expr else {
            continue;
        };
        let LuaVarExpr::NameExpr(name_expr) = var else {
            continue;
        };
        let Some(decl_id) = semantic_model.resolve_name(name_expr.get_position()) else {
            continue;
        };
        check_against(context, semantic_model, &decl_id, table);
    }
}

/// `test.a = 1` (test is a named type): if the target type has no such static member -> InjectField.
/// M0: only handle named classes without a parent type; dynamic keys, index signatures, and Object types are left for later.
fn check_index_assign(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    assign_stat: &LuaAssignStat,
) {
    let (vars, _) = assign_stat.get_var_and_expr_list();
    for var in vars {
        let LuaVarExpr::IndexExpr(index_expr) = var else {
            continue;
        };
        let Some(prefix) = index_expr.get_prefix_expr() else {
            continue;
        };
        let prefix_ty = semantic_model.type_of_expr(prefix.get_syntax_id());
        let Some(index_key) = index_expr.get_index_key() else {
            continue;
        };
        let Some(key) = static_member_key(semantic_model, &index_key) else {
            continue;
        };
        // Aliases must be expanded before deciding whether a field can be injected:
        // `---@alias Anything unknown` should allow `a.foo = 1`, while a class alias should
        // use the underlying class member surface.
        let prefix_ty = expand_alias_type(semantic_model, &prefix_ty).unwrap_or(prefix_ty);
        // `---@type { [number]: number }`: keys outside the object's index signature report injection errors.
        if let LuaType::Object(object) = &prefix_ty {
            if object.get_field(&key).is_none()
                && !object
                    .get_index_access()
                    .iter()
                    .any(|(key_ty, _)| index_key_type_accepts(key_ty, &key))
            {
                context.add_diagnostic(
                    DiagnosticCode::InjectField,
                    index_key
                        .get_range()
                        .unwrap_or_else(|| index_expr.get_range()),
                    t!(
                        "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                        class = humanize_type(semantic_model, &prefix_ty),
                        field = index_key.get_path_part()
                    ),
                );
            }
            continue;
        }
        let Some(target_id) = resolve_target(semantic_model, &prefix_ty) else {
            continue;
        };
        let Some(def) = find_type_def(semantic_model, &target_id) else {
            continue;
        };
        // Classes with a parent type or index signature (`[string]`) are handled broadly (M0).
        if !def.super_names.is_empty()
            || semantic_model
                .members_of_owner(&target_id)
                .iter()
                .any(|member| member.name.starts_with('['))
            || is_mapped_alias(semantic_model, &def)
        {
            continue;
        }
        if semantic_model.member_info(&prefix_ty, &key).is_some() {
            continue;
        }
        context.add_diagnostic(
            DiagnosticCode::InjectField,
            index_key
                .get_range()
                .unwrap_or_else(|| index_expr.get_range()),
            t!(
                "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                class = humanize_type(semantic_model, &prefix_ty),
                field = index_key.get_path_part()
            ),
        );
    }
}

/// Follows alias definitions until a non-alias type is reached. Returns `None` when the
/// input is not an alias (or an alias cycle/unresolved target prevents expansion).
fn expand_alias_type(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<LuaType> {
    let mut current = ty.clone();
    let mut visited = Vec::new();
    loop {
        let id = match &current {
            LuaType::Ref(id) | LuaType::Def(id) => id.clone(),
            _ => return if visited.is_empty() { None } else { Some(current) },
        };
        let def = crate::semantic_model::member::type_def_of(semantic_model, &id)?;
        if def.kind != crate::salsa_builder::def::TypeDefKind::Alias {
            return if visited.is_empty() { None } else { Some(current) };
        }
        if visited.contains(&id) {
            return None;
        }
        visited.push(id.clone());
        current = semantic_model.alias_target(&def)?;
    }
}

/// Static string key -> `LuaMemberKey` (dynamic keys are not checked).
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

/// Whether an object index-signature key type accepts this member key (`{[number]: ...}` does not accept string keys).
fn index_key_type_accepts(key_ty: &LuaType, member_key: &LuaMemberKey) -> bool {
    match member_key {
        LuaMemberKey::Name(_) => key_type_contains(key_ty, &LuaType::String),
        LuaMemberKey::Integer(_) => key_type_contains(key_ty, &LuaType::Integer),
        _ => true,
    }
}

fn key_type_contains(key_ty: &LuaType, expected: &LuaType) -> bool {
    match key_ty {
        LuaType::Any | LuaType::Unknown => true,
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .any(|ty| key_type_contains(&ty, expected)),
        _ => key_ty == expected || (expected == &LuaType::Integer && key_ty == &LuaType::Number),
    }
}

/// `---@class C: { a: number }`: when the parent type is an object literal, complete the field surface.
fn object_super_fields(
    semantic_model: &SemanticModel<'_>,
    def: &crate::salsa_builder::def::TypeDef,
) -> Vec<(String, LuaType)> {
    let Some(tree) = semantic_model.syntax_tree_of(def.file_id) else {
        return Vec::new();
    };
    let root = tree.get_red_root();
    let mut out = Vec::new();
    for class_tag in root
        .descendants()
        .filter_map(emmylua_parser::LuaDocTagClass::cast)
    {
        let Some(name_token) = class_tag.get_name_token() else {
            continue;
        };
        if name_token.syntax().text_range() != def.name_range {
            continue;
        }
        let Some(supers) = class_tag.get_supers() else {
            continue;
        };
        for super_ty in supers.get_types() {
            let emmylua_parser::LuaDocType::Object(object) = super_ty else {
                continue;
            };
            for field in object.get_fields() {
                let Some(key) = field.get_field_key() else {
                    continue;
                };
                let name = match &key {
                    emmylua_parser::LuaDocObjectFieldKey::Name(name) => {
                        name.get_name_text().to_string()
                    }
                    emmylua_parser::LuaDocObjectFieldKey::String(str) => str.get_value(),
                    _ => continue,
                };
                let ty = field
                    .get_type()
                    .map(|ty| semantic_model.doc_type_lua_rich_in(def.file_id, ty.get_syntax_id()))
                    .unwrap_or(LuaType::Unknown);
                if !out.iter().any(|(n, _)| n == &name) {
                    out.push((name, ty));
                }
            }
        }
    }
    out
}

/// Whether the alias target is a mapped type.
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

/// `f({ ... })`: check call-argument table literals for missing/extra fields against the parameter type.
fn check_call_args(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    let Some(callee) = call_expr.get_prefix_expr() else {
        return;
    };
    let candidates = super::param_type_check::callable_candidates(semantic_model, &callee);
    let Some(args) = call_expr.get_args_list() else {
        return;
    };
    for candidate in &candidates {
        let params = candidate.get_params();
        for (index, arg) in args.get_args().enumerate() {
            let LuaExpr::TableExpr(table) = arg else {
                continue;
            };
            let Some((_, Some(param_ty))) = params.get(index) else {
                continue;
            };
            check_against_type(context, semantic_model, param_ty, &table);
        }
    }
}

fn check_against(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    decl_id: &SemanticId,
    table: &LuaTableExpr,
) {
    // Target type: projection of the declared doc annotation.
    let Some(facts) = semantic_model.file_facts() else {
        return;
    };
    let Some(decl) = facts.decl_by_id(decl_id) else {
        return;
    };
    let Some(type_syntax) = decl.doc_type_syntax else {
        return;
    };
    let target = semantic_model.doc_type_lua(type_syntax);
    check_against_type(context, semantic_model, &target, table);
}

fn check_against_type(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    table: &LuaTableExpr,
) {
    // Expand aliases before checking table literals against them. This makes alias-to-object,
    // alias-to-class and alias-to-unknown behave like their target types.
    if let Some(expanded) = expand_alias_type(semantic_model, target) {
        check_against_type(context, semantic_model, &expanded, table);
        return;
    }
    match target {
        LuaType::Intersection(intersection) => {
            for component in intersection.get_types() {
                check_against_type(context, semantic_model, component, table);
            }
            return;
        }
        LuaType::Union(union) => {
            // Check the first named-class component; empty tables pass through the array component.
            let fields_count = table.get_fields_with_keys().len();
            for component in union.into_vec() {
                let named = matches!(
                    component,
                    LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_)
                );
                if named && fields_count > 0 {
                    check_against_type(context, semantic_model, &component, table);
                    return;
                }
                if matches!(component, LuaType::Array(_)) && fields_count == 0 {
                    return;
                }
            }
            return;
        }
        LuaType::Object(object) => {
            let fields = table.get_fields_with_keys().to_vec();
            let mut missing = Vec::new();
            for (key, ty) in object.get_fields() {
                let name = match key {
                    LuaMemberKey::Name(name) => name.to_string(),
                    LuaMemberKey::Integer(i) => i.to_string(),
                    _ => continue,
                };
                if !fields
                    .iter()
                    .any(|(_, field_key)| field_key.get_path_part() == name)
                    && !ty.is_nullable()
                {
                    missing.push(name);
                }
            }
            if !missing.is_empty() {
                context.add_diagnostic(
                    DiagnosticCode::MissingFields,
                    table.get_range(),
                    format!(
                        "Missing required fields: {}",
                        missing
                            .iter()
                            .map(|name| format!("`{}`", name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
            return;
        }
        _ => {}
    }
    let Some(target_id) = resolve_target(semantic_model, target) else {
        return;
    };

    // Target type's @field members (including inheritance).
    let mut members: Vec<(String, LuaType)> = Vec::new();
    let mut visited = Vec::new();
    collect_fields(semantic_model, &target_id, &mut visited, &mut members);
    if let Some(def) = find_type_def(semantic_model, &target_id) {
        members.extend(object_super_fields(semantic_model, &def));
    }

    let fields: Vec<String> = table
        .get_fields_with_keys()
        .into_iter()
        .map(|(_, key)| key.get_path_part())
        .collect();

    // InjectField: table field is not in the target members.
    for field in &fields {
        if !members.iter().any(|(name, _)| name == field) {
            context.add_diagnostic(
                DiagnosticCode::InjectField,
                table.get_range(),
                format!("Field `{}` cannot be injected into the target type.", field),
            );
        }
    }

    // MissingFields: required (non-nullable) members are missing.
    let missing: Vec<&str> = members
        .iter()
        .filter(|(name, ty)| !fields.contains(name) && !is_optional(ty))
        .map(|(name, _)| name.as_str())
        .collect();
    if !missing.is_empty() {
        context.add_diagnostic(
            DiagnosticCode::MissingFields,
            table.get_range(),
            format!(
                "Missing required fields: {}",
                missing
                    .iter()
                    .map(|name| format!("`{}`", name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
}

/// Target type -> named type id (Ref/Def/Generic base class).
fn resolve_target(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<SemanticId> {
    match ty {
        LuaType::Ref(id) | LuaType::Def(id) => {
            crate::semantic_model::member::type_def_of(semantic_model, id).map(|def| def.id)
        }
        LuaType::Generic(generic) => {
            crate::semantic_model::member::type_def_of(semantic_model, &generic.get_base_type_id())
                .map(|def| def.id)
        }
        _ => None,
    }
}

/// Type definition's `@field` members (including the inheritance chain; `visited` prevents cycles).
fn collect_fields(
    semantic_model: &SemanticModel<'_>,
    type_def_id: &SemanticId,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<(String, LuaType)>,
) {
    if visited.contains(type_def_id) {
        return;
    }
    visited.push(type_def_id.clone());
    for member_ref in semantic_model.members_of_owner(type_def_id) {
        let name = member_ref.name.to_string();
        let mut ty = semantic_model
            .type_of_member(&member_ref.id)
            .unwrap_or(LuaType::Unknown);
        if let LuaType::Ref(id) | LuaType::Def(id) = &ty
            && let Some(def) = semantic_model.type_def_of(id)
            && let Some(alias_target) = semantic_model.alias_target(&def)
        {
            ty = alias_target;
        }
        if let Some(facts) = semantic_model.file_facts_of(member_ref.file_id)
            && let Some(member) = facts.member_by_id(&member_ref.id)
            && member.is_nullable
            && !ty.is_nullable()
        {
            ty = LuaType::Union(std::sync::Arc::new(crate::LuaUnionType::from_vec(vec![
                ty,
                LuaType::Nil,
            ])));
        }
        if !out.iter().any(|(n, _)| n == &name) {
            out.push((name, ty));
        }
    }
    // Inheritance chain: locate the type definition; needs def info.
    if let Some(def) = find_type_def(semantic_model, type_def_id) {
        for super_name in &def.super_names {
            if let Some(super_def) = super_type_def(semantic_model, super_name) {
                collect_fields(semantic_model, &super_def.id, visited, out);
            }
        }
    }
}

/// Find a type definition by id (walk the workspace type index).
fn find_type_def(
    semantic_model: &SemanticModel<'_>,
    type_def_id: &SemanticId,
) -> Option<crate::salsa_builder::def::TypeDef> {
    let SemanticId::TypeDef(key) = type_def_id else {
        return None;
    };
    let defs = semantic_model.type_defs_in_scope(key.scope, &key.full_name);
    defs.into_iter().next()
}

/// Parent type (by full name, in Global scope).
fn super_type_def(
    semantic_model: &SemanticModel<'_>,
    full_name: &str,
) -> Option<crate::salsa_builder::def::TypeDef> {
    semantic_model
        .type_defs_in_scope(crate::salsa_builder::def::TypeScope::Global, full_name)
        .into_iter()
        .next()
}

/// Whether the field is optional (nullable / unknown).
fn is_optional(ty: &LuaType) -> bool {
    match ty {
        LuaType::Any | LuaType::Unknown | LuaType::Nil => true,
        LuaType::Union(union) => union.into_vec().iter().any(|t| matches!(t, LuaType::Nil)),
        _ => false,
    }
}
