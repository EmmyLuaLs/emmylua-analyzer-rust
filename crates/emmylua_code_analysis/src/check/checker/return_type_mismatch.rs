//! Check a function body's return values against its declared signature.
//! Undocumented closure fields use the class field's function signature.

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaExpr, LuaReturnStat};

use crate::semantic_model::{
    SemanticModel,
    infer::vm::{bind_signature_generics, replace_self_type},
    render::humanize_type,
    result_pack::{ResultPack, can_expand_results},
    type_check::check_type_at,
};
use crate::{DiagnosticCode, GenericTpl, LuaType, LuaTypeNode, SemanticId, SignatureDoc, TypeDef};

use super::{
    CheckContext, Checker,
    check_return_count::{declared_return_type, own_return_stats},
};

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
            let Some((expected, templates)) = declared_return_type(semantic_model, signature)
            else {
                continue;
            };
            let docs = signature.docs.as_deref().unwrap_or(&default_docs);

            let mut expected = ResultPack::from_type(&expected, Some(semantic_model));
            if let Some(owner) = semantic_model.method_owner_type(signature.closure_syntax) {
                for ty in expected.values_mut() {
                    *ty = replace_self_type(ty, &owner);
                }
            }
            for ret in own_return_stats(&closure) {
                check_return_slots(context, semantic_model, &ret, &expected, docs, &templates);
            }
        }
    }
}

/// Only the final call or `...` expands into multiple return values in Lua.
fn check_return_slots(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    ret: &LuaReturnStat,
    expected: &ResultPack,
    docs: &SignatureDoc,
    templates: &[GenericTpl],
) {
    let (expected_fixed, expected_max) = expected.bounds();
    let expressions = ret.get_expr_list().collect::<Vec<_>>();
    let checked_exprs = expressions
        .iter()
        .take(expected_max.unwrap_or(expressions.len()));
    for (index, expr) in checked_exprs.enumerate() {
        let range = expr.get_range();
        let expands = index + 1 == expressions.len() && can_expand_results(expr);
        let actual = ResultPack::from_expr(semantic_model, expr);

        let slots = if expands {
            let (source_fixed, source_max) = actual.bounds();
            let expected_prefix = expected_fixed.saturating_sub(index);
            let expected_max = expected_max.map(|max| max.saturating_sub(index));
            match (source_max, expected_max) {
                (Some(source), Some(target)) => source.min(target),
                (Some(source), None) => source,
                (None, Some(target)) => target,
                // Compare both fixed prefixes and one slot from each repeating tail.
                (None, None) => source_fixed.max(expected_prefix) + 1,
            }
        } else {
            1
        };

        for offset in 0..slots {
            let slot = index + offset;
            let Some(target) = expected.slot(slot, true) else {
                break;
            };
            // An omitted result in a repeating tail supplies no value to check.
            let pad_missing = !expands || slot < expected_fixed;
            // Lua substitutes nil when a single-value expression supplies no result.
            let Some(actual) = actual
                .slot(offset, pad_missing)
                .or((!expands).then_some(LuaType::Nil))
            else {
                break;
            };
            // Try the function's generic identities before falling back to constraints.
            if !templates.is_empty()
                && actual.any_type(|ty| is_unresolved_generic_ref(semantic_model, ty, docs))
            {
                let bound = bind_signature_generics(&actual, templates);
                if return_compatible(semantic_model, &bound, &target, range) {
                    continue;
                }
            }
            check_return_value(context, semantic_model, actual, &target, expr, docs);
        }
    }
}

fn check_return_value(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    raw: LuaType,
    expected: &LuaType,
    expr: &LuaExpr,
    docs: &SignatureDoc,
) {
    let range = expr.get_range();
    // An uninstantiated return parameter can only be checked through its constraint.
    let actual = match semantic_model.generic_constraint_at(&raw, expr) {
        Some(constraint) => constraint,
        None if is_unresolved_generic_ref(semantic_model, &raw, docs) => return,
        None => raw,
    };
    if matches!(actual, LuaType::Unknown | LuaType::Any | LuaType::Never)
        || return_compatible(semantic_model, &actual, expected, range)
    {
        return;
    }
    context.add_diagnostic(
        DiagnosticCode::ReturnTypeMismatch,
        range,
        t!(
            "expected `%{source}` but found `%{found}`. %{reason}",
            source = humanize_type(semantic_model, expected),
            found = humanize_type(semantic_model, &actual),
            reason = ""
        ),
    );
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
    docs.generic_params
        .iter()
        .any(|param| param.name == id.get_name())
        && semantic_model.type_def_of(id).is_none()
}
