use std::rc::Rc;

use std::collections::{HashMap, HashSet};

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaIndexExpr,
    LuaIndexKey, LuaLiteralToken, LuaLocalStat, LuaTableExpr, LuaVarExpr, NumberResult,
};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::type_check::{TypeCheckFailReason, check_type_detail, is_compatible};
use crate::{Decl, LuaMemberKey, LuaType, LuaTypeNode, SemanticId, TypeDef, TypeDefKind};

use super::{CheckContext, Checker, check_field::static_member_key};
use crate::semantic_model::render::humanize_type;

pub struct AssignTypeMismatchChecker;

pub(super) struct TypeMismatch {
    pub(super) range: rowan::TextRange,
    pub(super) message: String,
}

impl Checker for AssignTypeMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::AssignTypeMismatch];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for node in root.descendants().filter_map(LuaAst::cast) {
            match node {
                LuaAst::LuaLocalStat(stat) => check_local(context, semantic_model, &stat),
                LuaAst::LuaAssignStat(stat) => check_assign(context, semantic_model, &stat),
                LuaAst::LuaTableExpr(table) => {
                    check_inline_table_fields(context, semantic_model, &table)
                }
                _ => {}
            }
        }
    }
}

/// `---@generic T: string` plus `` `T` `` parameter: a string argument that resolves to a class name not satisfying the constraint yields AssignTypeMismatch.
fn check_str_tpl_constraint_call(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call: &LuaCallExpr,
) -> bool {
    let Some(callee) = call.get_prefix_expr() else {
        return false;
    };
    let LuaExpr::NameExpr(callee_name) = &callee else {
        return false;
    };
    let Some(decl) = semantic_model.resolve_name(callee_name.get_position()) else {
        return false;
    };
    let Some(signature) = semantic_model.type_of_decl_signature(&decl) else {
        return false;
    };
    let generic_params = signature.get_generic_params();
    let constraints: Vec<(String, LuaType)> = generic_params
        .iter()
        .filter_map(|param| {
            param
                .get_constraint()
                .map(|constraint| (param.get_name().to_string(), constraint.clone()))
        })
        .collect();
    if constraints.is_empty() {
        return false;
    }
    let Some(args) = call.get_args_list() else {
        return false;
    };
    for (index, arg) in args.get_args().enumerate() {
        let Some((_, Some(param_ty))) = signature.get_params().get(index) else {
            continue;
        };
        if !str_tpl_contains_tpl(param_ty) {
            continue;
        }
        let LuaExpr::LiteralExpr(literal) = &arg else {
            continue;
        };
        let Some(LuaLiteralToken::String(str_token)) = literal.get_literal() else {
            continue;
        };
        let full_name = str_token.get_value();
        let resolved = semantic_model
            .resolve_type_def(&full_name)
            .map(|def| semantic_model.type_def_ref(&def))
            .or_else(|| {
                semantic_model.file_facts().and_then(|facts| {
                    facts
                        .type_defs
                        .iter()
                        .find(|def| def.name.to_lowercase() == full_name.to_lowercase())
                        .map(|def| semantic_model.type_def_ref(def))
                })
            });
        let Some(resolved) = resolved else {
            continue;
        };
        for (name, constraint) in &constraints {
            if str_tpl_uses_name(param_ty, name)
                && !is_compatible(semantic_model, &resolved, constraint)
            {
                add_mismatch(context, arg.get_range(), constraint, &resolved);
                return true;
            }
        }
    }
    false
}

fn str_tpl_contains_tpl(ty: &LuaType) -> bool {
    match ty {
        LuaType::StrTplRef(_) => true,
        LuaType::Union(union) => union.into_vec().iter().any(str_tpl_contains_tpl),
        _ => false,
    }
}

fn str_tpl_uses_name(ty: &LuaType, name: &str) -> bool {
    match ty {
        LuaType::StrTplRef(tpl) => tpl.get_name() == name,
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .any(|component| str_tpl_uses_name(component, name)),
        _ => false,
    }
}

