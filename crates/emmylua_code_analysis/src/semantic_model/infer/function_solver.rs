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
use crate::semantic_model::type_check::{function_param_pairs, is_compatible};

use super::unify::{self, TplBindings};

/// A constraint: a type relationship that needs to be decided.
#[derive(Debug, Clone)]
enum Constraint {
    /// Function pair: can `actual` be used as `expected`.
    FnPair(ActualExpected),
    /// Leaf type: can `source` be assigned to `target` (covariant direction).
    Assign { source: LuaType, target: LuaType },
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ActualExpected {
    actual: LuaFunctionType,
    expected: LuaFunctionType,
}

/// Iteratively check whether `actual` can be used where `expected` is expected.
///
/// `bindings` are generic bindings already inferred at the call site; this function
/// extends a local copy while checking constraints.
pub fn functions_compatible(
    model: &SemanticModel,
    actual: &LuaFunctionType,
    expected: &LuaFunctionType,
    bindings: &TplBindings,
) -> bool {
    let mut bindings = bindings.clone();
    let mut queue = VecDeque::new();
    queue.push_back(Constraint::FnPair(ActualExpected {
        actual: actual.clone(),
        expected: expected.clone(),
    }));

    // Cycle guard: expand each function pair at most once.
    // Type keys are immutable; their OnceCell only caches a stable hash.
    #[allow(clippy::mutable_key_type)]
    let mut seen_pairs = std::collections::HashSet::new();
    while let Some(constraint) = queue.pop_front() {
        match constraint {
            Constraint::FnPair(pair) => {
                if !seen_pairs.insert(pair.clone()) {
                    continue;
                }
                expand_function_pair(&pair, &mut bindings, &mut queue);
            }
            Constraint::Assign { source, target } => {
                if !is_compatible(model, &source, &target) {
                    return false;
                }
            }
        }
    }

    true
}

/// Push contravariant parameter constraints and covariant result constraints.
/// Nested functions stay on the queue instead of calling the checker recursively.
fn expand_function_pair(
    pair: &ActualExpected,
    bindings: &mut TplBindings,
    queue: &mut VecDeque<Constraint>,
) {
    for (expected_param, actual_param) in function_param_pairs(&pair.actual, &pair.expected) {
        let expected_param = unify::substitute(&expected_param, bindings);
        let actual_param = unify::substitute(&actual_param, bindings);
        unify_bindings_or_queue(&actual_param, &expected_param, bindings, queue);
        // Contravariance reverses the function pair as well as leaf types.
        queue_type_pair(expected_param, actual_param, queue);
    }

    // Return covariance: actual return -> expected return.
    let actual_ret = unify::substitute(pair.actual.get_ret(), bindings);
    let expected_ret = unify::substitute(pair.expected.get_ret(), bindings);
    queue_type_pair(actual_ret, expected_ret, queue);
}

fn queue_type_pair(source: LuaType, target: LuaType, queue: &mut VecDeque<Constraint>) {
    match (&source, &target) {
        (LuaType::DocFunction(actual), LuaType::DocFunction(expected)) => {
            queue.push_back(Constraint::FnPair(ActualExpected {
                actual: actual.as_ref().clone(),
                expected: expected.as_ref().clone(),
            }));
        }
        _ => queue.push_back(Constraint::Assign { source, target }),
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
