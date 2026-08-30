//! # await_in_sync — calling `---@async` functions in sync contexts
//!
//! Two report cases (mirrors legacy `diagnostic::checker::await_in_sync`):
//! 1. an async callee is called in a sync context;
//! 2. an async function is passed as an argument to a `sync fun(...)` parameter.
//!
//! Async context recognition:
//! - an ancestor closure signature has `---@async`;
//! - `---@async` is attached to a statement/expression containing an anonymous closure (`---@async return function() ...`,
//!   `---@async name(function() ...)`) → that closure is treated as async;
//! - a closure is an argument to an async function parameter (`---@param cb async fun()`) → treated as async;
//! - `pcall/xpcall` callbacks execute immediately in current sync semantics, so skip that closure layer and keep looking outward for an async context.

use std::collections::HashSet;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallArgList, LuaCallExpr, LuaClosureExpr, LuaComment, LuaDocTag,
    LuaExpr, LuaSyntaxId,
};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;
use crate::{AsyncState, LuaFunctionType, LuaType, LuaUnionType};

use super::param_type_check::callable_candidates;
use super::{CheckContext, Checker};

pub struct AwaitInSyncChecker;

impl Checker for AwaitInSyncChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::AwaitInSync];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        let async_closures = collect_async_closures(&root);
        for call_expr in root.descendants().filter_map(LuaCallExpr::cast) {
            check_call_in_async(context, semantic_model, &call_expr, &async_closures);
            check_call_as_arg(context, semantic_model, &call_expr, &async_closures);
        }
    }
}

/// Closures under `---@async` comment owners (mirrors legacy `find_owner_closure`):
/// func stat → its closure; other owners → first closure in the owner subtree.
fn collect_async_closures(root: &emmylua_parser::LuaSyntaxNode) -> HashSet<LuaSyntaxId> {
    let mut set = HashSet::new();
    for comment in root.descendants().filter_map(LuaComment::cast) {
        let is_async = comment
            .get_doc_tags()
            .any(|tag| matches!(tag, LuaDocTag::Async(_)));
        if !is_async {
            continue;
        }
        let Some(owner) = comment.get_owner() else {
            continue;
        };
        let closure = match owner {
            LuaAst::LuaFuncStat(stat) => stat.get_closure(),
            LuaAst::LuaLocalFuncStat(stat) => stat.get_closure(),
            owner => owner.descendants::<LuaClosureExpr>().next(),
        };
        if let Some(closure) = closure {
            set.insert(closure.get_syntax_id());
        }
    }
    set
}

/// Candidate function signatures for the callee expression (reuses param_type_check member/file resolution).
fn callee_candidates(
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) -> Vec<LuaFunctionType> {
    let Some(prefix_expr) = call_expr.get_prefix_expr() else {
        return Vec::new();
    };
    let mut candidates = callable_candidates(semantic_model, &prefix_expr);
    // The VM prefers inferred closure-body signatures, so doc details such as `---@param f sync fun()` / `---@async`
    // are lost; put the declaration-site doc signature first here (legacy infer_call_expr_func preferred docs).
    if let LuaExpr::NameExpr(name_expr) = &prefix_expr
        && let Some(name) = name_expr.get_name_text()
    {
        let decl = semantic_model
            .resolve_name(name_expr.get_position())
            .or_else(|| semantic_model.global_decl(name.as_str()));
        if let Some(decl) = decl
            && let Some(func) = doc_signature_for_decl(semantic_model, &decl)
        {
            if !candidates.contains(&func) {
                candidates.insert(0, func);
            }
        }
    }
    candidates
}

/// Structural signature from declaration-site doc annotations (the `type_of_signature_in_file` version without a return point:
/// when an empty body has no return, `signature_return` is None, but async/sync parameters still participate in checks).
fn doc_signature_for_decl(
    semantic_model: &SemanticModel<'_>,
    decl: &crate::salsa_builder::def::SemanticId,
) -> Option<LuaFunctionType> {
    use crate::salsa_builder::def::SemanticId;
    let SemanticId::Decl(decl_key) = decl else {
        return None;
    };
    let facts = semantic_model.file_facts_of(decl_key.file_id)?;
    let decl = facts.decl_by_id(decl)?;
    let closure_syntax = decl.value_expr_syntax?;
    let signature = facts.signature_by_closure(closure_syntax)?;
    let docs = signature.docs.as_ref()?;
    let mut params = Vec::new();
    for name in signature.param_names.iter() {
        let mut ty = docs
            .param_types
            .iter()
            .find(|(param_name, _)| param_name == name)
            .map(|(_, syntax)| semantic_model.doc_type_lua_rich_in(decl_key.file_id, *syntax))
            .unwrap_or(LuaType::Any);
        if docs.nullable_params.iter().any(|n| n == name) && !ty.is_nullable() {
            ty = LuaType::Union(std::sync::Arc::new(LuaUnionType::from_vec(vec![
                ty,
                LuaType::Nil,
            ])));
        }
        params.push((name.to_string(), Some(ty)));
    }
    let async_state = if docs.is_async {
        AsyncState::Async
    } else {
        AsyncState::None
    };
    Some(LuaFunctionType::new(
        async_state,
        signature.is_method,
        false,
        params,
        LuaType::Unknown,
        None,
    ))
}

fn check_call_in_async(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
    async_closures: &HashSet<LuaSyntaxId>,
) {
    let Some(prefix_expr) = call_expr.get_prefix_expr() else {
        return;
    };
    let candidates = callee_candidates(semantic_model, call_expr);
    if !candidates
        .iter()
        .any(|fun| fun.get_async_state() == AsyncState::Async)
    {
        return;
    }
    if in_async_context(semantic_model, call_expr, async_closures) {
        return;
    }
    context.add_diagnostic(
        DiagnosticCode::AwaitInSync,
        prefix_expr.get_range(),
        t!("Async function can only be called in async function."),
    );
}

