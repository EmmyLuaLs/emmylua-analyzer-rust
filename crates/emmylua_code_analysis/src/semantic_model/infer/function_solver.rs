//! # Function type compatibility solver (explicit work queue, no recursion)
//!
//! The only part of higher-order generics that truly needs "recursion" is nested
//! function signatures (callback params / returned functions). This module flattens
//! that into a constraint work queue:
//!
//! - Parameter positions are **contravariant**: callers pass values according to the
//!   `expected` parameter types, so `actual` must be able to accept them;
//! - Return positions are **covariant**: `actual`'s return must be assignable to
//!   `expected`'s return;
//! - Generic bindings (`TplRef`) accumulate in the solver; after a binding is merged,
//!   it is substituted back into later constraints;
//! - Nested functions become new `FnPair` work items rather than recursive stack calls.
//!
//! Leaf-type decisions reuse `type_check::is_compatible` (named types/alias/union etc.);
//! this module only makes the function structure iterative.

use std::collections::VecDeque;

use crate::LuaFunctionType;
use crate::LuaType;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::type_check::is_compatible;

use super::unify::{self, TplBindings};

/// A constraint: a type relationship that needs to be decided.
#[derive(Debug, Clone)]
enum Constraint {
    /// Function pair: can `actual` be used as `expected`.
    FnPair(ActualExpected),
    /// Leaf type: can `source` be assigned to `target` (covariant direction).
    Assign { source: LuaType, target: LuaType },
}

#[derive(Debug, Clone)]
struct ActualExpected {
    actual: LuaFunctionType,
    expected: LuaFunctionType,
}

/// Solver result.
#[derive(Debug)]
pub struct FunctionCheckResult {
    pub compatible: bool,
}

/// Contravariant check for function parameter positions: when `source` is a union,
/// every component must be assignable to `target`.
fn parameter_assignable(model: &SemanticModel, source: &LuaType, target: &LuaType) -> bool {
    if let LuaType::Union(union) = source {
        return union
            .into_vec()
            .iter()
            .all(|component| parameter_assignable(model, component, target));
    }
    types_assignable(model, source, target)
}

/// Leaf assignability check: `type_check` requires all union-target components to be
/// compatible; here we also allow "any component compatible" (needed for function
/// parameter positions).
fn types_assignable(model: &SemanticModel, source: &LuaType, target: &LuaType) -> bool {
    if source == target {
        return true;
    }
    if is_compatible(model, source, target) {
        return true;
    }
    if let LuaType::Union(union) = target {
        return union
            .into_vec()
            .iter()
            .any(|component| types_assignable(model, source, component));
    }
    if let LuaType::Union(union) = source {
        return union
            .into_vec()
            .iter()
            .any(|component| types_assignable(model, component, target));
    }
    false
}

/// Iteratively check whether `actual` can be used where `expected` is expected.
///
/// `bindings` are generic bindings already inferred at the call site; this function
/// extends and returns the final bindings.
pub fn functions_compatible(
    model: &SemanticModel,
    actual: &LuaFunctionType,
    expected: &LuaFunctionType,
    bindings: &TplBindings,
) -> FunctionCheckResult {
    let mut bindings = bindings.clone();
    let mut queue = VecDeque::new();
    queue.push_back(Constraint::FnPair(ActualExpected {
        actual: actual.clone(),
        expected: expected.clone(),
    }));

    // Cycle guard: expand each function pair at most once.
    let mut seen_pairs = std::collections::HashSet::new();
    let mut failed = false;

    while let Some(constraint) = queue.pop_front() {
        match constraint {
            Constraint::FnPair(pair) => {
                let key = fn_pair_key(&pair.actual, &pair.expected);
                if !seen_pairs.insert(key) {
                    continue;
                }
                let failed_before = failed;
                expand_function_pair(model, &pair, &mut bindings, &mut queue, &mut |_, _| {
                    failed = true;
                });
                // Stop immediately if expansion hits a hard failure.
                if !failed_before && failed {
                    break;
                }
            }
            Constraint::Assign { source, target } => {
                if !types_assignable(model, &source, &target) {
                    failed = true;
                    break;
                }
            }
        }
    }

    FunctionCheckResult {
        compatible: !failed,
    }
}

