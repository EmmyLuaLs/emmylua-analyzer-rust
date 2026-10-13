//! # param_type_check - call argument type does not match parameter type
//!
//! M0+: callee candidates = primary signature + `---@overload`; if any candidate accepts all arguments, don't report,
//! otherwise report type and field errors from the candidate with the fewest mismatches. Colon calls:
//! strip one slot for the self parameter; plain functions treat the receiver as the first argument.

use std::rc::Rc;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaSyntaxId, LuaTokenKind,
};

use crate::semantic_model::infer::callable::CallableCandidateSet;
use crate::semantic_model::infer::function_solver::functions_compatible;
use crate::semantic_model::infer::unify;
use crate::semantic_model::infer::vm::{bind_signature_generics, unify_call_bindings};
use crate::semantic_model::render::humanize_type;
use crate::semantic_model::table_constructor::table_expr;
use crate::semantic_model::type_check::{check_type_at, is_compatible};
use crate::semantic_model::{SemanticModel, type_eval};
use crate::{
    DiagnosticCode, LuaFunctionType, LuaMemberKey, LuaTupleStatus, LuaTupleType, LuaType,
    LuaTypeNode, TypeDefKind, VariadicType,
};

use super::param_count::first_param_is_self;
use super::{CheckContext, Checker, Mismatch};

pub struct ParamTypeChecker;

impl Checker for ParamTypeChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::ParamTypeMismatch,
        DiagnosticCode::MissingFields,
        DiagnosticCode::InjectField,
    ];

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
    let forwarded = &args[1..];
    let callback_candidates = candidates_for_arguments(
        semantic_model,
        forwarded,
        false,
        semantic_model.callable_candidates_for_expr(callback_expr),
    );
    if callback_candidates.is_empty() {
        return false;
    }
    let mut best: Option<Vec<Mismatch>> = None;
    for candidate in &callback_candidates {
        let mismatches = check_candidate(
            semantic_model,
            candidate,
            forwarded,
            false,
            &LuaType::Unknown,
            &[],
            None,
        );
        if mismatches.is_empty() {
            return true;
        }
        if best
            .as_ref()
            .is_none_or(|current| mismatch_count(&mismatches) < mismatch_count(current))
        {
            best = Some(mismatches);
        }
    }
    if let Some(mismatches) = best {
        report_mismatches(context, mismatches, &[]);
    }
    true
}

/// Callee expression -> candidate function signatures (DocFunction / member overloads / cross-file global signatures).
pub(crate) fn callable_candidates(
    semantic_model: &SemanticModel<'_>,
    callee: &LuaExpr,
) -> Vec<LuaFunctionType> {
    semantic_model.callable_candidates_cached(callee)
}

