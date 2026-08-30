//! # param_type_check - call argument type does not match parameter type
//!
//! M0+: callee candidates = primary signature + `---@overload`; if any candidate accepts all arguments, don't report,
//! otherwise pick the candidate with the fewest mismatches and report `ParamTypeMismatch` per argument. Colon calls:
//! strip one slot for the self parameter; plain functions treat the receiver as the first argument.

use std::sync::Arc;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaSyntaxId, LuaTokenKind,
};

use crate::semantic_model::SemanticModel;
use crate::semantic_model::infer::function_solver::functions_compatible;
use crate::semantic_model::infer::unify;
use crate::semantic_model::infer::vm::unify_call_bindings;
use crate::semantic_model::render::humanize_type;
use crate::semantic_model::type_check::is_compatible;
use crate::{DiagnosticCode, LuaTupleStatus, LuaTupleType, LuaType, LuaTypeNode};

use super::param_count::{callable_functions, first_param_is_self};
use super::{CheckContext, Checker};

pub struct ParamTypeChecker;

impl Checker for ParamTypeChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::ParamTypeMismatch];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for node in root.descendants().filter_map(LuaAst::cast) {
            let LuaAst::LuaCallExpr(call_expr) = node else {
                continue;
            };
            check_call(context, semantic_model, &call_expr);
        }
    }
}

struct Mismatch {
    range: rowan::TextRange,
    message: String,
}

/// pcall forwarding check: take the first argument's function signature and check the remaining `pcall` arguments against its parameters.
/// Returns true if handled (whether or not there are mismatches); returns false if the callback signature cannot be resolved.
fn check_pcall_forward(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    args: &[LuaExpr],
) -> bool {
    let Some(callback_expr) = args.first() else {
        return false;
    };
    let callback_candidates = callable_candidates(semantic_model, callback_expr);
    if callback_candidates.is_empty() {
        return false;
    }
    let forwarded = &args[1..];
    let mut best: Option<Vec<Mismatch>> = None;
    for candidate in &callback_candidates {
        let mismatches = check_candidate(
            semantic_model,
            candidate,
            forwarded,
            false,
            &LuaType::Unknown,
            &[],
        );
        if mismatches.is_empty() {
            return true;
        }
        if best
            .as_ref()
            .is_none_or(|current| mismatches.len() < current.len())
        {
            best = Some(mismatches);
        }
    }
    if let Some(mismatches) = best {
        for mismatch in mismatches {
            context.add_diagnostic(
                DiagnosticCode::ParamTypeMismatch,
                mismatch.range,
                mismatch.message,
            );
        }
    }
    true
}

/// Callee expression -> candidate function signatures (DocFunction / member overloads / cross-file global signatures).
pub(crate) fn callable_candidates(
    semantic_model: &SemanticModel<'_>,
    callee: &LuaExpr,
) -> Vec<crate::LuaFunctionType> {
    let callee_ty = semantic_model.type_of_expr(callee.get_syntax_id());
    let mut candidates = callable_functions(semantic_model, &callee_ty);
    if candidates.is_empty()
        && let LuaExpr::IndexExpr(index_expr) = callee
        && let Some(resolved) = semantic_model.resolve_member(index_expr)
    {
        if resolved.member_id.is_none()
            && let Some(prefix) = index_expr.get_prefix_expr()
        {
            let prefix_ty = semantic_model.type_of_expr(prefix.get_syntax_id());
            let key = crate::LuaMemberKey::Name(resolved.name.to_string().into());
            let member_ty = semantic_model.member_type(&prefix_ty, &key);
            if let Some(ty) = member_ty {
                candidates.extend(callable_functions(semantic_model, &ty));
            }
            // Runtime function members (`string.rep`): find the member closure signature in the file of the prefix table identity.
            if candidates.is_empty()
                && let LuaType::TableConst(table) = &prefix_ty
                && let Some(facts) = semantic_model.file_facts_of(table.file_id)
            {
                for member in facts
                    .members
                    .iter()
                    .filter(|member| member.key.name() == Some(resolved.name.as_str()))
                {
                    let Some(value_syntax) = member.value_syntax else {
                        continue;
                    };
                    if let Some(func) =
                        semantic_model.type_of_signature_in_file(facts.file_id, value_syntax)
                    {
                        candidates.push(func);
                        break;
                    }
                }
            }
        }
        if let Some(member_id) = resolved.member_id {
            if let Some(member_file) = resolved.file_id
                && let Some(facts) = semantic_model.file_facts_of(member_file)
            {
                let mut overloads = Vec::new();
                for overload in facts
                    .members
                    .iter()
                    .filter(|member| member.key.name() == Some(resolved.name.as_str()))
                {
                    if let Some(ty) = semantic_model.type_of_member(&overload.id) {
                        overloads.extend(callable_functions(semantic_model, &ty));
                    }
                }
                if !overloads.is_empty() {
                    candidates = overloads;
                }
            }
            if candidates.is_empty()
                && let Some(member_ty) = semantic_model.type_of_member(&member_id)
            {
                candidates = callable_functions(semantic_model, &member_ty);
            }
            // Runtime method members (`self.name`) are often projected to a broad `Function`: fill in the member closure signature,
            // so `pcall(obj.method, obj, ...)` can get the real function signature.
            if candidates.is_empty()
                && let Some(member_file) = resolved.file_id
                && let Some(facts) = semantic_model.file_facts_of(member_file)
                && let Some(member) = facts.member_by_id(&member_id)
                && let Some(value_syntax) = member.value_syntax
                && let Some(func) =
                    semantic_model.type_of_signature_in_file(member_file, value_syntax)
            {
                candidates.push(func);
            }
        }
        // Even if member_id was resolved, if no signature was obtained above,
        // fall back to the runtime member closure signature in the TableConst's file (`string.rep` / `math.randomseed`).
        if candidates.is_empty()
            && let Some(prefix) = index_expr.get_prefix_expr()
            && let LuaType::TableConst(table) = &semantic_model.type_of_expr(prefix.get_syntax_id())
            && let Some(facts) = semantic_model.file_facts_of(table.file_id)
        {
            for member in facts
                .members
                .iter()
                .filter(|member| member.key.name() == Some(resolved.name.as_str()))
            {
                let Some(value_syntax) = member.value_syntax else {
                    continue;
                };
                if let Some(func) =
                    semantic_model.type_of_signature_in_file(facts.file_id, value_syntax)
                {
                    candidates.push(func);
                    break;
                }
            }
        }
    }
    if candidates.is_empty()
        && let LuaExpr::NameExpr(name_expr) = callee
        && let Some(decl) = semantic_model.resolve_name(name_expr.get_position())
        && let Some(func) = semantic_model.type_of_decl_signature(&decl)
    {
        candidates.push(func);
    }
    candidates
}

