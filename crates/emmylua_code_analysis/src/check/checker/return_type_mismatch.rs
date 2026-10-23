//! # return_type_mismatch: actual return types in the function body do not match the `---@return` annotation
//!
//! Only functions **with `@return` annotations** are checked: signature annotation types vs function-body return expression types.
//! Supports multiple return values (union components checked individually), generic parameters (uninstantiated `Ref("T")` skipped),
//! class inheritance alias function types, and `return expr ---@as type` inline assertions.

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaReturnStat};

use crate::LuaType;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::type_check::check_type_at;
use crate::{DiagnosticCode, SemanticId, SignatureDoc, TypeDef};

use super::{CheckContext, Checker};
use crate::semantic_model::render::humanize_type;

pub struct ReturnTypeMismatchChecker;

impl Checker for ReturnTypeMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::ReturnTypeMismatch];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(signatures) = semantic_model.signatures() else {
            return;
        };
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        let default_docs = SignatureDoc::default();
        for signature in signatures {
            let Some(node) = signature.closure_syntax.to_node_from_root(&root) else {
                continue;
            };
            let Some(closure) = LuaClosureExpr::cast(node) else {
                continue;
            };
            // Signatures with `---@return` annotations are checked against the annotation;
            // closure fields in table literals without annotations are checked against the `---@field func? fun(...)` field signature.
            let (expected, docs) = if let Some(sig_docs) = signature.docs.as_deref() {
                if sig_docs.returns.is_empty() {
                    continue;
                }
                let Some(expected) = semantic_model.return_type(signature.closure_syntax) else {
                    continue;
                };
                (expected, sig_docs)
            } else {
                let Some(expected_fun) =
                    semantic_model.expected_member_signature_for_closure(signature.closure_syntax)
                else {
                    continue;
                };
                (expected_fun.get_ret().clone(), &default_docs)
            };
            for ret in own_return_stats(&closure) {
                check_return_stat(context, semantic_model, &ret, &expected, docs);
            }
        }
    }
}

/// Only check return statements belonging to this closure itself.
fn own_return_stats(closure: &LuaClosureExpr) -> Vec<LuaReturnStat> {
    closure
        .descendants::<LuaReturnStat>()
        .filter(|stat| {
            stat.ancestors::<LuaClosureExpr>()
                .next()
                .is_some_and(|expr| &expr == closure)
        })
        .collect()
}

fn check_return_stat(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    ret: &LuaReturnStat,
    expected: &LuaType,
    docs: &SignatureDoc,
) {
    for expr in ret.get_expr_list() {
        let raw = semantic_model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start());
        // Uninstantiated function generic argument `Ref("T")`: check against the constraint when present, otherwise skip.
        if is_unresolved_generic_ref(semantic_model, &raw, docs) {
            if let Some(constraint) = generic_constraint_type(semantic_model, &raw)
                && !return_compatible(semantic_model, &constraint, expected, expr.get_range())
            {
                context.add_diagnostic(
                    DiagnosticCode::ReturnTypeMismatch,
                    expr.get_range(),
                    t!(
                        "expected `%{source}` but found `%{found}`. %{reason}",
                        source = humanize_type(semantic_model, expected),
                        found = humanize_type(semantic_model, &constraint),
                        reason = ""
                    ),
                );
            }
            continue;
        }
        let actual = generic_constraint_type(semantic_model, &raw).unwrap_or(raw);
        if matches!(actual, LuaType::Unknown | LuaType::Any | LuaType::Never)
            || return_compatible(semantic_model, &actual, expected, expr.get_range())
        {
            continue;
        }
        context.add_diagnostic(
            DiagnosticCode::ReturnTypeMismatch,
            expr.get_range(),
            t!(
                "expected `%{source}` but found `%{found}`. %{reason}",
                source = humanize_type(semantic_model, expected),
                found = humanize_type(semantic_model, &actual),
                reason = ""
            ),
        );
    }
}

/// Return type compatibility:
/// - every component of a source union must be compatible with target;
/// - any component satisfying target union is enough;
/// - generic instances (`B<string>`) match alias/class through the parent type chain.
fn return_compatible(
    semantic_model: &SemanticModel<'_>,
    source: &LuaType,
    target: &LuaType,
    range: rowan::TextRange,
) -> bool {
    if source == target {
        return true;
    }
    if check_type_at(semantic_model, source, target, range, false).is_ok() {
        return true;
    }
    if let LuaType::Union(target_union) = target {
        return target_union
            .into_vec()
            .iter()
            .any(|component| return_compatible(semantic_model, source, component, range));
    }
    generic_extends_named(semantic_model, source, target)
}

/// `---@class B<T>: A` (where `A` is an alias/class): assign a `B<string>` instance to `A`.
fn generic_extends_named(
    semantic_model: &SemanticModel<'_>,
    source: &LuaType,
    target: &LuaType,
) -> bool {
    let LuaType::Generic(generic) = source else {
        return false;
    };
    let expected_id = match target {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return false,
    };
    let Some(source_def) = semantic_model.type_def_of(&generic.get_base_type_id()) else {
        return false;
    };
    let Some(target_def) = semantic_model.type_def_of(expected_id) else {
        return false;
    };
    let mut visited = Vec::new();
    def_extends(semantic_model, &source_def, &target_def, &mut visited)
}

fn def_extends(
    semantic_model: &SemanticModel<'_>,
    source: &TypeDef,
    target: &TypeDef,
    visited: &mut Vec<SemanticId>,
) -> bool {
    if visited.contains(&source.id) {
        return false;
    }
    visited.push(source.id.clone());
    if source.id == target.id {
        return true;
    }
    for super_def in semantic_model.base_type_defs(&source.type_decl_id()) {
        if def_extends(semantic_model, &super_def, target, visited) {
            return true;
        }
    }
    false
}

/// Uninstantiated `Ref("T")` / `Ref("E")`: the signature doc declares a generic parameter with the same name.
fn is_unresolved_generic_ref(
    semantic_model: &SemanticModel<'_>,
    ty: &LuaType,
    docs: &SignatureDoc,
) -> bool {
    let (LuaType::Ref(id) | LuaType::Def(id)) = ty else {
        return false;
    };
    if semantic_model.type_def_of(id).is_some() {
        return false;
    }
    docs.generic_params
        .iter()
        .any(|param| param.name.as_str() == id.get_name())
}

/// `Ref("T")` when the signature declares `---@generic T: Animal` -> project to the constraint `Animal`.
fn generic_constraint_type(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<LuaType> {
    let (LuaType::Ref(id) | LuaType::Def(id)) = ty else {
        return None;
    };
    if semantic_model.type_def_of(id).is_some() {
        return None;
    }
    let name = id.get_name();
    for (_signature, param) in semantic_model.file_facts()?.generic_param_constraints(name) {
        let Some(constraint) = param.constraint else {
            continue;
        };
        let constraint_ty = semantic_model.doc_type_lua(constraint);
        if !matches!(constraint_ty, LuaType::Unknown) {
            return Some(constraint_ty);
        }
    }
    None
}