/// A `sync fun(...)` parameter receiving an async argument → report at the argument.
fn check_call_as_arg(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
    async_closures: &HashSet<LuaSyntaxId>,
) {
    let Some(args_list) = call_expr.get_args_list() else {
        return;
    };
    let args = args_list.get_args().collect::<Vec<_>>();
    if args.is_empty() {
        return;
    }
    let candidates = callee_candidates(semantic_model, call_expr);
    let colon_call = call_expr.is_colon_call();
    // Report the same argument only once (overload candidates may reach the same conclusion).
    let mut reported = HashSet::new();
    for fun in &candidates {
        let colon_define = fun.is_colon_define();
        for (param_idx, (_, param_type)) in fun.get_params().iter().enumerate() {
            let Some(param_type) = param_type else {
                continue;
            };
            if param_type_async_state(param_type) != Some(AsyncState::Sync) {
                continue;
            }
            let arg_idx = match (colon_define, colon_call) {
                (true, false) => param_idx + 1,
                (false, true) => {
                    if param_idx == 0 {
                        continue;
                    }
                    param_idx - 1
                }
                _ => param_idx,
            };
            let Some(arg) = args.get(arg_idx) else {
                continue;
            };
            if !reported.insert(arg.get_range()) {
                continue;
            }
            if !expr_is_async_function(semantic_model, arg) {
                continue;
            }
            if in_async_context(semantic_model, call_expr, async_closures) {
                continue;
            }
            context.add_diagnostic(
                DiagnosticCode::AwaitInSync,
                arg.get_range(),
                t!("Async function can only be called in async function."),
            );
        }
    }
}

/// Whether the call site is in an async context.
fn in_async_context(
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
    async_closures: &HashSet<LuaSyntaxId>,
) -> bool {
    for closure in call_expr
        .syntax()
        .ancestors()
        .filter_map(LuaClosureExpr::cast)
    {
        if closure_is_async(semantic_model, &closure, async_closures) {
            return true;
        }
        // pcall/xpcall callbacks execute immediately: that closure layer inherits the async state of the outer call site.
        if is_pcall_callback(&closure) {
            continue;
        }
        return false;
    }
    false
}

fn closure_is_async(
    semantic_model: &SemanticModel<'_>,
    closure: &LuaClosureExpr,
    async_closures: &HashSet<LuaSyntaxId>,
) -> bool {
    if async_closures.contains(&closure.get_syntax_id()) {
        return true;
    }
    if let Some(facts) = semantic_model.file_facts()
        && let Some(signature) = facts.signature_by_closure(closure.get_syntax_id())
        && signature.docs.as_ref().is_some_and(|docs| docs.is_async)
    {
        return true;
    }
    // Argument closure: the corresponding outer callee parameter is an async function type.
    callback_param_is_async(semantic_model, closure)
}

/// Whether the matching parameter is an async function type when a closure is passed as a call argument.
fn callback_param_is_async(semantic_model: &SemanticModel<'_>, closure: &LuaClosureExpr) -> bool {
    let Some((call_expr, param_idx)) = closure_call_arg(&closure) else {
        return false;
    };
    let candidates = callee_candidates(semantic_model, &call_expr);
    let colon_call = call_expr.is_colon_call();
    candidates.iter().any(|fun| {
        let colon_define = fun.is_colon_define();
        let idx = match (colon_define, colon_call) {
            (true, false) => param_idx + 1,
            (false, true) => {
                if param_idx == 0 {
                    return false;
                }
                param_idx - 1
            }
            _ => param_idx,
        };
        fun.get_params()
            .get(idx)
            .and_then(|(_, ty)| ty.as_ref())
            .and_then(param_type_async_state)
            == Some(AsyncState::Async)
    })
}

/// Whether the closure is the first argument of `pcall/xpcall` (an immediately executed callback).
fn is_pcall_callback(closure: &LuaClosureExpr) -> bool {
    let Some((call_expr, arg_idx)) = closure_call_arg(closure) else {
        return false;
    };
    if arg_idx != 0 {
        return false;
    }
    let Some(LuaExpr::NameExpr(name_expr)) = call_expr.get_prefix_expr() else {
        return false;
    };
    matches!(
        name_expr.get_name_text().as_deref(),
        Some("pcall") | Some("xpcall")
    )
}

/// Position of a closure as a call argument (directly inside LuaCallArgList).
fn closure_call_arg(closure: &LuaClosureExpr) -> Option<(LuaCallExpr, usize)> {
    let arg_list = closure.get_parent::<LuaCallArgList>()?;
    let call_expr = arg_list.get_parent::<LuaCallExpr>()?;
    let position = closure.get_position();
    let index = arg_list
        .get_args()
        .position(|arg| arg.get_position() == position)?;
    Some((call_expr, index))
}

/// Async state of a parameter type (determined by any function component in the union).
fn param_type_async_state(ty: &LuaType) -> Option<AsyncState> {
    match ty {
        LuaType::DocFunction(fun) => Some(fun.get_async_state()),
        LuaType::Union(union) => {
            let mut state = None;
            for ty in union.into_vec() {
                if let LuaType::DocFunction(fun) = ty {
                    state = Some(fun.get_async_state());
                }
            }
            state
        }
        _ => None,
    }
}

/// Whether the argument expression is an async function (NameExpr cross-file declarations resolve through callable_candidates).
fn expr_is_async_function(semantic_model: &SemanticModel<'_>, expr: &LuaExpr) -> bool {
    callable_candidates(semantic_model, expr)
        .iter()
        .any(|fun| fun.get_async_state() == AsyncState::Async)
}