fn check_inline_table_fields(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    table: &LuaTableExpr,
) {
    let owner = SemanticId::member(semantic_model.file_id(), table.get_range());
    for member_ref in semantic_model.members_of_owner(&owner).iter() {
        let Some(facts) = semantic_model.file_facts_of(member_ref.file_id) else {
            continue;
        };
        let Some(member) = facts.member_by_id(&member_ref.id) else {
            continue;
        };
        let Some(doc_syntax) = member.doc_type_syntax else {
            continue;
        };
        let Some(value_expr) = member
            .value_syntax
            .and_then(|syntax| {
                semantic_model
                    .syntax_tree()
                    .and_then(|tree| syntax.to_node_from_root(&tree.get_red_root()))
            })
            .and_then(LuaExpr::cast)
        else {
            continue;
        };
        let target = semantic_model.doc_type_lua(doc_syntax);
        let value_ty = semantic_model.type_of_expr(value_expr.get_syntax_id());
        if matches!(
            value_ty,
            LuaType::Unknown | LuaType::Any | LuaType::Never | LuaType::Nil
        ) || is_compatible(semantic_model, &value_ty, &target)
        {
            continue;
        }
        add_mismatch(context, value_expr.get_range(), &target, &value_ty);
    }
}

fn check_local(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    stat: &LuaLocalStat,
) {
    let name_list = stat.get_local_name_list().collect::<Vec<_>>();
    let value_exprs = stat.get_value_exprs().collect::<Vec<_>>();
    for (index, local_name) in name_list.iter().enumerate() {
        let Some(expr) = value_exprs.get(index) else {
            break;
        };
        let Some(decl_id) = semantic_model.decl_by_offset(local_name.get_range().start()) else {
            continue;
        };
        let Some(facts) = semantic_model.file_facts() else {
            continue;
        };
        let Some(decl) = facts.decl_by_id(&decl_id) else {
            continue;
        };
        // `local class = meta("class")`: `T: string` plus a `` `T` `` string template argument resolves to a class name.
        if let LuaExpr::CallExpr(call) = expr
            && check_str_tpl_constraint_call(context, semantic_model, call)
        {
            continue;
        }
        if decl.doc_type_syntax.is_none() {
            // `---@class B` directly attached to `local b = a`: use the type definition as the declaration contract.
            if let Some(target) = class_contract_for_decl(semantic_model, &decl) {
                // `local B = setmetatable({}, A)` is the standard class-inheritance idiom: B's runtime
                // table uses A as its metatable, so an RHS type of A is expected and must not be treated as a base-class assignment mismatch.
                if let LuaExpr::CallExpr(call) = expr
                    && is_setmetatable_call(call)
                {
                    continue;
                }
                check_value_against(context, semantic_model, &target, &expr);
            }
            continue;
        }
        check_against(context, semantic_model, &decl_id, &expr);
    }
}

fn is_setmetatable_call(call: &LuaCallExpr) -> bool {
    call.get_prefix_expr().is_some_and(|prefix| match prefix {
        LuaExpr::NameExpr(name_expr) => {
            name_expr.get_name_text().as_deref() == Some("setmetatable")
        }
        LuaExpr::IndexExpr(index_expr) => index_expr
            .get_index_name_token()
            .is_some_and(|name| name.text() == "setmetatable"),
        _ => false,
    })
}

fn check_assign(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    stat: &LuaAssignStat,
) {
    let (vars, exprs) = stat.get_var_and_expr_list();
    for (var, expr) in vars.iter().zip(exprs.iter()) {
        match var {
            LuaVarExpr::NameExpr(name_expr) => {
                let Some(decl_id) = semantic_model.resolve_name(name_expr.get_position()) else {
                    // `---@type C` on the next line before `_ = c`: `_` is not in facts, so the comment attaches directly to the assignment statement.
                    if name_expr.get_text() == "_"
                        && let Some(target) = preceding_doc_type_target(semantic_model, stat)
                    {
                        check_value_against(context, semantic_model, &target, &expr);
                    }
                    continue;
                };
                check_assign_against(
                    context,
                    semantic_model,
                    &decl_id,
                    name_expr.get_position(),
                    &expr,
                );
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                let Some(resolved) = semantic_model.resolve_member(index_expr) else {
                    check_self_member_assign(context, semantic_model, index_expr, &expr);
                    continue;
                };
                let Some(member_id) = resolved.member_id else {
                    check_self_member_assign(context, semantic_model, index_expr, &expr);
                    continue;
                };
                let owner_is_self = semantic_model
                    .file_facts()
                    .and_then(|facts| facts.member_by_id(&member_id))
                    .is_some_and(|member| {
                        matches!(
                            member.owner,
                            SemanticId::Name(ref name)
                                if name.as_str() == "self"
                        )
                    });
                if owner_is_self {
                    check_self_member_assign(context, semantic_model, index_expr, &expr);
                    continue;
                }
                check_member_assign(
                    context,
                    semantic_model,
                    &member_id,
                    index_expr.get_range().start(),
                    &expr,
                );
            }
        }
    }
}