pub(crate) fn callable_candidates_uncached(
    semantic_model: &SemanticModel<'_>,
    callee: &LuaExpr,
) -> Vec<LuaFunctionType> {
    // Unified path for name callees: global cross-file overloads must be visible
    // here exactly as they are in the VM call-selection path.
    if let LuaExpr::NameExpr(_) = callee {
        let candidates = semantic_model.callable_candidates_for_name_expr(callee);
        if !candidates.is_empty() {
            return candidates;
        }
    }
    // For static member calls, the member table is the authoritative source of
    // callable signatures.  Avoid paying for full VM expression inference on the
    // callee when the member lookup already tells us there is no callable here.
    if let LuaExpr::IndexExpr(index_expr) = callee {
        let direct = index_expr_callable_candidates(semantic_model, index_expr);
        if !direct.is_empty() {
            return direct;
        }
        let static_key = index_expr.get_index_key().is_some_and(|key| {
            matches!(
                key,
                emmylua_parser::LuaIndexKey::Name(_)
                    | emmylua_parser::LuaIndexKey::String(_)
                    | emmylua_parser::LuaIndexKey::Integer(_)
            )
        });
        if static_key {
            return Vec::new();
        }
    }

    let callee_ty = semantic_model.type_of_expr(callee.get_syntax_id());
    let mut candidates = semantic_model.callable_functions_cached(&callee_ty);
    if candidates.is_empty()
        && let LuaExpr::IndexExpr(index_expr) = callee
    {
        candidates = index_expr_callable_candidates(semantic_model, index_expr);
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

/// Resolve callable signatures directly from an index expression's member declaration.
///
/// P6b: all same-key/inherited members (repeated `---@field`, runtime closures,
/// class call overloads/operators) are collected through `CallableCandidateSet`
/// instead of the old single-member heuristics.
fn index_expr_callable_candidates(
    semantic_model: &SemanticModel<'_>,
    index_expr: &LuaIndexExpr,
) -> Vec<LuaFunctionType> {
    let Some(resolved) = semantic_model.resolve_member(index_expr) else {
        return Vec::new();
    };

    if let Some(prefix) = index_expr.get_prefix_expr() {
        let prefix_ty = semantic_model.type_of_expr(prefix.get_syntax_id());
        let key = LuaMemberKey::Name(resolved.name.clone());
        let candidates = match &resolved.member_id {
            Some(member_id) => CallableCandidateSet::from_prefix_type_for_member(
                semantic_model,
                &prefix_ty,
                &key,
                member_id,
            ),
            None => CallableCandidateSet::from_prefix_type(semantic_model, &prefix_ty, &key),
        };
        let mut candidates = candidates.into_candidates();
        // A partial class can have both `@field` overloads and runtime methods on its local owner.
        for member in semantic_model
            .members_of_owner_named(&resolved.owner, &resolved.name)
            .iter()
        {
            if let Some(functions) =
                crate::semantic_model::infer::vm::InferVm::new(semantic_model, &[])
                    .callable_candidates_for_owner_single(&member.id)
            {
                candidates.extend(functions);
            }
        }
        if !candidates.is_empty() {
            return CallableCandidateSet::new(candidates).into_candidates();
        }
    }

    // Legacy fallback: prefix inference failed, but the resolved member itself
    // carries a usable signature.
    let mut candidates = Vec::new();
    if let Some(member_id) = &resolved.member_id
        && let Some(member_ty) = semantic_model.type_of_member(member_id)
    {
        candidates = semantic_model.callable_functions_cached(&member_ty);
    }
    if candidates.is_empty()
        && let Some(member_file) = resolved.file_id
        && let Some(member_id) = &resolved.member_id
        && let Some(facts) = semantic_model.file_facts_of(member_file)
        && let Some(member) = facts.member_by_id(member_id)
        && let Some(value_syntax) = member.value_syntax
        && let Some(func) = semantic_model.type_of_signature_in_file(member_file, value_syntax)
    {
        candidates.push(func);
    }
    candidates
}

/// Prefer signatures whose argument counts fit, before comparing argument types.
fn candidates_for_arguments(
    model: &SemanticModel<'_>,
    args: &[LuaExpr],
    colon_call: bool,
    candidates: Vec<LuaFunctionType>,
) -> Vec<LuaFunctionType> {
    let matching = candidates
        .iter()
        .filter(|candidate| {
            super::param_count::candidate_accepts_arguments(model, candidate, args, colon_call)
        })
        .cloned()
        .collect::<Vec<_>>();
    if matching.is_empty() {
        candidates
    } else {
        matching
    }
}

fn check_call(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    let Some(callee) = call_expr.get_prefix_expr() else {
        return;
    };
    let analysis = semantic_model.call_site_analysis(call_expr);
    let args = call_expr
        .get_args_list()
        .map(|list| list.get_args().collect::<Vec<_>>())
        .unwrap_or_default();
    let candidates = candidates_for_arguments(
        semantic_model,
        &args,
        analysis.colon_call,
        analysis.candidates,
    );
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
    let colon_call = analysis.colon_call;
    let receiver_ty = analysis.receiver_ty;
    let explicit_generics = &analysis.explicit_generics;

    // Cheap candidate ordering: check likely overloads first so the common
    // “one candidate matches” case returns before running the expensive
    // parameter-by-parameter check on every other overload.
    let mut ordered: Vec<&LuaFunctionType> = candidates.iter().collect();
    ordered.sort_by_key(|func| {
        quick_candidate_score(func, &analysis.arg_types, colon_call, &receiver_ty)
    });

    let mut best: Option<Vec<Mismatch>> = None;
    for candidate in ordered {
        let mismatches = check_candidate(
            semantic_model,
            candidate,
            &args,
            colon_call,
            &receiver_ty,
            explicit_generics,
            Some(&analysis.arg_types),
        );
        if mismatches.is_empty() {
            return;
        }
        if best
            .as_ref()
            .is_none_or(|current| mismatch_count(&mismatches) < mismatch_count(current))
        {
            best = Some(mismatches);
        }
    }
    if let Some(mismatches) = best {
        let literal_names = doc_overload_literal_names(semantic_model, &callee);
        report_mismatches(context, mismatches, &literal_names);
    }
}

fn field_error_covers(field: &Mismatch, range: rowan::TextRange) -> bool {
    matches!(
        field.code,
        DiagnosticCode::MissingFields | DiagnosticCode::InjectField
    ) && range.contains_range(field.range)
}

fn mismatch_count(mismatches: &[Mismatch]) -> usize {
    mismatches
        .iter()
        .filter(|mismatch| {
            mismatch.code != DiagnosticCode::ParamTypeMismatch
                || !mismatches
                    .iter()
                    .any(|field| field_error_covers(field, mismatch.range))
        })
        .count()
}

fn report_mismatches(
    context: &mut CheckContext<'_>,
    mismatches: Vec<Mismatch>,
    literal_names: &[String],
) {
    for mismatch in &mismatches {
        // Prefer an enabled field error over the whole argument's type error.
        // Candidate validation and selection use the unfiltered errors.
        if mismatch.code == DiagnosticCode::ParamTypeMismatch
            && mismatches.iter().any(|field| {
                field_error_covers(field, mismatch.range)
                    && context.is_diagnostic_enabled(field.code, field.range)
            })
        {
            continue;
        }
        let message =
            if mismatch.code == DiagnosticCode::ParamTypeMismatch && literal_names.len() >= 2 {
                let found = mismatch
                    .message
                    .split_once("but found")
                    .map(|(_, found)| format!("but found{found}"))
                    .unwrap_or_else(|| mismatch.message.clone());
                format!("expected `{}` {found}", literal_names.join(" | "))
            } else {
                mismatch.message.clone()
            };
        context.add_diagnostic(mismatch.code, mismatch.range, message);
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

/// A cheap literal/scalar-shape score used only to order call candidates.
/// It does not replace the real compatibility check; it only avoids running
/// the expensive type-check path on obviously wrong overloads first.
fn quick_candidate_score(
    func: &LuaFunctionType,
    arg_types: &[LuaType],
    colon_call: bool,
    receiver_ty: &LuaType,
) -> usize {
    let params = func.get_params();
    let self_param = first_param_is_self(func);
    let param_start = usize::from(colon_call && self_param);
    let dot_method_receiver = !colon_call && !self_param && func.is_colon_define();
    let arg_offset = usize::from(dot_method_receiver);
    let mut score = 0usize;

    let mut check_pair = |arg_ty: &LuaType, param_ty: &LuaType| {
        if obvious_scalar_mismatch(arg_ty, param_ty) {
            score += 1;
        }
    };

    if colon_call && !self_param && !func.is_colon_define() {
        if let Some((_, Some(param_ty))) = params.first() {
            check_pair(receiver_ty, param_ty);
        }
        let remainder = if params.is_empty() {
            params
        } else {
            &params[1..]
        };
        for (index, (_, param_ty)) in remainder.iter().enumerate() {
            let Some(param_ty) = param_ty else {
                continue;
            };
            let Some(arg_ty) = arg_types.get(index) else {
                break;
            };
            check_pair(arg_ty, param_ty);
        }
    } else {
        for (index, (_, param_ty)) in params[param_start..].iter().enumerate() {
            let Some(param_ty) = param_ty else {
                continue;
            };
            let Some(arg_ty) = arg_types.get(index + arg_offset) else {
                break;
            };
            check_pair(arg_ty, param_ty);
        }
    }
    score
}

fn obvious_scalar_mismatch(arg_ty: &LuaType, param_ty: &LuaType) -> bool {
    match param_ty {
        LuaType::Any | LuaType::Unknown | LuaType::Never => return false,
        LuaType::Union(union) => {
            return !union
                .into_vec()
                .iter()
                .any(|component| !obvious_scalar_mismatch(arg_ty, component));
        }
        _ => {}
    }
    match (arg_ty, param_ty) {
        (
            LuaType::StringConst(_) | LuaType::DocStringConst(_),
            LuaType::String
            | LuaType::StringConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::StrTplRef(_),
        ) => false,
        (LuaType::StringConst(_) | LuaType::DocStringConst(_), _) => true,
        (
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
            LuaType::Integer
            | LuaType::Number
            | LuaType::IntegerConst(_)
            | LuaType::DocIntegerConst(_),
        ) => false,
        (LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_), _) => true,
        (
            LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_),
            LuaType::Boolean | LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_),
        ) => false,
        (LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_), _) => true,
        _ => false,
    }
}