fn check_call(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    let Some(callee) = call_expr.get_prefix_expr() else {
        return;
    };
    let callee_ty = semantic_model.type_of_expr(callee.get_syntax_id());
    let mut candidates = callable_functions(semantic_model, &callee_ty);
    // `A.foo`: VM only gives Function; use the declared member type from member resolution.
    if candidates.is_empty()
        && let Some(index_expr) = LuaIndexExpr::cast(callee.syntax().clone())
        && let Some(resolved) = semantic_model.resolve_member(&index_expr)
    {
        if let Some(member_id) = resolved.member_id {
            // Same-name `@field` overloads: all same-name members in the file (different owners) are candidate signatures.
            if let Some(member_file) = resolved.file_id
                && let Some(facts) = semantic_model.file_facts_of(member_file)
            {
                let mut overloads = Vec::new();
                for overload in facts
                    .members
                    .iter()
                    .filter(|member| member.key.name() == Some(resolved.name.as_str()))
                {
                    if let Some(ty) = semantic_model.type_of_member(&overload.id) {
                        overloads.extend(callable_functions(semantic_model, &ty));
                    }
                }
                if !overloads.is_empty() {
                    candidates = overloads;
                }
            }
            if candidates.is_empty()
                && let Some(member_ty) = semantic_model.type_of_member(&member_id)
            {
                candidates = callable_functions(semantic_model, &member_ty);
            }
            if candidates.is_empty()
                && let Some(member_file) = resolved.file_id
                && let Some(facts) = semantic_model.file_facts_of(member_file)
                && let Some(member) = facts.member_by_id(&member_id)
                && let Some(value_syntax) = member.value_syntax
                && let Some(tree) = semantic_model.syntax_tree()
                && let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root())
                && let Some(closure) = emmylua_parser::LuaClosureExpr::cast(node)
                && let Some(func) = semantic_model.type_of_signature(closure.get_syntax_id())
            {
                candidates.push(func);
            }
        }
    }
    // Cross-file global functions: VM only gives Function; take the signature from the name declaration file.
    if candidates.is_empty()
        && let LuaExpr::NameExpr(name_expr) = &callee
        && let Some(decl) = semantic_model.resolve_name(name_expr.get_position())
    {
        if let Some(func) = semantic_model.type_of_decl_signature(&decl) {
            candidates.push(func);
        }
    }
    // Local `---@type F1` (fun alias) declaration structure takes precedence over closure body inference.
    if let LuaExpr::NameExpr(name_expr) = &callee
        && let Some(decl) = semantic_model.resolve_name(name_expr.get_position())
    {
        if let Some(facts) = semantic_model.file_facts()
            && let Some(decl_facts) = facts.decl_by_id(&decl)
            && let Some(doc_syntax) = decl_facts.doc_type_syntax
        {
            let doc_ty = semantic_model.doc_type_lua(doc_syntax);
            if let Some(func) = doc_function_of(semantic_model, &doc_ty) {
                candidates = vec![func];
            }
        }
    }
    let args = call_expr
        .get_args_list()
        .map(|list| list.get_args().collect::<Vec<_>>())
        .unwrap_or_default();
    let explicit_generics: Vec<LuaSyntaxId> = call_expr
        .get_call_generic_type_list()
        .map(|list| list.get_types().map(|ty| ty.get_syntax_id()).collect())
        .unwrap_or_default();
    // `pcall(callback, ...)`: forward the remaining arguments to the callback check.
    if let LuaExpr::NameExpr(callee_name) = &callee
        && callee_name.get_name_text().as_deref() == Some("pcall")
        && !args.is_empty()
        && check_pcall_forward(context, semantic_model, &args)
    {
        return;
    }
    if candidates.is_empty() {
        return;
    }
    let colon_call = call_expr.is_colon_call();
    let receiver_ty = if colon_call {
        LuaIndexExpr::cast(callee.syntax().clone())
            .and_then(|index| index.get_prefix_expr())
            .map(|prefix| semantic_model.type_of_expr(prefix.get_syntax_id()))
            .unwrap_or(LuaType::Unknown)
    } else {
        LuaType::Unknown
    };

    let mut best: Option<Vec<Mismatch>> = None;
    for candidate in &candidates {
        let mismatches = check_candidate(
            semantic_model,
            candidate,
            &args,
            colon_call,
            &receiver_ty,
            &explicit_generics,
        );
        if mismatches.is_empty() {
            return;
        }
        if best
            .as_ref()
            .is_none_or(|current| mismatches.len() < current.len())
        {
            best = Some(mismatches);
        }
    }
    if let Some(mismatches) = best {
        let literal_names = doc_overload_literal_names(semantic_model, &callee);
        for mismatch in mismatches {
            let message = if literal_names.len() >= 2 {
                let found = mismatch
                    .message
                    .split_once("but found")
                    .map(|(_, found)| format!("but found{found}"))
                    .unwrap_or_else(|| mismatch.message.clone());
                format!("expected `{}` {found}", literal_names.join(" | "))
            } else {
                mismatch.message
            };
            context.add_diagnostic(DiagnosticCode::ParamTypeMismatch, mismatch.range, message);
        }
    }
}

