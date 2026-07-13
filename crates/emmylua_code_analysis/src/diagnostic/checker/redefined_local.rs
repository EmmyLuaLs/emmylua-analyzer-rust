//! Redefined local checker — salsa-native.

use hashbrown::{HashMap, HashSet};

use crate::DiagnosticCode;
use crate::compilation::{
    SalsaDeclId, SalsaDeclKindSummary, SalsaDeclTreeSummary, SalsaScopeChildSummary,
    SalsaScopeKindSummary, SalsaScopeSummary,
};
use crate::semantic_model::SemanticModel;
use rowan::TextRange;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let Some(decl_tree) = model.decl_tree() else {
        return;
    };
    let Some(root_scope) = find_root_scope(&decl_tree) else {
        return;
    };

    let mut diagnostics = HashSet::new();
    let mut root_locals = HashMap::new();
    check_scope(&decl_tree, root_scope, &mut root_locals, &mut diagnostics);

    for decl_id in &diagnostics {
        let Some(decl) = decl_tree.decls.iter().find(|d| d.id == *decl_id) else {
            continue;
        };
        context.add_diagnostic(
            DiagnosticCode::RedefinedLocal,
            TextRange::new(decl.start_offset, decl.end_offset),
            t!("Redefined local variable `%{name}`", name = decl.name).to_string(),
            None,
        );
    }
}

fn find_root_scope(tree: &SalsaDeclTreeSummary) -> Option<&SalsaScopeSummary> {
    tree.scopes.iter().find(|s| s.parent.is_none())
}

fn check_scope(
    tree: &SalsaDeclTreeSummary,
    scope: &SalsaScopeSummary,
    parent_locals: &mut HashMap<String, SalsaDeclId>,
    diagnostics: &mut HashSet<SalsaDeclId>,
) {
    let should_propagate = matches!(
        scope.kind,
        SalsaScopeKindSummary::FuncStat
            | SalsaScopeKindSummary::LocalOrAssignStat
            | SalsaScopeKindSummary::Repeat
            | SalsaScopeKindSummary::MethodStat
    );
    let mut current = parent_locals.clone();

    for child in &scope.children {
        let decl_id = match child {
            SalsaScopeChildSummary::Decl(id) => *id,
            _ => continue,
        };
        let Some(decl) = tree.decls.iter().find(|d| d.id == decl_id) else {
            continue;
        };
        // Only check local variables (not params, not implicit self)
        if !matches!(decl.kind, SalsaDeclKindSummary::Local { .. }) {
            continue;
        }
        let name = decl.name.to_string();
        if name == "..." || name.starts_with('_') {
            continue;
        }

        if let Some(old_id) = current.get(&name) {
            if !is_param_shadow_ok(tree, decl_id, *old_id) {
                diagnostics.insert(decl_id);
            }
        }
        current.insert(name, decl_id);
    }

    // Recurse into child scopes
    for child in &scope.children {
        if let SalsaScopeChildSummary::Scope(sid) = child {
            if let Some(child_scope) = tree.scopes.iter().find(|s| s.id == *sid) {
                check_scope(tree, child_scope, &mut current, diagnostics);
            }
        }
    }

    if should_propagate {
        for (name, id) in current {
            parent_locals.insert(name, id);
        }
    }
}

/// `local a = function(a)` — param shadowing its own closure is OK.
fn is_param_shadow_ok(
    tree: &SalsaDeclTreeSummary,
    current_id: SalsaDeclId,
    old_id: SalsaDeclId,
) -> bool {
    let current_decl = tree.decls.iter().find(|d| d.id == current_id);
    let old_decl = tree.decls.iter().find(|d| d.id == old_id);
    let (Some(current), Some(old)) = (current_decl, old_decl) else {
        return false;
    };
    // Old must NOT be a param, current must BE a param
    if matches!(old.kind, SalsaDeclKindSummary::Param { .. })
        || !matches!(current.kind, SalsaDeclKindSummary::Param { .. })
    {
        return false;
    }
    // Old's value expression must be a closure
    let Some(val_syntax) = &old.value_expr_syntax_id else {
        return false;
    };
    // Closure position must match param's signature offset
    if let SalsaDeclKindSummary::Param {
        signature_offset, ..
    } = &current.kind
    {
        if val_syntax.start_offset == *signature_offset {
            return true;
        }
    }
    false
}