fn check_against(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    decl_id: &SemanticId,
    value_expr: &LuaExpr,
) {
    let Some(target) = semantic_model.type_of_decl(decl_id) else {
        return;
    };
    check_value_against(context, semantic_model, &target, value_expr);
}

/// Reassignment `x = value`: use the flow type at the offset as the target type (the current assignment is excluded,
/// `---@cast` widening applies; branch narrowing does not alter the declaration contract).
fn check_assign_against(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    decl_id: &SemanticId,
    target_offset: rowan::TextSize,
    value_expr: &LuaExpr,
) {
    let target = semantic_model.type_of_decl_assign_target_at(decl_id, target_offset);
    check_value_against(context, semantic_model, &target, value_expr);
}

/// `function mt:init() self.x = value`: self member falls back to the same-named member on the method owner.
fn check_self_member_assign(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    index_expr: &LuaIndexExpr,
    value_expr: &LuaExpr,
) -> Option<()> {
    let prefix = index_expr.get_prefix_expr()?;
    let LuaExpr::NameExpr(name_expr) = &prefix else {
        return None;
    };
    if name_expr.get_text() != "self" {
        return None;
    }
    let key = index_expr
        .get_index_key()
        .map(|key| key.get_path_part())
        .unwrap_or_default();
    let closure = index_expr.ancestors::<LuaClosureExpr>().next()?;
    let facts = semantic_model.file_facts()?;
    let methods: Vec<_> = facts
        .members
        .iter()
        .filter(|member| member.value_syntax == Some(closure.get_syntax_id()))
        .collect();
    let method = methods.into_iter().find(|member| member.is_method)?;
    let owner_ty = semantic_model
        .type_of_decl(&method.owner)
        .unwrap_or(LuaType::Unknown);
    // The owner's VM type may be Unknown: directly take the declaration type of the same-named runtime member on the owner.
    let target = facts
        .members
        .iter()
        .find(|member| {
            member.owner == method.owner
                && member.key.name() == Some(key.as_str())
                && member.value_syntax != Some(value_expr.get_syntax_id())
        })
        .map(|member| semantic_model.type_of_member_at(&member.id, index_expr.get_range().start()))
        .or_else(|| {
            semantic_model.member_type(&owner_ty, &LuaMemberKey::Name(key.clone().into()))
        })?;
    check_value_against(context, semantic_model, &target, value_expr);
    Some(())
}

fn check_member_assign(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    member_id: &SemanticId,
    target_offset: rowan::TextSize,
    value_expr: &LuaExpr,
) {
    let mut target = semantic_model.member_assignment_type(
        member_id,
        semantic_model.type_of_member_at(member_id, target_offset),
    );
    // `t.x = value`: resolve_member first returns the synthesized member for this assignment; prefer the same-named member on the prefix's declared type.
    if let Some(facts) = semantic_model.file_facts()
        && let Some(member) = facts.member_by_id(member_id)
    {
        if member.value_syntax == Some(value_expr.get_syntax_id()) {
            let SemanticId::Decl(owner) = &member.owner else {
                return check_value_against_target(context, semantic_model, &target, value_expr);
            };
            let prefix_ty = semantic_model
                .type_of_decl_assign_target_at(&SemanticId::Decl(owner.clone()), target_offset);
            let key = member.key.clone();
            let infos = semantic_model.member_infos_with_key(&prefix_ty, &key);
            let declared_ty = infos
                .into_iter()
                .find(|info| info.id.as_ref() != Some(member_id))
                .map(|info| match info.id {
                    Some(id) => semantic_model.member_assignment_type(&id, info.typ),
                    None => info.typ,
                })
                .or_else(|| semantic_model.member_type(&prefix_ty, &key));
            if let Some(declared_ty) = declared_ty {
                target = declared_ty;
            }
        }
    }
    let value_ty =
        semantic_model.type_of_expr_at(value_expr.get_syntax_id(), value_expr.get_range().start());
    if matches!(value_ty, LuaType::Nil) || matches!(target, LuaType::Nil) {
        return;
    }
    check_value_against_target(context, semantic_model, &target, value_expr);
}