/// String literal overload names for a local `---@type fun(name: "A") | fun(name: "B")`.
fn doc_overload_literal_names(semantic_model: &SemanticModel<'_>, callee: &LuaExpr) -> Vec<String> {
    let LuaExpr::NameExpr(name_expr) = callee else {
        return Vec::new();
    };
    let Some(decl) = semantic_model.resolve_name(name_expr.get_position()) else {
        return Vec::new();
    };
    let Some(facts) = semantic_model.file_facts() else {
        return Vec::new();
    };
    let Some(decl_facts) = facts.decl_by_id(&decl) else {
        return Vec::new();
    };
    let Some(doc_syntax) = decl_facts.doc_type_syntax else {
        return Vec::new();
    };
    let Some(tree) = semantic_model.syntax_tree() else {
        return Vec::new();
    };
    let Some(node) = doc_syntax.to_node_from_root(&tree.get_red_root()) else {
        return Vec::new();
    };
    node.descendants_with_tokens()
        .filter_map(|item| item.into_token())
        .filter(|token| token.kind() == LuaTokenKind::TkString.into())
        .map(|token| token.text().trim_matches(['"', '\'']).to_string())
        .collect()
}

/// When object / intersection / union contains an object, check table literals field by field.
fn has_table_literal_shape(ty: &LuaType) -> bool {
    match ty {
        LuaType::Object(_) => true,
        LuaType::Intersection(intersection) => {
            intersection.get_types().iter().any(has_table_literal_shape)
        }
        LuaType::Union(union) => union.into_vec().iter().any(has_table_literal_shape),
        _ => false,
    }
}

fn table_literal_mismatch(
    semantic_model: &SemanticModel<'_>,
    param_ty: &LuaType,
    table: &emmylua_parser::LuaTableExpr,
) -> Option<Mismatch> {
    match param_ty {
        LuaType::Object(object) => {
            let fields = object.get_fields();
            for (field, key) in table.get_fields_with_keys() {
                let path = key.get_path_part();
                let Some(expected) = fields.get(&crate::LuaMemberKey::Name(path.clone().into()))
                else {
                    continue;
                };
                let Some(value_expr) = field.get_value_expr() else {
                    continue;
                };
                let value_ty = semantic_model.type_of_expr(value_expr.get_syntax_id());
                if value_ty == *expected || is_compatible(semantic_model, &value_ty, expected) {
                    continue;
                }
                return Some(Mismatch {
                    range: value_expr.get_range(),
                    message: mismatch_message(semantic_model, &value_ty, expected, &path),
                });
            }
            None
        }
        LuaType::Intersection(intersection) => intersection
            .get_types()
            .iter()
            .find_map(|component| table_literal_mismatch(semantic_model, component, table)),
        LuaType::Union(union) => {
            let components = union.into_vec();
            let mut last: Option<Mismatch> = None;
            for component in components.iter() {
                {
                    let mismatch = table_literal_mismatch(semantic_model, component, table)?;
                    last = Some(mismatch);
                }
            }
            last
        }
        _ => None,
    }
}

fn check_candidate(
    semantic_model: &SemanticModel<'_>,
    func: &crate::LuaFunctionType,
    args: &[LuaExpr],
    colon_call: bool,
    receiver_ty: &LuaType,
    explicit_generics: &[LuaSyntaxId],
) -> Vec<Mismatch> {
    let params = func.get_params();
    let self_param = first_param_is_self(func);
    let param_start = usize::from(colon_call && self_param);
    // Method definition called with dot syntax (`obj.method(obj, ...)` / `pcall(obj.method, obj, ...)`):
    // the salsa method signature contains only user parameters; the first actual argument is an explicit receiver and must be skipped.
    let dot_method_receiver = !colon_call && !self_param && func.is_colon_define();
    let arg_offset = usize::from(dot_method_receiver);

    // Per-candidate generic binding table: bindings inferred from earlier arguments (T[]) are used for later callback parameters.
    let mut bindings = unify::TplBindings::new();
    // Explicit call generics `test--[[@<number | string>]]()` fill bindings first instead of inferring from arguments.
    for (index, syntax) in explicit_generics.iter().enumerate() {
        if let Some(tpl) = func.get_generic_params().get(index) {
            let ty = semantic_model.doc_type_lua_in(semantic_model.file_id(), *syntax, &[]);
            bindings.insert(tpl.get_tpl_id(), ty);
        }
    }

    // Plain function called with colon syntax: the receiver occupies the first argument slot.
    if colon_call && !self_param {
        let mut mismatches = Vec::new();
        if let Some((name, Some(param_ty))) = params.first() {
            let param_ty = effective_param(param_ty, receiver_ty);
            let _ = unify_call_bindings(semantic_model, &param_ty, receiver_ty, &mut bindings);
            if !is_compatible(semantic_model, receiver_ty, &param_ty) {
                mismatches.push(Mismatch {
                    range: receiver_range(args),
                    message: mismatch_message(semantic_model, receiver_ty, &param_ty, name),
                });
            }
        }
        mismatches.extend(check_arg_pairs(
            semantic_model,
            &params[1..],
            args,
            func.is_variadic(),
            &mut bindings,
        ));
        return mismatches;
    }

    check_arg_pairs(
        semantic_model,
        &params[param_start..],
        &args[arg_offset..],
        func.is_variadic(),
        &mut bindings,
    )
}

fn param_generic_base_match(
    semantic_model: &SemanticModel<'_>,
    arg_ty: &LuaType,
    param_ty: &LuaType,
) -> bool {
    let param_base = match param_ty {
        LuaType::Generic(generic) => Some(generic.get_base_type_id()),
        _ => None,
    };
    let arg_base = match arg_ty {
        LuaType::Ref(id) | LuaType::Def(id) => Some(id.clone()),
        LuaType::Generic(generic) => Some(generic.get_base_type_id()),
        _ => None,
    };
    match (param_base, arg_base) {
        (Some(param_base), Some(arg_base)) if param_base != arg_base => {
            semantic_model.type_check_subtype(arg_ty, param_ty)
        }
        _ => is_compatible(semantic_model, arg_ty, param_ty),
    }
}