/// Expand one function pair: push contravariant parameter constraints and covariant
/// return constraints onto the queue. On leaves call `fail_if_incompatible` (records
/// only the first failure).
fn expand_function_pair(
    model: &SemanticModel,
    pair: &ActualExpected,
    bindings: &mut TplBindings,
    queue: &mut VecDeque<Constraint>,
    fail_if_incompatible: &mut dyn FnMut(LuaType, LuaType),
) {
    let actual_params = pair.actual.get_params();
    let expected_params = pair.expected.get_params();

    for (index, (_, expected_param)) in expected_params.iter().enumerate() {
        let Some(expected_param) = expected_param else {
            continue;
        };
        let actual_param = actual_params
            .get(index)
            .and_then(|(_, ty)| ty.clone())
            .unwrap_or(LuaType::Unknown);

        let expected_param = unify::substitute(expected_param, bindings);
        let actual_param = unify::substitute(&actual_param, bindings);

        // Generic binding: if `actual` is still a TplRef, let `expected` (the value the
        // caller actually passes) flow in.
        unify_bindings_or_queue(&actual_param, &expected_param, bindings, queue);

        // `self` parameter is covariant: if `expected` is `self`, any receiver works
        // (e.g. `fun(self: A) -> fun(self: self)`).
        if expected_param.is_self_infer() || actual_param.is_self_infer() {
            continue;
        }

        // Nested function -> new work item (contravariant parameter).
        match (&actual_param, &expected_param) {
            (LuaType::DocFunction(actual_fun), LuaType::DocFunction(expected_fun)) => {
                queue.push_back(Constraint::FnPair(ActualExpected {
                    actual: actual_fun.as_ref().clone(),
                    expected: expected_fun.as_ref().clone(),
                }));
            }
            // `expected` is a function but `actual` did not resolve to one -> leaf failure.
            (_, LuaType::DocFunction(_)) => {
                fail_if_incompatible(expected_param.clone(), actual_param.clone());
            }
            _ => {
                // Contravariant: `expected` (what the caller passes) must be assignable
                // to `actual` (what the callee accepts). For a source union in parameter
                // position, every component must work because the caller may pass any member.
                if !parameter_assignable(model, &expected_param, &actual_param) {
                    fail_if_incompatible(expected_param.clone(), actual_param.clone());
                }
            }
        }
    }

    // Return covariance: actual return -> expected return.
    let actual_ret = unify::substitute(pair.actual.get_ret(), bindings);
    let expected_ret = unify::substitute(pair.expected.get_ret(), bindings);
    match (&actual_ret, &expected_ret) {
        (LuaType::DocFunction(actual_fun), LuaType::DocFunction(expected_fun)) => {
            queue.push_back(Constraint::FnPair(ActualExpected {
                actual: actual_fun.as_ref().clone(),
                expected: expected_fun.as_ref().clone(),
            }));
        }
        _ => {
            if !types_assignable(model, &actual_ret, &expected_ret) {
                fail_if_incompatible(actual_ret.clone(), expected_ret.clone());
            }
        }
    }
}

/// Unify bindings; on conflict, push a leaf constraint onto the queue instead of
/// recursively deciding complex types here.
fn unify_bindings_or_queue(
    actual: &LuaType,
    expected: &LuaType,
    bindings: &mut TplBindings,
    queue: &mut VecDeque<Constraint>,
) {
    if let LuaType::TplRef(tpl) = actual {
        let id = tpl.get_tpl_id();
        match bindings.get(&id) {
            Some(existing) if existing == expected => {}
            Some(_) => {
                queue.push_back(Constraint::Assign {
                    source: expected.clone(),
                    target: actual.clone(),
                });
            }
            None => {
                bindings.insert(id, expected.clone());
            }
        }
    }
}

/// Function pair key: a lightweight fingerprint beyond `Debug` output, to prevent
/// alias-cycle expansion from exploding.
fn fn_pair_key(actual: &LuaFunctionType, expected: &LuaFunctionType) -> (String, String) {
    (function_fingerprint(actual), function_fingerprint(expected))
}

fn function_fingerprint(fun: &LuaFunctionType) -> String {
    let mut out = String::new();
    for (name, ty) in fun.get_params() {
        out.push_str(name);
        out.push(':');
        if let Some(ty) = ty {
            out.push_str(&format!("{:?}", ty));
        }
        out.push(';');
    }
    out.push_str("->");
    out.push_str(&format!("{:?}", fun.get_ret()));
    out
}