fn check_value_against_target(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    value_expr: &LuaExpr,
) {
    if target.contains_tpl_node() {
        // Skip when class-level generic projection is incomplete, avoiding false positives for conditional generics like `MockResult<MockReturnType<T>>`.
        return;
    }
    check_value_against(context, semantic_model, target, value_expr);
}

fn is_structured_table_target(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> bool {
    match ty {
        LuaType::Array(_) | LuaType::Object(_) => true,
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .any(|component| is_structured_table_target(semantic_model, component)),
        LuaType::Generic(generic) => {
            generic.get_base_type_id().get_name() == "array"
                || is_structured_table_target(
                    semantic_model,
                    &LuaType::Ref(generic.get_base_type_id()),
                )
        }
        LuaType::Ref(_) | LuaType::Def(_) => {
            let Some(def) = named_def(semantic_model, ty) else {
                return false;
            };
            match def.kind {
                TypeDefKind::Class => true,
                TypeDefKind::Alias => semantic_model
                    .alias_target(&def)
                    .is_some_and(|target| is_structured_table_target(semantic_model, &target)),
                TypeDefKind::Enum => false,
            }
        }
        _ => false,
    }
}

fn check_value_against(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    value_expr: &LuaExpr,
) {
    let mut mismatches = Vec::new();
    collect_value_mismatches(&mut mismatches, semantic_model, target, value_expr);
    for mismatch in mismatches {
        context.add_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            mismatch.range,
            mismatch.message,
        );
    }
}

fn collect_value_mismatches(
    mismatches: &mut Vec<TypeMismatch>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    value_expr: &LuaExpr,
) {
    if let LuaExpr::TableExpr(table) = value_expr {
        // Plain table target without a declared type: keep old behavior — don't check each field or report a type mismatch (same table identity).
        if matches!(
            target,
            LuaType::Table | LuaType::TableConst(_) | LuaType::TableGeneric(_)
        ) {
            return;
        }
        if let LuaType::Generic(generic) = target
            && generic.get_base_type_id().get_name() == "table"
        {
            return;
        }
        if is_structured_table_target(semantic_model, target) {
            collect_table_field_mismatches(mismatches, semantic_model, target, table);
        } else {
            let value_ty = semantic_model
                .type_of_expr_at(value_expr.get_syntax_id(), value_expr.get_range().start());
            if matches!(value_ty, LuaType::Unknown | LuaType::Any | LuaType::Never) {
                return;
            }
            // Compatibility resolves tuple aliases and unions. Keep the intersection guard:
            // the general check currently accepts a table matching only one component.
            if !matches!(target, LuaType::Intersection(_))
                && is_compatible(semantic_model, &value_ty, target)
            {
                return;
            }
            mismatches.push(type_mismatch(
                semantic_model,
                value_expr.get_range(),
                target,
                &value_ty,
            ));
        }
        return;
    }
    let value_ty = assignment_value_type(semantic_model, value_expr);
    if matches!(value_ty, LuaType::Unknown | LuaType::Any | LuaType::Never)
        || is_compatible(semantic_model, &value_ty, target)
    {
        return;
    }
    mismatches.push(type_mismatch(
        semantic_model,
        value_expr.get_range(),
        target,
        &value_ty,
    ));
}

/// Assignment RHS type: when VM yields `Function`, if the RHS is a named function, resolve its cross-file signature structure.
fn assignment_value_type(semantic_model: &SemanticModel<'_>, value_expr: &LuaExpr) -> LuaType {
    let ty =
        semantic_model.type_of_expr_at(value_expr.get_syntax_id(), value_expr.get_range().start());
    if matches!(ty, LuaType::Function)
        && let LuaExpr::NameExpr(name_expr) = value_expr
        && let Some(decl) = semantic_model.resolve_name(name_expr.get_position())
        && let Some(fun) = semantic_model.type_of_decl_signature(&decl)
    {
        return LuaType::DocFunction(Rc::new(fun));
    }
    resolve_unbound_generic(semantic_model, ty)
}