fn check_arg_pairs(
    semantic_model: &SemanticModel<'_>,
    params: &[(String, Option<LuaType>)],
    args: &[LuaExpr],
    is_variadic: bool,
    bindings: &mut unify::TplBindings,
) -> Vec<Mismatch> {
    let mut out = Vec::new();
    for (index, (name, param_ty)) in params.iter().enumerate() {
        let is_vararg_slot = is_variadic && index + 1 == params.len();
        if is_vararg_slot {
            if let Some(param_ty) = param_ty {
                let param_ty = unify::substitute(param_ty, bindings);
                let param_ty = crate::semantic_model::type_eval::expand_alias_generic(
                    semantic_model,
                    &param_ty,
                );
                let param_ty =
                    crate::semantic_model::type_eval::eval_conditionals(semantic_model, &param_ty);
                if let LuaType::Variadic(variadic) = &param_ty {
                    // `@param ... T...`: T is bound to a tuple of the whole argument sequence, not requiring the same type for each argument.
                    if let crate::VariadicType::Base(base) = variadic.as_ref()
                        && let LuaType::TplRef(tpl) = base
                        && !bindings.contains_key(&tpl.get_tpl_id())
                    {
                        let rest_types: Vec<LuaType> = args[index..]
                            .iter()
                            .map(|arg| {
                                let ty = semantic_model.type_of_expr(arg.get_syntax_id());
                                normalize_arg_for_check(semantic_model, &ty)
                            })
                            .collect();
                        let tuple_ty = LuaType::Tuple(Arc::new(LuaTupleType::new(
                            rest_types,
                            LuaTupleStatus::InferResolve,
                        )));
                        let _ = unify_call_bindings(
                            semantic_model,
                            &LuaType::TplRef(tpl.clone()),
                            &tuple_ty,
                            bindings,
                        );
                        break;
                    }
                    for (variadic_index, arg) in args[index..].iter().enumerate() {
                        let Some(slot_ty) = variadic.get_type(variadic_index) else {
                            break;
                        };
                        let arg_ty = semantic_model.type_of_expr(arg.get_syntax_id());
                        let arg_ty = normalize_arg_for_check(semantic_model, &arg_ty);
                        let mut slot_ty = slot_ty.clone();
                        slot_ty = unify::substitute(&slot_ty, bindings);
                        // The same function generic in variadic arguments must be bound consistently across all arguments;
                        // explicit generics (e.g. `T = number | string`) are already substituted and no longer TplRef, so go straight to compatibility checks.
                        if slot_ty.contains_tpl_node()
                            && !unify_call_bindings(semantic_model, &slot_ty, &arg_ty, bindings)
                        {
                            out.push(Mismatch {
                                range: arg.get_range(),
                                message: mismatch_message(semantic_model, &arg_ty, &slot_ty, name),
                            });
                            continue;
                        }
                        slot_ty = unify::substitute(&slot_ty, bindings);
                        slot_ty = crate::semantic_model::type_eval::expand_alias_generic(
                            semantic_model,
                            &slot_ty,
                        );
                        if arg_ty == slot_ty
                            || is_compatible(semantic_model, &arg_ty, &slot_ty)
                            || crate::semantic_model::type_check::is_assign_compatible(
                                semantic_model,
                                &arg_ty,
                                &slot_ty,
                            )
                        {
                            continue;
                        }
                        out.push(Mismatch {
                            range: arg.get_range(),
                            message: mismatch_message(semantic_model, &arg_ty, &slot_ty, name),
                        });
                    }
                } else {
                    for arg in &args[index..] {
                        let arg_ty = semantic_model.type_of_expr(arg.get_syntax_id());
                        let arg_ty = normalize_arg_for_check(semantic_model, &arg_ty);
                        let param_ty_orig = param_ty.clone();
                        let unified =
                            unify_call_bindings(semantic_model, &param_ty_orig, &arg_ty, bindings);
                        if param_ty_orig.contains_tpl_node() && !unified {
                            out.push(Mismatch {
                                range: arg.get_range(),
                                message: mismatch_message(
                                    semantic_model,
                                    &arg_ty,
                                    &param_ty_orig,
                                    name,
                                ),
                            });
                            continue;
                        }
                        let param_ty = unify::substitute(&param_ty_orig, bindings);
                        if arg_ty == param_ty
                            || param_generic_base_match(semantic_model, &arg_ty, &param_ty)
                            || crate::semantic_model::type_check::is_assign_compatible(
                                semantic_model,
                                &arg_ty,
                                &param_ty,
                            )
                        {
                            continue;
                        }
                        out.push(Mismatch {
                            range: arg.get_range(),
                            message: mismatch_message(semantic_model, &arg_ty, &param_ty, name),
                        });
                    }
                }
            }
            break;
        }
        let Some(param_ty) = param_ty else {
            continue;
        };
        let Some(arg) = args.get(index) else {
            continue;
        };
        let arg_ty = semantic_model.type_of_expr(arg.get_syntax_id());
        let arg_ty = normalize_arg_for_check(semantic_model, &arg_ty);
        let call_arg_ty = call_argument_type(semantic_model, arg, &arg_ty);
        let call_arg_ty = normalize_arg_for_check(semantic_model, &call_arg_ty);
        let param_ty = unify::substitute(param_ty, bindings);
        let param_ty =
            crate::semantic_model::type_eval::expand_alias_generic(semantic_model, &param_ty);
        let param_ty =
            crate::semantic_model::type_eval::eval_conditionals(semantic_model, &param_ty);
        // Missing union member: when only A in `A|C` has `handle`, accessing `target.handle`
        // should report ParamTypeMismatch even if the type face happens to be string (missing members count as nil).
        if let LuaExpr::IndexExpr(index_expr) = arg
            && semantic_model.member_missing_in_union(index_expr)
        {
            out.push(Mismatch {
                range: arg.get_range(),
                message: mismatch_message(semantic_model, &LuaType::Nil, &param_ty, name),
            });
            continue;
        }
        // Table literals targeting object / intersection: check field by field and then fully take over.
        if let LuaExpr::TableExpr(table) = arg {
            if has_table_literal_shape(&param_ty) {
                if let Some(mismatch) = table_literal_mismatch(semantic_model, &param_ty, table) {
                    out.push(mismatch);
                }
                continue;
            }
            // Named generic class `Params<T>`: table literals missing required `@field`s are also incompatible.
            if let Some(mismatch) =
                generic_table_required_mismatch(semantic_model, &param_ty, table)
            {
                out.push(mismatch);
                continue;
            }
        }
        if skip_arg(&call_arg_ty, &param_ty) {
            continue;
        }
        // `---@param x string|true`: non-function arguments pass when they match any union component.
        // Unions containing generic components (`` `T`|T ``) cannot be swallowed early by is_compatible, otherwise T won't bind.
        let has_generic_union_tpl = matches!(
            &param_ty,
            LuaType::Union(union)
                if union
                    .into_vec()
                    .iter()
                    .any(|ty| matches!(ty, LuaType::TplRef(_) | LuaType::StrTplRef(_)))
        );
        if !has_generic_union_tpl
            && !matches!(
                call_arg_ty,
                LuaType::DocFunction(_) | LuaType::Signature(_) | LuaType::Function
            )
            && let LuaType::Union(union) = &param_ty
        {
            let components = union
                .into_vec()
                .into_iter()
                .filter(|ty| !matches!(ty, LuaType::Nil))
                .collect::<Vec<_>>();
            if components
                .iter()
                .any(|component| union_component_accepts(semantic_model, &call_arg_ty, component))
            {
                continue;
            }
        }
        let param_ty = effective_param(&param_ty, &call_arg_ty);
        // Unify first: T[] <- (A|B|C|D)[] and similar.
        let _ = unify_call_bindings(semantic_model, &param_ty, &call_arg_ty, bindings);
        let param_ty = unify::substitute(&param_ty, bindings);
        // After substituting generic unions (`T|StrTpl` -> `Table|...`), arguments pass if they match any component.
        if let LuaType::Union(union) = &param_ty {
            if union
                .into_vec()
                .iter()
                .any(|component| union_component_accepts(semantic_model, &call_arg_ty, component))
            {
                continue;
            }
        }
        // Object index signature vs V[]: bind the object's integer index values to the array element generic.
        if object_array_compatible(semantic_model, &call_arg_ty, &param_ty, bindings) {
            continue;
        }
        // Higher-order functions go through the iterative solver first; broad `Function` arguments must not be swallowed by is_compatible.
        if function_arg_compatible(semantic_model, &call_arg_ty, &param_ty, bindings) {
            continue;
        }
        if call_arg_ty == param_ty {
            continue;
        }
        let exact_const = match (&call_arg_ty, &param_ty) {
            (LuaType::StringConst(a), LuaType::StringConst(b)) => a == b,
            (
                LuaType::IntegerConst(a) | LuaType::DocIntegerConst(a),
                LuaType::IntegerConst(b) | LuaType::DocIntegerConst(b),
            ) => a == b,
            _ => true,
        };
        let leaf_compat = exact_const
            && !matches!(call_arg_ty, LuaType::Function)
            && param_generic_base_match(semantic_model, &call_arg_ty, &param_ty);
        if leaf_compat
            || literal_compatible(semantic_model, &call_arg_ty, &param_ty)
            || named_kind_compatible(semantic_model, &call_arg_ty, &param_ty)
        {
            continue;
        }
        out.push(Mismatch {
            range: arg.get_range(),
            message: mismatch_message(semantic_model, &arg_ty, &param_ty, name),
        });
    }
    out
}

