//! # infer -- the `LuaType` inference layer (expressions / calls / callback params)
//!
//! Built on top of the TypeShell structural inference layer (bounded fixpoint):
//! - Cycle convergence is guaranteed by the TypeShell layer (`cycle_fn`); this layer
//!   does not perform fixpoint iteration;
//! - This layer's "memo" is salsa queries (`decl_type`/`member_type`/`expr_type_of`),
//!   and its "recursion guard" is the TypeShell layer's cycle convergence
//!   (`Recursive -> Unknown`);
//! - The output is a faithfully projected `LuaType`, consumed directly by `type_check`.
//!
//! Call inference and callback-parameter back-inference use `unify`
//! (actual argument -> formal parameter generic unification), e.g. the
//! `map(list, function(x, y) end)` scenario.
//!
//! Public type-query entry points live on `SemanticModel::type_of_*`; this module only
//! contains engine-level functions (VM expression inference, call generic
//! back-inference, closure parameter back-inference) and does not duplicate model methods.

pub(crate) mod function_solver;
pub(crate) mod overload;
pub(crate) mod unify;
pub(crate) mod vm;

pub use vm::infer_doc_func;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaSyntaxId};

use crate::LuaType;

use super::SemanticModel;

#[cfg(test)]
mod legacy_tests;
#[cfg(test)]
mod test;

/// Type of an expression (by syntax position): name/index/call/literal/operator.
/// Uses bytecode VM compilation plus flat interpretation (no recursion; cycle
/// protection lives in the interpreter).
pub fn infer_expr(model: &SemanticModel, expr_syntax: LuaSyntaxId) -> LuaType {
    vm::infer_expr_vm(model, expr_syntax)
}

/// Type of the `param_index`-th closure parameter (VM: compile the wrapping call,
/// then back-infer from the environment).
pub fn closure_param_lua(
    model: &SemanticModel,
    closure_syntax: LuaSyntaxId,
    param_index: usize,
) -> LuaType {
    // Prevent recompiling the wrapping call during closure-return inference
    // (`closure_return_type_with_env` evaluating `f(x)` may resolve a same-named param
    // declaration back to the same call site, causing infinite recursion).
    if model.is_in_closure_return_infer(closure_syntax) {
        return LuaType::Unknown;
    }
    vm::closure_param_vm(model, closure_syntax, param_index)
}

/// Closure return type (semantic VM): when `---@return` signatures or member fields
/// cannot provide a return type, scan the function body's `return` statements with the
/// full semantic model, preserving structures TypeShell cannot express such as `never`
/// and intersection members.
pub(crate) fn closure_return_lua(model: &SemanticModel, closure_syntax: LuaSyntaxId) -> LuaType {
    if model.is_in_closure_return_infer(closure_syntax) {
        return LuaType::Unknown;
    }
    let vm = vm::InferVm::new(model, &[]);
    vm.closure_return_type_with_env(closure_syntax)
}

/// Call type plus generic bindings (`T -> actual argument type`; consumed by the
/// generic constraint checker).
pub fn infer_call_with_bindings(
    model: &SemanticModel,
    call_syntax: LuaSyntaxId,
) -> Option<(LuaType, unify::TplBindings)> {
    let tree = model.syntax_tree()?;
    let root = tree.get_red_root();
    let node = call_syntax.to_node_from_root(&root)?;
    let call_expr = LuaCallExpr::cast(node)?;
    let callee = call_expr.get_prefix_expr()?;
    let callee_ty = model.type_of_expr(callee.get_syntax_id());
    let callee_fun = match callee_ty {
        LuaType::DocFunction(fun) => fun,
        _ => {
            // Runtime members/class-table methods are often projected as bare `Function`;
            // look up the real signature from the member declaration so calls like
            // `A.add(B)` can still back-infer function-level generics.
            if let emmylua_parser::LuaExpr::IndexExpr(index_expr) = &callee {
                let resolved = model.resolve_member(index_expr)?;
                let member_id = resolved.member_id?;
                let member_file = match &member_id {
                    crate::SemanticId::Member(key) => key.file_id,
                    _ => return None,
                };
                let member = model.file_facts_of(member_file)?.member_by_id(&member_id)?;
                let value_syntax = member.value_syntax?;
                std::sync::Arc::new(model.type_of_signature_in_file(member_file, value_syntax)?)
            } else {
                return None;
            }
        }
    };

    // Argument types.
    let arg_types: Vec<LuaType> = call_expr
        .get_args_list()
        .map(|list| {
            list.get_args()
                .map(|arg| model.type_of_expr(arg.get_syntax_id()))
                .collect()
        })
        .unwrap_or_default();

    // unify: formal params <-> actual args, back-inferring generic bindings.
    let mut bindings = unify::TplBindings::new();
    for (param, arg) in callee_fun.get_params().iter().zip(arg_types.iter()) {
        if let Some(param_ty) = &param.1 {
            // Callback params themselves do not need unify (callback internals are
            // inferred via closure params), so skip structural mismatch; function params
            // use the same unification engine as call matching to back-infer callback
            // return generics.
            let _ = vm::unify_call_bindings(model, param_ty, arg, &mut bindings);
        }
    }

    // Substitute the return type.
    let ret = unify::substitute(callee_fun.get_ret(), &bindings);
    let ret = super::type_eval::expand_alias_generic(model, &ret);
    let ret = super::type_eval::eval_conditionals(model, &ret);
    Some((ret, bindings))
}

/// Types for a multi-value expression list (mirrors the old `infer_expr_list_types`):
/// infer each expression; when `var_count` is known, expand trailing multi-return /
/// variadic expressions by slot and truncate.
pub fn infer_expr_list_types(
    model: &SemanticModel,
    exprs: &[emmylua_parser::LuaExpr],
    var_count: Option<usize>,
) -> Vec<(LuaType, rowan::TextRange)> {
    let mut value_types: Vec<(LuaType, rowan::TextRange)> = Vec::new();
    for (idx, expr) in exprs.iter().enumerate() {
        if let Some(var_count) = var_count
            && value_types.len() >= var_count
        {
            break;
        }

        let expr_type = infer_expr(model, expr.get_syntax_id());
        if let Some(var_count) = var_count
            && expr_type.contain_multi_return()
        {
            if idx < var_count {
                for i in idx..var_count {
                    if let Some(typ) = expr_type.get_result_slot_type(i - idx) {
                        value_types.push((typ, expr.get_range()));
                    } else {
                        break;
                    }
                }
            }
            break;
        }

        match &expr_type {
            LuaType::Variadic(variadic) => {
                match variadic.as_ref() {
                    crate::VariadicType::Base(base) => {
                        value_types.push((base.clone(), expr.get_range()));
                    }
                    crate::VariadicType::Multi(types) => {
                        for typ in types {
                            value_types.push((typ.clone(), expr.get_range()));
                        }
                    }
                }
                break;
            }
            _ => value_types.push((expr_type, expr.get_range())),
        }
    }
    value_types
}
