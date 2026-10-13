use std::rc::Rc;

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaCallExpr, LuaExpr, LuaLiteralToken, LuaLocalStat,
    LuaSyntaxId, LuaTableExpr, LuaVarExpr,
};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::table_constructor::table_expr;
use crate::semantic_model::type_check::{TypeCheckFailReason, check_type_at, is_compatible};
use crate::{LuaObjectType, LuaType, LuaTypeNode, SemanticId, TypeDefKind};

use super::check_field::{class_fields, table_literal_mismatches};
use super::{CheckContext, Checker};
use crate::semantic_model::render::humanize_type;

pub struct AssignTypeMismatchChecker;

impl Checker for AssignTypeMismatchChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::AssignTypeMismatch,
        DiagnosticCode::MissingFields,
        DiagnosticCode::InjectField,
    ];

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
            if let Some(target) = class_contract_for_owner(semantic_model, stat.get_syntax_id()) {
                check_class_initializer(context, semantic_model, &target, expr);
            }
            continue;
        }
        check_against(context, semantic_model, &decl_id, &expr);
    }
}

fn check_class_initializer(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    value_expr: &LuaExpr,
) {
    let mut expr = value_expr.clone();
    while let LuaExpr::ParenExpr(paren) = &expr {
        let Some(inner) = paren.get_expr() else {
            return;
        };
        expr = inner;
    }
    // A class table may inherit its members through a metatable.
    if let LuaExpr::CallExpr(call) = &expr
        && is_setmetatable_call(call)
    {
        return;
    }
    let value_ty = assignment_value_type(semantic_model, &expr)
        .get_result_slot_type(0)
        .unwrap_or(LuaType::Nil);
    let is_constructor = match &value_ty {
        LuaType::TableConst(_) => true,
        LuaType::Union(union) => union.iter().all(|ty| matches!(ty, LuaType::TableConst(_))),
        _ => false,
    };
    if !is_constructor {
        check_value_against(context, semantic_model, target, &expr);
        return;
    }
    // A class definition may fill its top-level fields later. Check supplied
    // values through the same contract regardless of where the table was created.
    let (fields, indexes) = class_fields(semantic_model, target);
    let fields = fields
        .into_iter()
        .map(|(key, ty)| (key, LuaType::from_vec(vec![ty, LuaType::Nil])))
        .collect();
    let initializer = LuaType::Object(Rc::new(LuaObjectType::new_with_fields(fields, indexes)));
    check_value_against(context, semantic_model, &initializer, &expr);
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
                if let Some(decl) = semantic_model
                    .file_facts()
                    .and_then(|facts| facts.decl_by_id(&decl_id))
                    && decl.doc_type_syntax.is_none()
                    && let Some(target) =
                        class_contract_for_owner(semantic_model, stat.get_syntax_id())
                {
                    check_class_initializer(context, semantic_model, &target, expr);
                    continue;
                }
                check_assign_against(
                    context,
                    semantic_model,
                    &decl_id,
                    name_expr.get_position(),
                    &expr,
                );
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                if let Some(targets) = semantic_model.index_assignment_targets(index_expr) {
                    for target in targets {
                        check_value_against_target(context, semantic_model, &target, expr);
                    }
                }
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

fn check_value_against(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    value_expr: &LuaExpr,
) {
    let field_errors = table_expr(value_expr).map_or_else(Vec::new, |table| {
        table_literal_mismatches(semantic_model, target, &table)
    });
    let has_field_error = field_errors
        .iter()
        .any(|error| context.is_diagnostic_enabled(error.code, error.range));
    for error in field_errors {
        context.add_diagnostic(error.code, error.range, error.message);
    }
    let value_ty = assignment_value_type(semantic_model, value_expr);
    let compatible = if matches!(value_ty, LuaType::TableConst(_)) {
        check_type_at(
            semantic_model,
            &value_ty,
            target,
            value_expr.get_range(),
            false,
        )
        .is_ok()
    } else {
        is_compatible(semantic_model, &value_ty, target)
    };
    if matches!(value_ty, LuaType::Unknown | LuaType::Any | LuaType::Never)
        || compatible
        || has_field_error
    {
        return;
    }
    add_mismatch(context, value_expr.get_range(), target, &value_ty);
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

/// Statement attached to a `---@class B` comment -> class reference type.
fn class_contract_for_owner(
    semantic_model: &SemanticModel<'_>,
    owner: LuaSyntaxId,
) -> Option<LuaType> {
    let facts = semantic_model.file_facts()?;
    let def = facts.type_def_by_owner_syntax(owner)?;
    if def.kind != TypeDefKind::Class {
        return None;
    }
    Some(semantic_model.type_def_ref(def))
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
    mut range: rowan::TextRange,
    target: &LuaType,
    value_ty: &LuaType,
) {
    let failure = check_type_at(context.semantic_model, value_ty, target, range, true).err();
    let (target, value_ty) = if let Some(TypeCheckFailReason::TypeNotMatchAtField(field)) = &failure
        && field.location.file_id == context.semantic_model.file_id()
        && range.contains_range(field.location.value)
    {
        range = field.location.value;
        (&field.target, &field.source)
    } else {
        (target, value_ty)
    };
    let reason = failure.as_ref().and_then(TypeCheckFailReason::reason);
    let message = match reason {
        Some(reason) => t!(
            "expected `%{target}` but found `%{value}`.\n  %{reason}",
            target = humanize_type(context.semantic_model, target),
            value = humanize_type(context.semantic_model, value_ty),
            reason = reason
        ),
        None => t!(
            "expected `%{target}` but found `%{value}`",
            target = humanize_type(context.semantic_model, target),
            value = humanize_type(context.semantic_model, value_ty)
        ),
    };
    context.add_diagnostic(DiagnosticCode::AssignTypeMismatch, range, message);
}