fn generic_table_required_mismatch(
    semantic_model: &SemanticModel<'_>,
    param_ty: &LuaType,
    table: &emmylua_parser::LuaTableExpr,
) -> Option<Mismatch> {
    let id = match param_ty {
        LuaType::Generic(generic) => generic.get_base_type_id().clone(),
        LuaType::Ref(id) | LuaType::Def(id) => id.clone(),
        LuaType::Union(union) => {
            return union.into_vec().iter().find_map(|component| {
                generic_table_required_mismatch(semantic_model, component, table)
            });
        }
        LuaType::Intersection(intersection) => {
            return intersection.get_types().iter().find_map(|component| {
                generic_table_required_mismatch(semantic_model, component, table)
            });
        }
        _ => return None,
    };
    let def = crate::semantic_model::member::type_def_of(semantic_model, &id)?;
    if def.kind != crate::TypeDefKind::Class {
        return None;
    }
    let provided: Vec<String> = table
        .get_fields_with_keys()
        .iter()
        .map(|(_, key)| key.get_path_part())
        .collect();
    for member_ref in semantic_model.members_of_owner(&def.id) {
        let Some(facts) = semantic_model.file_facts_of(member_ref.file_id) else {
            continue;
        };
        let Some(member) = facts.member_by_id(&member_ref.id) else {
            continue;
        };
        if member.is_nullable || member.is_index_signature {
            continue;
        }
        if !provided.contains(&member_ref.name.to_string()) {
            let expected = semantic_model
                .type_of_member(&member_ref.id)
                .unwrap_or(LuaType::Unknown);
            return Some(Mismatch {
                range: table.get_range(),
                message: mismatch_message(
                    semantic_model,
                    &semantic_model.type_of_expr(table.get_syntax_id()),
                    &expected,
                    &member_ref.name,
                ),
            });
        }
    }
    None
}

