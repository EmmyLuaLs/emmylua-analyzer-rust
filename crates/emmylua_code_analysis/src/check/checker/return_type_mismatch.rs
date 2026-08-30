//! # return_type_mismatch: actual return types in the function body do not match the `---@return` annotation
//!
//! Only functions **with `@return` annotations** are checked: signature annotation types vs function-body return expression types.
//! Supports multiple return values (union components checked individually), generic parameters (uninstantiated `Ref("T")` skipped),
//! class inheritance alias function types, and `return expr ---@as type` inline assertions.

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaReturnStat};

use crate::LuaType;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::type_check::is_compatible;
use crate::{DiagnosticCode, SemanticId, SignatureDoc, TypeDef, TypeScope};

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
        let mut raw =
            semantic_model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start());
        // `return dd() ---@as integer`: the inline type assertion overrides the expression type.
        if let Some(as_type) = inline_as_type(semantic_model, ret, &expr) {
            raw = as_type;
        }
        // Uninstantiated function generic argument `Ref("T")`: check against the constraint when present, otherwise skip.
        if is_unresolved_generic_ref(semantic_model, &raw, docs) {
            if let Some(constraint) = generic_constraint_type(semantic_model, &raw)
                && !return_compatible(semantic_model, &constraint, expected)
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
            || return_compatible(semantic_model, &actual, expected)
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
) -> bool {
    if source == target {
        return true;
    }
    if let LuaType::Union(source_union) = source {
        // Historical compatibility: ordinary union returns still use "any member assignable" (some flow guards do not yet fully narrow impossible components).
        // But when the return value contains a function component and the target is a named class, all components must be checked; otherwise the function could be silently treated as a class instance.
        let components = source_union.into_vec();
        let target_is_named_class = matches!(target, LuaType::Ref(_) | LuaType::Def(_));
        let has_function = components.iter().any(|component| {
            matches!(
                component,
                LuaType::Function | LuaType::DocFunction(_) | LuaType::Signature(_)
            )
        });
        let require_all = target_is_named_class && has_function;
        return if require_all {
            components
                .iter()
                .all(|component| return_compatible(semantic_model, component, target))
        } else {
            components
                .iter()
                .any(|component| return_compatible(semantic_model, component, target))
        };
    }
    // Integer literal -> number.
    if matches!(
        source,
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)
    ) && matches!(target, LuaType::Number)
    {
        return true;
    }
    // `table<K,V>` -> `V[]`.
    if let (LuaType::Generic(generic), LuaType::Array(array)) = (source, target)
        && generic.get_base_type_id().get_name() == "table"
        && let Some(value_ty) = generic.get_params().get(1)
        && return_compatible(semantic_model, value_ty, array.get_base())
    {
        return true;
    }
    if is_compatible(semantic_model, source, target) {
        return true;
    }
    if let LuaType::Union(target_union) = target {
        return target_union
            .into_vec()
            .iter()
            .any(|component| return_compatible(semantic_model, source, component));
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
    for super_name in &source.super_names {
        let super_def = semantic_model
            .resolve_type_def_in(source.file_id, super_name.as_str())
            .or_else(|| {
                semantic_model
                    .type_defs_in_scope(TypeScope::Global, super_name)
                    .into_iter()
                    .next()
            });
        if let Some(super_def) = super_def
            && def_extends(semantic_model, &super_def, target, visited)
        {
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

/// Inline type for `return expr ---@as integer` / `--[[@as integer]]`.
fn inline_as_type(
    semantic_model: &SemanticModel<'_>,
    ret: &LuaReturnStat,
    expr: &emmylua_parser::LuaExpr,
) -> Option<LuaType> {
    let expr_siblings = expr.syntax().siblings_with_tokens(rowan::Direction::Next);
    let ret_siblings = ret.syntax().siblings_with_tokens(rowan::Direction::Next);
    for sibling in expr_siblings.chain(ret_siblings) {
        if let Some(ty) = as_type_from_token(semantic_model, sibling.as_token()) {
            return Some(ty);
        }
    }
    // The parser may not attach the end-of-line comment as a sibling of ReturnStat; fall back to searching by position inside the closure.
    if let Some(closure) = ret.ancestors::<LuaClosureExpr>().next() {
        let mut prev_text = String::new();
        let mut pending_as = false;
        for item in closure.syntax().descendants_with_tokens() {
            let Some(token) = item.into_token() else {
                continue;
            };
            let text = token.text();
            if pending_as {
                let name: String = text
                    .trim()
                    .trim_start_matches(['[', '-'])
                    .chars()
                    .take_while(|c| {
                        c.is_alphanumeric() || matches!(c, '.' | '_' | '?' | '|' | '<' | '>')
                    })
                    .collect();
                let name = name.trim_end_matches([']', '-']);
                if !name.is_empty() {
                    return Some(semantic_model.type_from_name(name));
                }
                if !text.trim().is_empty() {
                    pending_as = false;
                }
            }
            if prev_text.ends_with("---@") && text == "as" {
                pending_as = true;
            }
            prev_text = text.to_string();
            if token.text_range().start() >= expr.get_range().end() {
                if let Some(ty) = as_type_from_token(semantic_model, Some(&token)) {
                    return Some(ty);
                }
            }
        }
    }
    None
}

fn as_type_from_token(
    semantic_model: &SemanticModel<'_>,
    token: Option<&emmylua_parser::LuaSyntaxToken>,
) -> Option<LuaType> {
    let token = token?;
    let text = token.text();
    let mark = text.find("@as")?;
    let rest = &text[mark + 3..];
    let name: String = rest
        .trim_start_matches(|c: char| c.is_whitespace() || c == '[' || c == '-')
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '?' | '|' | '<' | '>'))
        .collect();
    let name = name.trim_end_matches([']', '-']);
    if name.is_empty() {
        return None;
    }
    Some(semantic_model.type_from_name(name))
}