fn check_candidate(
    semantic_model: &SemanticModel<'_>,
    func: &LuaFunctionType,
    args: &[LuaExpr],
    colon_call: bool,
    receiver_ty: &LuaType,
    explicit_generics: &[LuaSyntaxId],
    arg_types: Option<&[LuaType]>,
) -> Vec<Mismatch> {
    // Signature projections can leave generic names as `Ref("T")`.
    let params = func
        .get_params()
        .iter()
        .map(|(name, ty)| {
            (
                name.clone(),
                ty.as_ref()
                    .map(|ty| bind_signature_generics(ty, func.get_generic_params())),
            )
        })
        .collect::<Vec<_>>();
    let self_param = first_param_is_self(func);
    let param_start = usize::from(colon_call && self_param).min(params.len());
    // Method definition called with dot syntax (`obj.method(obj, ...)` / `pcall(obj.method, obj, ...)`):
    // the semantic method signature contains only user parameters; the first actual argument is an explicit receiver and must be skipped.
    let dot_method_receiver = !colon_call && !self_param && func.is_colon_define();
    let arg_offset = usize::from(dot_method_receiver).min(args.len());

    // Per-candidate generic binding table: bindings inferred from earlier arguments (T[]) are used for later callback parameters.
    let mut bindings = unify::TplBindings::new();
    // Explicit call generics `test--[[@<number | string>]]()` fill bindings first instead of inferring from arguments.
    for (index, syntax) in explicit_generics.iter().enumerate() {
        if let Some(tpl) = func.get_generic_params().get(index) {
            let ty = semantic_model.doc_type_lua_rich(*syntax);
            bindings.insert(tpl.get_tpl_id(), ty);
        }
    }

    // Plain function called with colon syntax: the receiver occupies the first argument slot.
    // Method definitions (`function C:m()`) store only user parameters, so the implicit
    // receiver is not part of the user-facing parameter list and should not be compared
    // against the first declared parameter.
    if colon_call && !self_param && !func.is_colon_define() {
        let mut mismatches = Vec::new();
        if let Some((name, Some(param_ty))) = params.first() {
            let param_ty = effective_param(param_ty, receiver_ty);
            let _ = unify_call_bindings(semantic_model, &param_ty, receiver_ty, &mut bindings);
            if !is_compatible(semantic_model, receiver_ty, &param_ty) {
                mismatches.push(Mismatch {
                    code: DiagnosticCode::ParamTypeMismatch,
                    range: receiver_range(args),
                    message: mismatch_message(semantic_model, receiver_ty, &param_ty, name),
                });
            }
        }
        let remainder_params = if params.is_empty() {
            params.as_slice()
        } else {
            &params[1..]
        };
        mismatches.extend(check_arg_pairs(
            semantic_model,
            remainder_params,
            args,
            func.is_variadic(),
            &mut bindings,
            arg_types,
        ));
        return mismatches;
    }

    // When a colon-defined method is invoked with dot syntax, the explicit receiver is
    // part of `args` but not part of the user-facing parameter list. Slice `arg_types` too;
    // otherwise cached argument types are shifted and the wrong argument is type-checked.
    let dot_arg_types = arg_types.map(|types| {
        let offset = arg_offset.min(types.len());
        &types[offset..]
    });
    check_arg_pairs(
        semantic_model,
        &params[param_start..],
        &args[arg_offset..],
        func.is_variadic(),
        &mut bindings,
        dot_arg_types,
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

fn cached_arg_type(
    semantic_model: &SemanticModel<'_>,
    args: &[LuaExpr],
    arg_types: Option<&[LuaType]>,
    index: usize,
) -> LuaType {
    if let Some(types) = arg_types
        && let Some(ty) = types.get(index)
    {
        return ty.clone();
    }
    args.get(index)
        .map(|arg| semantic_model.type_of_expr(arg.get_syntax_id()))
        .unwrap_or(LuaType::Unknown)
}

fn check_arg_pairs(
    semantic_model: &SemanticModel<'_>,
    params: &[(String, Option<LuaType>)],
    args: &[LuaExpr],
    is_variadic: bool,
    bindings: &mut unify::TplBindings,
    arg_types: Option<&[LuaType]>,
) -> Vec<Mismatch> {
    let mut out = Vec::new();
    for (index, (name, param_ty)) in params.iter().enumerate() {
        let is_vararg_slot = is_variadic && index + 1 == params.len();
        if is_vararg_slot {
            // A call may pass fewer arguments than the variadic slot position
            // (`function f(a, b, ...)` called as `f(1)`), in which case the variadic
            // parameter receives no argument at all. Clamp the slice start instead of
            // panicking on `args[index..]`.
            let rest_start = index.min(args.len());
            let rest_args = &args[rest_start..];
            if let Some(param_ty) = param_ty {
                let param_ty = unify::substitute(param_ty, bindings);
                let param_ty = type_eval::expand_alias_generic(semantic_model, &param_ty);
                let param_ty = type_eval::eval_conditionals(semantic_model, &param_ty);
                if let LuaType::Variadic(variadic) = &param_ty {
                    // `@param ... T...`: T is bound to a tuple of the whole argument sequence, not requiring the same type for each argument.
                    if let VariadicType::Base(base) = variadic.as_ref()
                        && let LuaType::TplRef(tpl) = base
                        && !bindings.contains_key(&tpl.get_tpl_id())
                    {
                        let rest_types: Vec<LuaType> = rest_args
                            .iter()
                            .enumerate()
                            .map(|(offset, _)| {
                                let ty = cached_arg_type(
                                    semantic_model,
                                    args,
                                    arg_types,
                                    rest_start + offset,
                                );
                                normalize_arg_for_check(semantic_model, &ty)
                            })
                            .collect();
                        let tuple_ty = LuaType::Tuple(Rc::new(LuaTupleType::new(
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
                    for (variadic_index, arg) in rest_args.iter().enumerate() {
                        let Some(slot_ty) = variadic.get_type(variadic_index) else {
                            break;
                        };
                        let arg_ty = cached_arg_type(
                            semantic_model,
                            args,
                            arg_types,
                            rest_start + variadic_index,
                        );
                        let arg_ty = normalize_arg_for_check(semantic_model, &arg_ty);
                        let mut slot_ty = slot_ty.clone();
                        slot_ty = unify::substitute(&slot_ty, bindings);
                        // The same function generic in variadic arguments must be bound consistently across all arguments;
                        // explicit generics (e.g. `T = number | string`) are already substituted and no longer TplRef, so go straight to compatibility checks.
                        if slot_ty.contains_tpl_node()
                            && !unify_call_bindings(semantic_model, &slot_ty, &arg_ty, bindings)
                        {
                            out.push(Mismatch {
                                code: DiagnosticCode::ParamTypeMismatch,
                                range: arg.get_range(),
                                message: mismatch_message(semantic_model, &arg_ty, &slot_ty, name),
                            });
                            continue;
                        }
                        slot_ty = unify::substitute(&slot_ty, bindings);
                        slot_ty = type_eval::expand_alias_generic(semantic_model, &slot_ty);
                        if let Some(mismatches) =
                            table_argument_mismatches(semantic_model, arg, &arg_ty, &slot_ty)
                        {
                            out.extend(mismatches);
                            continue;
                        }
                        if arg_ty == slot_ty || is_compatible(semantic_model, &arg_ty, &slot_ty) {
                            continue;
                        }
                        out.push(Mismatch {
                            code: DiagnosticCode::ParamTypeMismatch,
                            range: arg.get_range(),
                            message: mismatch_message(semantic_model, &arg_ty, &slot_ty, name),
                        });
                    }
                } else {
                    for (offset, arg) in rest_args.iter().enumerate() {
                        let arg_ty =
                            cached_arg_type(semantic_model, args, arg_types, rest_start + offset);
                        let arg_ty = normalize_arg_for_check(semantic_model, &arg_ty);
                        let param_ty_orig = param_ty.clone();
                        let unified =
                            unify_call_bindings(semantic_model, &param_ty_orig, &arg_ty, bindings);
                        if param_ty_orig.contains_tpl_node() && !unified {
                            out.push(Mismatch {
                                code: DiagnosticCode::ParamTypeMismatch,
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
                        if let Some(mismatches) =
                            table_argument_mismatches(semantic_model, arg, &arg_ty, &param_ty)
                        {
                            out.extend(mismatches);
                            continue;
                        }
                        if arg_ty == param_ty
                            || param_generic_base_match(semantic_model, &arg_ty, &param_ty)
                            || is_compatible(semantic_model, &arg_ty, &param_ty)
                        {
                            continue;
                        }
                        out.push(Mismatch {
                            code: DiagnosticCode::ParamTypeMismatch,
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
        let arg_ty = cached_arg_type(semantic_model, args, arg_types, index);
        let arg_ty = normalize_arg_for_check(semantic_model, &arg_ty);
        let call_arg_ty = call_argument_type(semantic_model, arg, &arg_ty);
        let call_arg_ty = normalize_arg_for_check(semantic_model, &call_arg_ty);
        let param_ty = unify::substitute(param_ty, bindings);
        let param_ty = type_eval::expand_alias_generic(semantic_model, &param_ty);
        let param_ty = type_eval::eval_conditionals(semantic_model, &param_ty);
        // Missing union member: when only A in `A|C` has `handle`, accessing `target.handle`
        // should report ParamTypeMismatch even if the type face happens to be string (missing members count as nil).
        if let LuaExpr::IndexExpr(index_expr) = arg
            && semantic_model.member_missing_in_union(index_expr)
        {
            out.push(Mismatch {
                code: DiagnosticCode::ParamTypeMismatch,
                range: arg.get_range(),
                message: mismatch_message(semantic_model, &LuaType::Nil, &param_ty, name),
            });
            continue;
        }
        if skip_arg(&call_arg_ty, &param_ty) {
            continue;
        }
        // Check the whole boolean domain before selecting a single union component.
        if is_compatible(semantic_model, &call_arg_ty, &LuaType::Boolean)
            && !param_ty.contains_tpl_node()
        {
            if !is_compatible(semantic_model, &call_arg_ty, &param_ty) {
                out.push(Mismatch {
                    code: DiagnosticCode::ParamTypeMismatch,
                    range: arg.get_range(),
                    message: mismatch_message(semantic_model, &call_arg_ty, &param_ty, name),
                });
            }
            continue;
        }
        // Keep constructors and callbacks on the full check path below.
        // Generic unions (`` `T`|T ``) must bind T before compatibility can accept them.
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
                LuaType::DocFunction(_)
                    | LuaType::Signature(_)
                    | LuaType::Function
                    | LuaType::TableConst(_)
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
                .any(|component| is_compatible(semantic_model, &call_arg_ty, component))
            {
                continue;
            }
        }
        let param_ty = effective_param(&param_ty, &call_arg_ty);
        // Unify first: T[] <- (A|B|C|D)[] and similar.
        let _ = unify_call_bindings(semantic_model, &param_ty, &call_arg_ty, bindings);
        let param_ty = unify::substitute(&param_ty, bindings);
        // Check constructors after binding this argument, including field constraints
        // that do not depend on unresolved generics.
        if let Some(mismatches) =
            table_argument_mismatches(semantic_model, arg, &call_arg_ty, &param_ty)
        {
            out.extend(mismatches);
            continue;
        }
        // After substituting generic unions (`T|StrTpl` -> `Table|...`), arguments pass if they match any component.
        if let LuaType::Union(union) = &param_ty {
            if union
                .into_vec()
                .iter()
                .any(|component| is_compatible(semantic_model, &call_arg_ty, component))
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
        let leaf_compat = (!matches!(call_arg_ty, LuaType::Function)
            || matches!(param_ty, LuaType::Any))
            && param_generic_base_match(semantic_model, &call_arg_ty, &param_ty);
        if leaf_compat || enum_name_compatible(semantic_model, &call_arg_ty, &param_ty) {
            continue;
        }
        out.push(Mismatch {
            code: DiagnosticCode::ParamTypeMismatch,
            range: arg.get_range(),
            message: mismatch_message(semantic_model, &arg_ty, &param_ty, name),
        });
    }
    out
}

fn table_argument_mismatches(
    model: &SemanticModel<'_>,
    arg: &LuaExpr,
    actual: &LuaType,
    expected: &LuaType,
) -> Option<Vec<Mismatch>> {
    if !matches!(actual, LuaType::TableConst(_)) {
        return None;
    }
    let mut mismatches = table_expr(arg).map_or_else(Vec::new, |table| {
        super::check_field::table_literal_mismatches(model, expected, &table)
    });
    if check_type_at(model, actual, expected, arg.get_range(), false).is_err() {
        mismatches.push(Mismatch {
            code: DiagnosticCode::ParamTypeMismatch,
            range: arg.get_range(),
            message: mismatch_message(model, actual, expected, ""),
        });
    }
    Some(mismatches)
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
    let try_value = |value_ty: &LuaType, bindings: &mut unify::TplBindings| -> bool {
        if let LuaType::TplRef(tpl) = array.get_base() {
            match bindings.get(&tpl.get_tpl_id()) {
                Some(existing) => return existing == value_ty,
                None => {
                    bindings.insert(tpl.get_tpl_id(), value_ty.clone());
                    return true;
                }
            }
        }
        is_compatible(semantic_model, value_ty, array.get_base())
    };
    for (key, value_ty) in object.get_fields() {
        if key.get_integer().is_some() && try_value(value_ty, bindings) {
            return true;
        }
    }
    for (key_ty, value_ty) in object.get_index_access() {
        let numeric = matches!(
            key_ty,
            LuaType::Integer | LuaType::Number | LuaType::IntegerConst(_)
        );
        if numeric && try_value(value_ty, bindings) {
            return true;
        }
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
            return LuaType::DocFunction(Rc::new(func));
        }
    }
    if matches!(constrained, LuaType::Function)
        && let LuaExpr::NameExpr(name_expr) = arg
        && let Some(decl) = semantic_model.resolve_name(name_expr.get_position())
    {
        if let Some(func) = semantic_model.type_of_decl_signature(&decl) {
            return LuaType::DocFunction(Rc::new(func));
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
    if semantic_model.type_def_of(id).is_some() {
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

/// Callback parameters vs function arguments: use the explicit work-queue function solver for structural/contravariant checks.
fn is_function_like_type(ty: &LuaType) -> bool {
    match ty {
        LuaType::DocFunction(_) | LuaType::Signature(_) => true,
        LuaType::Union(union) => union.into_vec().iter().any(is_function_like_type),
        LuaType::Intersection(intersection) => {
            intersection.get_types().iter().any(is_function_like_type)
        }
        _ => false,
    }
}

fn function_arg_compatible(
    semantic_model: &SemanticModel<'_>,
    arg_ty: &LuaType,
    param_ty: &LuaType,
    bindings: &unify::TplBindings,
) -> bool {
    // A broad `function` value (e.g. `---@param cb function`) may be passed
    // wherever a function type is expected. Without this, stdlib generic
    // callbacks (`table.sort(list, comp)`) would report false positives.
    if matches!(arg_ty, LuaType::Function) && is_function_like_type(param_ty) {
        return true;
    }
    let actual = doc_function_of(semantic_model, arg_ty);
    let expected = doc_function_of(semantic_model, param_ty);
    match (actual, expected) {
        (Some(actual), Some(expected)) => {
            functions_compatible(semantic_model, &actual, &expected, bindings)
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
fn doc_function_of(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<LuaFunctionType> {
    match ty {
        LuaType::DocFunction(func) => Some(func.as_ref().clone()),
        LuaType::Signature(signature_id) => {
            semantic_model.function_type_by_signature_id(signature_id)
        }
        LuaType::Ref(id) | LuaType::Def(id) => {
            let def = semantic_model.type_def_of(id)?;
            if def.kind != TypeDefKind::Alias {
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

/// Preserve the argument checker's acceptance of enum member-name strings.
fn enum_name_compatible(
    semantic_model: &SemanticModel<'_>,
    arg_ty: &LuaType,
    param_ty: &LuaType,
) -> bool {
    let (LuaType::StringConst(name) | LuaType::DocStringConst(name)) = arg_ty else {
        return false;
    };
    let mut pending = vec![param_ty.clone()];
    let mut visited = Vec::new();
    while let Some(ty) = pending.pop() {
        match ty {
            LuaType::Union(union) => pending.extend(union.into_vec()),
            LuaType::Ref(id) | LuaType::Def(id) => {
                if visited.contains(&id) {
                    continue;
                }
                visited.push(id.clone());
                let Some(def) = semantic_model.type_def_of(&id) else {
                    continue;
                };
                if def.kind == TypeDefKind::Alias {
                    pending.extend(semantic_model.alias_target(&def));
                } else if def.kind == TypeDefKind::Enum
                    && let Some(facts) = semantic_model.file_facts_of(def.file_id)
                    && let Some(decl) = facts.decl_named(def.name.as_str())
                    && semantic_model
                        .members_of_owner(&decl.id)
                        .iter()
                        .any(|member| member.name.as_str() == name.as_ref())
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Unknown/any/never arguments are not checked.
fn skip_arg(arg_ty: &LuaType, param_ty: &LuaType) -> bool {
    if matches!(arg_ty, LuaType::Unknown | LuaType::Any) {
        return true;
    }
    matches!(arg_ty, LuaType::Nil) && matches!(param_ty, LuaType::Any | LuaType::Unknown)
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
    let expanded = type_eval::expand_alias_generic(semantic_model, ty);
    if contains_mapped(&expanded) {
        return ty.clone();
    }
    type_eval::eval_conditionals(semantic_model, &expanded)
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
            VariadicType::Base(base) | VariadicType::Repeat(base) => contains_mapped(base),
            VariadicType::Multi(types) => types.iter().any(contains_mapped),
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