/// Object index signature vs `V[]`: an integer-key signature satisfies an array and binds the values to V.
fn object_array_compatible(
    semantic_model: &SemanticModel<'_>,
    arg_ty: &LuaType,
    param_ty: &LuaType,
    bindings: &mut unify::TplBindings,
) -> bool {
    let (LuaType::Object(object), LuaType::Array(array)) = (arg_ty, param_ty) else {
        return false;
    };
    for (key_ty, value_ty) in object.get_index_access() {
        let numeric = matches!(
            key_ty,
            LuaType::Integer | LuaType::Number | LuaType::IntegerConst(_)
        );
        if !numeric {
            continue;
        }
        if let LuaType::TplRef(tpl) = array.get_base() {
            match bindings.get(&tpl.get_tpl_id()) {
                Some(existing) => return existing == value_ty,
                None => {
                    bindings.insert(tpl.get_tpl_id(), value_ty.clone());
                    return true;
                }
            }
        }
        return is_compatible(semantic_model, value_ty, array.get_base());
    }
    false
}

/// For name arguments where VM only gives `Function`: resolve cross-file signatures for structural checks by the higher-order function solver.
fn call_argument_type(
    semantic_model: &SemanticModel<'_>,
    arg: &LuaExpr,
    arg_ty: &LuaType,
) -> LuaType {
    let constrained =
        generic_constraint_type(semantic_model, arg_ty).unwrap_or_else(|| arg_ty.clone());
    if let LuaExpr::ClosureExpr(closure) = arg {
        if let Some(func) = semantic_model.type_of_signature(closure.get_syntax_id()) {
            return LuaType::DocFunction(Arc::new(func));
        }
    }
    if matches!(constrained, LuaType::Function)
        && let LuaExpr::NameExpr(name_expr) = arg
        && let Some(decl) = semantic_model.resolve_name(name_expr.get_position())
    {
        if let Some(func) = semantic_model.type_of_decl_signature(&decl) {
            return LuaType::DocFunction(Arc::new(func));
        }
        if let Some(facts) = semantic_model.file_facts()
            && let Some(decl) = facts.decl_by_id(&decl)
            && let Some(doc_syntax) = decl.doc_type_syntax
        {
            let ty = semantic_model.doc_type_lua_rich(doc_syntax);
            if let LuaType::DocFunction(func) = ty {
                return LuaType::DocFunction(func);
            }
            if let LuaType::Union(union) = &ty {
                for component in union.into_vec() {
                    if let LuaType::DocFunction(func) = component {
                        return LuaType::DocFunction(func);
                    }
                }
            }
        }
    }
    constrained
}

/// `Ref("T")` with a signature declaring `---@generic T: Animal` -> project to the constraint Animal.
fn generic_constraint_type(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<LuaType> {
    let (LuaType::Ref(id) | LuaType::Def(id)) = ty else {
        return None;
    };
    if crate::semantic_model::member::type_def_of(semantic_model, id).is_some() {
        return None;
    }
    let name = id.get_name();
    let signatures = semantic_model.signatures()?;
    for signature in signatures {
        let docs = signature.docs.as_ref()?;
        if let Some(param) = docs
            .generic_params
            .iter()
            .find(|param| param.name.as_str() == name)
            && let Some(constraint) = param.constraint
        {
            let constraint_ty = semantic_model.doc_type_lua(constraint);
            if !matches!(constraint_ty, LuaType::Unknown) {
                return Some(constraint_ty);
            }
        }
    }
    None
}

/// Basic value-domain classification: string / integer / number / boolean categories after enum/alias/class expansion.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Nil,
    Boolean,
    String,
    Integer,
    Number,
    Other,
}

fn value_kind(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> ValueKind {
    value_kind_inner(semantic_model, ty, &mut Vec::new())
}

fn value_kind_inner(
    semantic_model: &SemanticModel<'_>,
    ty: &LuaType,
    visited: &mut Vec<crate::LuaTypeDeclId>,
) -> ValueKind {
    match ty {
        LuaType::Nil => ValueKind::Nil,
        LuaType::Boolean | LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => {
            ValueKind::Boolean
        }
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => ValueKind::String,
        LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
            ValueKind::Integer
        }
        LuaType::Number | LuaType::FloatConst(_) => ValueKind::Number,
        LuaType::Union(union) => {
            let mut kind = None;
            for component in union.into_vec() {
                let component_kind = value_kind_inner(semantic_model, &component, visited);
                if kind.is_some_and(|kind| kind != component_kind) {
                    return ValueKind::Other;
                }
                kind = Some(component_kind);
            }
            kind.unwrap_or(ValueKind::Other)
        }
        LuaType::Ref(id) | LuaType::Def(id) => {
            if visited.contains(id) {
                return ValueKind::Other;
            }
            visited.push(id.clone());
            let Some(def) = crate::semantic_model::member::type_def_of(semantic_model, id) else {
                return ValueKind::Other;
            };
            match def.kind {
                crate::TypeDefKind::Alias => semantic_model
                    .alias_target(&def)
                    .map(|target| value_kind_inner(semantic_model, &target, visited))
                    .unwrap_or(ValueKind::Other),
                crate::TypeDefKind::Class => {
                    if def
                        .super_names
                        .iter()
                        .any(|name| matches!(name.as_str(), "integer" | "int"))
                    {
                        ValueKind::Integer
                    } else if def
                        .super_names
                        .iter()
                        .any(|name| matches!(name.as_str(), "number"))
                    {
                        ValueKind::Number
                    } else if def.super_names.iter().any(|name| name.as_str() == "string") {
                        ValueKind::String
                    } else {
                        ValueKind::Other
                    }
                }
                crate::TypeDefKind::Enum => enum_value_kind(semantic_model, &def),
            }
        }
        _ => ValueKind::Other,
    }
}