/// `Ref("T")` with a signature declaring `---@generic T: Animal` -> check against the constraint Animal.
fn resolve_unbound_generic(semantic_model: &SemanticModel<'_>, ty: LuaType) -> LuaType {
    let (LuaType::Ref(id) | LuaType::Def(id)) = &ty else {
        return ty;
    };
    if semantic_model.type_def_of(id).is_some() {
        return ty;
    }
    let name = id.get_name();
    let Some(signatures) = semantic_model.signatures() else {
        return ty;
    };
    for signature in signatures {
        let Some(docs) = signature.docs.as_ref() else {
            continue;
        };
        if let Some(param) = docs
            .generic_params
            .iter()
            .find(|param| param.name.as_str() == name)
            && let Some(constraint) = param.constraint
        {
            let constraint_ty = semantic_model.doc_type_lua_rich(constraint);
            if !matches!(constraint_ty, LuaType::Unknown) {
                return constraint_ty;
            }
        }
    }
    ty
}

/// Let argument checking collect field errors after it substitutes call generics.
pub(super) fn table_field_mismatches(
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    table: &LuaTableExpr,
) -> Vec<TypeMismatch> {
    let mut mismatches = Vec::new();
    if is_structured_table_target(semantic_model, target) {
        collect_table_field_mismatches(&mut mismatches, semantic_model, target, table);
    }
    mismatches
}

fn collect_table_field_mismatches(
    mismatches: &mut Vec<TypeMismatch>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    table: &LuaTableExpr,
) {
    // Union target: use the first structured component to check the table literal (`Callback | Callback[]` scenario).
    if let LuaType::Union(union) = target {
        for component in union.into_vec().iter() {
            if is_structured_table_target(semantic_model, component) {
                collect_table_field_mismatches(mismatches, semantic_model, component, table);
                return;
            }
        }
    }
    // Array target: check every table-literal element against the element type.
    if let LuaType::Array(array) = target {
        let expected = array.get_base();
        for (field, _) in table.get_fields_with_keys() {
            let Some(value_expr) = field.get_value_expr() else {
                continue;
            };
            collect_table_value_mismatches(mismatches, semantic_model, &value_expr, expected);
        }
        return;
    }
    // Projection form where the `array<T>` alias / `T[]` is not expanded into `Array`.
    if let LuaType::Generic(generic) = target
        && generic.get_base_type_id().get_name() == "array"
        && let Some(expected) = generic.get_params().first()
    {
        for (field, _) in table.get_fields_with_keys() {
            let Some(value_expr) = field.get_value_expr() else {
                continue;
            };
            collect_table_value_mismatches(mismatches, semantic_model, &value_expr, expected);
        }
        return;
    }
    // Object target: static fields first, then `{[string]: string}` index signatures.
    if let LuaType::Object(object) = target {
        for (field, key) in table.get_fields_with_keys() {
            let Some(value_expr) = field.get_value_expr() else {
                continue;
            };
            let member_key = index_key_to_member_key(semantic_model, &key);
            if let Some(expected) = member_key
                .as_ref()
                .and_then(|key| object.get_fields().get(key))
            {
                collect_table_value_mismatches(mismatches, semantic_model, &value_expr, expected);
                continue;
            }
            let key_ty = index_key_type(semantic_model, &key);
            if let Ok(infos) = semantic_model.applicable_index_infos(target, &key_ty)
                && let Some(info) = infos.first()
            {
                collect_table_value_mismatches(
                    mismatches,
                    semantic_model,
                    &value_expr,
                    &info.value_type,
                );
            }
        }
        return;
    }

    // Alias target: expand it, then check as object/array/class.
    if let LuaType::Ref(_) | LuaType::Def(_) = target
        && let Some(def) = named_def(semantic_model, target)
        && def.kind == TypeDefKind::Alias
        && let Some(alias_target) = semantic_model.alias_target(&def)
        && is_structured_table_target(semantic_model, &alias_target)
    {
        collect_table_field_mismatches(mismatches, semantic_model, &alias_target, table);
        return;
    }

    if named_def(semantic_model, target).is_none() {
        return;
    }
    let mut members = HashMap::new();
    let mut required_members: Vec<(String, LuaType)> = Vec::new();
    for info in semantic_model.member_infos(target) {
        let declaration = info.id.as_ref().and_then(|id| {
            semantic_model
                .file_facts_of(info.file_id?)
                .and_then(|facts| facts.member_by_id(id))
        });
        if declaration.is_some_and(|member| !matches!(member.owner, SemanticId::TypeDef(_))) {
            continue;
        }
        let required = declaration.is_some_and(|member| !member.is_nullable);
        if required {
            required_members.push((info.key.to_path(), info.typ.clone()));
        }
        members.insert(info.key, info.typ);
    }
    let fields: Vec<_> = table
        .get_fields_with_keys()
        .into_iter()
        .map(|(field, key)| {
            let member_key = match &key {
                LuaIndexKey::Expr(_) => None,
                _ => static_member_key(semantic_model, &key),
            };
            (field, key, member_key)
        })
        .collect();
    for (field, key, member_key) in &fields {
        if let Some(expected) = member_key.as_ref().and_then(|key| members.get(key)) {
            let Some(value_expr) = field.get_value_expr() else {
                continue;
            };
            collect_table_value_mismatches(mismatches, semantic_model, &value_expr, expected);
            continue;
        }
        // Static member not found -> check against index signatures (`@field [integer] string`).
        let Ok(infos) =
            semantic_model.applicable_index_infos(target, &index_key_type(semantic_model, key))
        else {
            continue;
        };
        let Some(info) = infos.first() else {
            continue;
        };
        let expected = &info.value_type;
        let Some(value_expr) = field.get_value_expr() else {
            continue;
        };
        collect_table_value_mismatches(mismatches, semantic_model, &value_expr, expected);
    }
    // Array-shaped table literal + integer index signature: required static members from class inheritance must still appear
    // (an Opts with `@field [integer] string` still requires the parent's `@field foo boolean`).
    if fields
        .iter()
        .any(|(_, key, _)| matches!(key, LuaIndexKey::Integer(_) | LuaIndexKey::Idx(_)))
    {
        let field_names: HashSet<String> = fields
            .iter()
            .map(|(_, key, member_key)| {
                member_key
                    .as_ref()
                    .map(|key| key.to_path())
                    .unwrap_or_else(|| key.get_path_part())
            })
            .collect();
        for (name, required_ty) in &required_members {
            if !field_names.contains(name.as_str()) {
                if let Some((first_field, _, _)) = fields.first()
                    && let Some(value_expr) = first_field.get_value_expr()
                {
                    let value_ty = semantic_model.type_of_expr_at(
                        value_expr.get_syntax_id(),
                        value_expr.get_range().start(),
                    );
                    mismatches.push(type_mismatch(
                        semantic_model,
                        value_expr.get_range(),
                        required_ty,
                        &value_ty,
                    ));
                }
                break;
            }
        }
    }
}