fn enum_value_kind(semantic_model: &SemanticModel<'_>, def: &crate::TypeDef) -> ValueKind {
    let mut kind: Option<ValueKind> = None;
    let Some(facts) = semantic_model.file_facts_of(def.file_id) else {
        return ValueKind::Other;
    };
    let Some(decl) = facts.decl_named(def.name.as_str()) else {
        return ValueKind::Other;
    };
    for member_ref in semantic_model.members_of_owner(&decl.id) {
        let Some(member_facts) = semantic_model.file_facts_of(member_ref.file_id) else {
            continue;
        };
        let Some(member) = member_facts.member_by_id(&member_ref.id) else {
            continue;
        };
        let Some(value_syntax) = member.value_syntax else {
            continue;
        };
        let Some(node) = semantic_model
            .syntax_tree()
            .and_then(|tree| value_syntax.to_node_from_root(&tree.get_red_root()))
        else {
            continue;
        };
        let raw = node.text().to_string();
        let item_kind = if raw.starts_with(['"', '\'']) {
            ValueKind::String
        } else if raw.parse::<i64>().is_ok() {
            ValueKind::Integer
        } else if raw.parse::<f64>().is_ok() {
            ValueKind::Number
        } else {
            ValueKind::Other
        };
        if let Some(kind) = kind
            && kind != item_kind
        {
            return ValueKind::Other;
        }
        kind = Some(item_kind);
    }
    kind.unwrap_or(ValueKind::Other)
}

fn named_kind_compatible(
    semantic_model: &SemanticModel<'_>,
    arg_ty: &LuaType,
    param_ty: &LuaType,
) -> bool {
    // Literal constants must match exactly (`"a"` != `"beforeAll"`).
    if let (LuaType::StringConst(a), LuaType::StringConst(b)) = (arg_ty, param_ty) {
        return a == b;
    }
    if let (
        LuaType::IntegerConst(a) | LuaType::DocIntegerConst(a),
        LuaType::IntegerConst(b) | LuaType::DocIntegerConst(b),
    ) = (arg_ty, param_ty)
    {
        return a == b;
    }
    let arg_kind = value_kind(semantic_model, arg_ty);
    let param_kind = value_kind(semantic_model, param_ty);
    if matches!(arg_kind, ValueKind::Other) || matches!(param_kind, ValueKind::Other) {
        return false;
    }
    arg_kind == param_kind
        || matches!(
            (arg_kind, param_kind),
            (ValueKind::Integer, ValueKind::Number) | (ValueKind::Number, ValueKind::Integer)
        )
}

/// Callback parameters vs function arguments: use the explicit work-queue function solver for structural/contravariant checks.
fn function_arg_compatible(
    semantic_model: &SemanticModel<'_>,
    arg_ty: &LuaType,
    param_ty: &LuaType,
    bindings: &unify::TplBindings,
) -> bool {
    let actual = doc_function_of(semantic_model, arg_ty);
    let expected = doc_function_of(semantic_model, param_ty);
    match (actual, expected) {
        (Some(actual), Some(expected)) => {
            functions_compatible(semantic_model, &actual, &expected, bindings).compatible
        }
        // One side cannot be projected to DocFunction: preserve legacy M0 behavior to avoid false positives for Function/Ref forms.
        _ => matches!(
            (arg_ty, param_ty),
            (
                LuaType::DocFunction(_) | LuaType::Signature(_),
                LuaType::DocFunction(_) | LuaType::Signature(_)
            )
        ),
    }
}