fn collect_table_value_mismatches(
    mismatches: &mut Vec<TypeMismatch>,
    semantic_model: &SemanticModel<'_>,
    value_expr: &LuaExpr,
    expected: &LuaType,
) {
    if matches!(value_expr, LuaExpr::TableExpr(_)) {
        collect_value_mismatches(mismatches, semantic_model, expected, value_expr);
        return;
    }
    let value_ty =
        semantic_model.type_of_expr_at(value_expr.get_syntax_id(), value_expr.get_range().start());
    // Nil omits an entry from a table constructor.
    let expected_or_absent = LuaType::from_vec(vec![expected.clone(), LuaType::Nil]);
    if matches!(value_ty, LuaType::Unknown | LuaType::Any | LuaType::Never)
        || is_compatible(semantic_model, &value_ty, &expected_or_absent)
    {
        return;
    }
    mismatches.push(type_mismatch(
        semantic_model,
        value_expr.get_range(),
        expected,
        &value_ty,
    ));
}

pub(super) fn index_key_type(semantic_model: &SemanticModel<'_>, key: &LuaIndexKey) -> LuaType {
    if let Some(key) = static_member_key(semantic_model, key).and_then(|key| key.to_index_type()) {
        return key;
    }
    match key {
        LuaIndexKey::Name(_) | LuaIndexKey::String(_) => LuaType::String,
        LuaIndexKey::Integer(token) => match token.get_number_value() {
            NumberResult::Float(value) => LuaType::FloatConst(value),
            NumberResult::Int(value) => LuaType::IntegerConst(value),
            NumberResult::Uint(value) => LuaType::FloatConst(value as f64),
            NumberResult::Number => LuaType::Number,
        },
        LuaIndexKey::Idx(value) => LuaType::IntegerConst(*value as i64),
        LuaIndexKey::Expr(expr) => semantic_model.type_of_expr(expr.get_syntax_id()),
    }
}