/// Signature / alias fun -> DocFunction (alias expands one layer; solver queue guards cycles).
fn doc_function_of(
    semantic_model: &SemanticModel<'_>,
    ty: &LuaType,
) -> Option<crate::LuaFunctionType> {
    match ty {
        LuaType::DocFunction(func) => Some(func.as_ref().clone()),
        LuaType::Signature(signature_id) => semantic_model.signature_lua_by_legacy_id(signature_id),
        LuaType::Ref(id) | LuaType::Def(id) => {
            let def = crate::semantic_model::member::type_def_of(semantic_model, id)?;
            if def.kind != crate::TypeDefKind::Alias {
                return None;
            }
            match semantic_model.alias_target(&def)? {
                LuaType::DocFunction(func) => Some(func.as_ref().clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether the argument constant text matches an enum / literal-alias value set.
fn literal_compatible(
    semantic_model: &SemanticModel<'_>,
    arg_ty: &LuaType,
    param_ty: &LuaType,
) -> bool {
    let arg_text = match arg_ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => Some(s.as_ref().to_string()),
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => Some(i.to_string()),
        LuaType::BooleanConst(b) | LuaType::DocBooleanConst(b) => Some(b.to_string()),
        _ => None,
    };
    let Some(arg_text) = arg_text else {
        return false;
    };
    literal_values(semantic_model, param_ty, &mut Vec::new())
        .is_some_and(|values| values.iter().any(|value| value == &arg_text))
}

/// Literal value text for enum / alias (recursive cycle guard).
fn literal_values(
    semantic_model: &SemanticModel<'_>,
    ty: &LuaType,
    visited: &mut Vec<crate::LuaTypeDeclId>,
) -> Option<Vec<String>> {
    match ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => Some(vec![s.as_ref().to_string()]),
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => Some(vec![i.to_string()]),
        LuaType::BooleanConst(b) | LuaType::DocBooleanConst(b) => Some(vec![b.to_string()]),
        LuaType::Union(union) => {
            let mut values = Vec::new();
            for component in union.into_vec() {
                if let Some(mut component_values) =
                    literal_values(semantic_model, &component, visited)
                {
                    values.append(&mut component_values);
                }
            }
            (!values.is_empty()).then_some(values)
        }
        LuaType::Ref(id) | LuaType::Def(id) => {
            if visited.contains(id) {
                return None;
            }
            visited.push(id.clone());
            let def = crate::semantic_model::member::type_def_of(semantic_model, id)?;
            if def.kind == crate::TypeDefKind::Alias {
                return semantic_model
                    .alias_target(&def)
                    .and_then(|target| literal_values(semantic_model, &target, visited));
            }
            // enum: runtime table member names and values.
            if def.kind == crate::TypeDefKind::Enum {
                let facts = semantic_model.file_facts_of(def.file_id)?;
                let decl = facts.decl_named(def.name.as_str())?;
                let mut values = Vec::new();
                for member_ref in semantic_model.members_of_owner(&decl.id) {
                    values.push(member_ref.name.to_string());
                    if let Some(member_facts) = semantic_model.file_facts_of(member_ref.file_id)
                        && let Some(member) = member_facts.member_by_id(&member_ref.id)
                        && let Some(value_syntax) = member.value_syntax
                        && let Some(node) = semantic_model
                            .syntax_tree()
                            .and_then(|tree| value_syntax.to_node_from_root(&tree.get_red_root()))
                    {
                        let raw = node.text().to_string();
                        values.push(raw.trim_matches(['"', '\'']).to_string());
                    }
                }
                return (!values.is_empty()).then_some(values);
            }
            None
        }
        _ => None,
    }
}

/// Unknown/any/never arguments are not checked.
fn skip_arg(arg_ty: &LuaType, param_ty: &LuaType) -> bool {
    if matches!(arg_ty, LuaType::Unknown | LuaType::Any) {
        return true;
    }
    matches!(arg_ty, LuaType::Nil) && matches!(param_ty, LuaType::Any | LuaType::Unknown)
}

/// `param?` projects to T|nil: both nil and T are compatible; for union parameters choose a component based on argument shape
/// (`F|integer` + function argument -> take F).
/// Component acceptance for union parameters: constant components of the same kind with different values don't match (`keyof` member union semantics),
/// others fall back to structural compatibility (`string|true`'s boolean matches the boolean component).
fn union_component_accepts(
    semantic_model: &SemanticModel<'_>,
    arg: &LuaType,
    component: &LuaType,
) -> bool {
    if arg == component {
        return true;
    }
    match (arg, component) {
        (LuaType::StringConst(a), LuaType::StringConst(b))
        | (LuaType::DocStringConst(a), LuaType::DocStringConst(b))
            if a != b =>
        {
            false
        }
        (LuaType::IntegerConst(a), LuaType::IntegerConst(b))
        | (LuaType::DocIntegerConst(a), LuaType::DocIntegerConst(b))
            if a != b =>
        {
            false
        }
        (LuaType::BooleanConst(a), LuaType::BooleanConst(b))
        | (LuaType::DocBooleanConst(a), LuaType::DocBooleanConst(b))
            if a != b =>
        {
            false
        }
        _ => is_compatible(semantic_model, arg, component),
    }
}

fn effective_param(param_ty: &LuaType, arg_ty: &LuaType) -> LuaType {
    if let LuaType::Union(union) = param_ty {
        let components = union.into_vec();
        if components
            .iter()
            .any(|component| matches!(component, LuaType::TplRef(_) | LuaType::StrTplRef(_)))
        {
            // Generic unions (`` `T`|T ``) cannot collapse to the first component, or template literal branches are lost.
            return param_ty.clone();
        }
        if !matches!(arg_ty, LuaType::Nil) {
            let non_nil: Vec<&LuaType> = components
                .iter()
                .filter(|component| !matches!(component, LuaType::Nil))
                .collect();
            let function_like = matches!(
                arg_ty,
                LuaType::DocFunction(_) | LuaType::Signature(_) | LuaType::Function
            );
            if function_like
                && let Some(function_component) = non_nil.iter().find(|component| {
                    matches!(
                        component,
                        LuaType::DocFunction(_) | LuaType::Signature(_) | LuaType::TplRef(_)
                    )
                })
            {
                return (*function_component).clone();
            }
            if let Some(non_nil) = non_nil.first() {
                return (*non_nil).clone();
            }
        }
    }
    param_ty.clone()
}

fn receiver_range(args: &[LuaExpr]) -> rowan::TextRange {
    args.first().map(|arg| arg.get_range()).unwrap_or_default()
}

/// Argument type normalization for diagnostics: expand normal/conditional aliases, but keep mapped aliases as-is.
/// Mapped aliases like `Pick<T,K>` are handled nominally by `type_check::generic_type`;
/// expanding them to raw Mapped early would break table-structure checks.
fn normalize_arg_for_check(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> LuaType {
    let expanded = crate::semantic_model::type_eval::expand_alias_generic(semantic_model, ty);
    if contains_mapped(&expanded) {
        return ty.clone();
    }
    crate::semantic_model::type_eval::eval_conditionals(semantic_model, &expanded)
}

fn contains_mapped(ty: &LuaType) -> bool {
    use LuaType::*;
    match ty {
        Mapped(_) => true,
        Array(array) => contains_mapped(array.get_base()),
        Tuple(tuple) => tuple.get_types().iter().any(contains_mapped),
        Union(union) => union.into_vec().iter().any(contains_mapped),
        Intersection(intersection) => intersection.get_types().iter().any(contains_mapped),
        Object(object) => {
            object.get_fields().values().any(contains_mapped)
                || object
                    .get_index_access()
                    .iter()
                    .any(|(k, v)| contains_mapped(k) || contains_mapped(v))
        }
        Variadic(variadic) => match variadic.as_ref() {
            crate::VariadicType::Base(base) => contains_mapped(base),
            crate::VariadicType::Multi(types) => types.iter().any(contains_mapped),
        },
        Call(call) => call.get_operands().iter().any(contains_mapped),
        Generic(generic) => generic.get_params().iter().any(contains_mapped),
        _ => false,
    }
}

fn mismatch_message(
    semantic_model: &SemanticModel<'_>,
    arg_ty: &LuaType,
    param_ty: &LuaType,
    name: &str,
) -> String {
    let _ = name;
    t!(
        "expected `%{source}` but found `%{found}`. %{reason}",
        source = humanize_type(semantic_model, param_ty),
        found = humanize_type(semantic_model, arg_ty),
        reason = ""
    )
    .to_string()
}