fn index_key_to_member_key(
    _semantic_model: &SemanticModel<'_>,
    key: &LuaIndexKey,
) -> Option<LuaMemberKey> {
    match key {
        LuaIndexKey::Name(_) | LuaIndexKey::String(_) => {
            Some(LuaMemberKey::Name(key.get_path_part().into()))
        }
        LuaIndexKey::Integer(_) | LuaIndexKey::Idx(_) => {
            let value = match key {
                LuaIndexKey::Integer(token) => Some(token.get_number_value()),
                LuaIndexKey::Idx(_) => key
                    .get_path_part()
                    .parse::<i64>()
                    .ok()
                    .map(NumberResult::Int),
                _ => None,
            };
            match value {
                Some(NumberResult::Int(i)) => Some(LuaMemberKey::Integer(i)),
                Some(NumberResult::Uint(u)) => Some(LuaMemberKey::Integer(u as i64)),
                _ => None,
            }
        }
        LuaIndexKey::Expr(_) => None,
    }
}

/// Local declaration attached to a `---@class B` comment -> class reference type.
fn class_contract_for_decl(semantic_model: &SemanticModel<'_>, decl: &Decl) -> Option<LuaType> {
    let def = facts_type_def_for_decl(semantic_model, decl)?;
    if def.kind == TypeDefKind::Enum {
        return None;
    }
    Some(semantic_model.type_def_ref(&def))
}

fn facts_type_def_for_decl(semantic_model: &SemanticModel<'_>, decl: &Decl) -> Option<TypeDef> {
    let facts = semantic_model.file_facts()?;
    facts
        .type_defs
        .iter()
        .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl.owner_syntax)
        .cloned()
}

fn named_def(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<TypeDef> {
    let id = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        LuaType::Generic(generic) => {
            return named_def(semantic_model, &LuaType::Ref(generic.get_base_type_id()));
        }
        _ => return None,
    };
    semantic_model.type_def_of(id)
}

/// The `---@type C` comment immediately before an assignment statement -> target type (`_ = c` scenario).
fn preceding_doc_type_target(
    semantic_model: &SemanticModel<'_>,
    stat: &LuaAssignStat,
) -> Option<LuaType> {
    let tree = semantic_model.syntax_tree()?;
    let mut before = String::new();
    for item in tree.get_red_root().descendants_with_tokens() {
        let Some(token) = item.into_token() else {
            continue;
        };
        if token.text_range().end() <= stat.get_range().start() {
            before.push_str(token.text());
        }
    }
    let mark = before.rfind("@type")?;
    let rest = &before[mark + 5..];
    let name: String = rest
        .trim()
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '_'))
        .collect();
    if name.is_empty() {
        return None;
    }
    Some(semantic_model.type_from_name(&name))
}

fn add_mismatch(
    context: &mut CheckContext<'_>,
    range: rowan::TextRange,
    target: &LuaType,
    value_ty: &LuaType,
) {
    let mismatch = type_mismatch(context.semantic_model, range, target, value_ty);
    context.add_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        mismatch.range,
        mismatch.message,
    );
}

fn type_mismatch(
    semantic_model: &SemanticModel<'_>,
    range: rowan::TextRange,
    target: &LuaType,
    value_ty: &LuaType,
) -> TypeMismatch {
    let reason = match check_type_detail(semantic_model, value_ty, target) {
        Err(TypeCheckFailReason::TypeNotMatchWithReason(reason)) => Some(reason),
        _ => None,
    };
    let message = match reason {
        Some(reason) => t!(
            "expected `%{target}` but found `%{value}`.\n  %{reason}",
            target = humanize_type(semantic_model, target),
            value = humanize_type(semantic_model, value_ty),
            reason = reason
        ),
        None => t!(
            "expected `%{target}` but found `%{value}`",
            target = humanize_type(semantic_model, target),
            value = humanize_type(semantic_model, value_ty)
        ),
    };
    TypeMismatch {
        range,
        message: message.into_owned(),
    }
}
