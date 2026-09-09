//! redefined_local: repeated same-name local declarations in the same scope.

use std::collections::HashMap;

use emmylua_parser::LuaAstNode;
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::semantic_db::def::{DeclKind, ScopeChild, ScopeKind, SemanticId};
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct RedefinedLocalChecker;

impl Checker for RedefinedLocalChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::RedefinedLocal];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(facts) = semantic_model.file_facts() else {
            return;
        };
        let mut parent_locals: HashMap<String, SemanticId> = HashMap::new();
        check_scope(context, semantic_model, facts, 0, &mut parent_locals);
    }
}

fn check_scope(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    facts: &crate::FileFacts,
    scope_idx: u32,
    parent_locals: &mut HashMap<String, SemanticId>,
) {
    let Some(scope) = facts.scopes.get(scope_idx as usize) else {
        return;
    };
    let should_merge = matches!(
        scope.kind,
        ScopeKind::FuncStat | ScopeKind::LocalStat | ScopeKind::AssignStat | ScopeKind::Repeat
    );

    let mut current_locals = parent_locals.clone();
    for child in &scope.children {
        if let ScopeChild::Decl(decl_id) = child
            && let Some(decl) = facts.decl_by_id(decl_id)
            && decl.kind.is_local()
            && decl.name != "..."
            && !decl.name.starts_with('_')
        {
            let name = decl.name.to_string();
            if current_locals.contains_key(&name) {
                // issue 481: `local a = function(a)` - parameter a does not conflict with the local a of its own initializing closure;
                // `local a; a = function(a)` - the local a has no initializing closure, so report the error.
                let conflicts_with_param_of_own_closure = decl.kind == DeclKind::Param
                    && current_locals.get(&name).is_some_and(|old_id| {
                        facts.decl_by_id(old_id).is_some_and(|old_decl| {
                            old_decl.value_expr_syntax
                                == enclosing_closure_of_param(semantic_model, decl.name_range)
                        })
                    });
                if !conflicts_with_param_of_own_closure {
                    context.add_diagnostic(
                        DiagnosticCode::RedefinedLocal,
                        decl.name_range,
                        t!("Redefined local variable `%{name}`", name = name),
                    );
                }
            }
            current_locals.insert(name, decl.id.clone());
        }
    }

    for child in &scope.children {
        if let ScopeChild::Scope(child_idx) = child {
            check_scope(
                context,
                semantic_model,
                facts,
                *child_idx,
                &mut current_locals,
            );
        }
    }

    if should_merge {
        for (name, decl_id) in current_locals {
            parent_locals.insert(name, decl_id);
        }
    }
}

/// The `LuaSyntaxId` of the closure containing the parameter name.
fn enclosing_closure_of_param(
    semantic_model: &SemanticModel<'_>,
    param_range: TextRange,
) -> Option<emmylua_parser::LuaSyntaxId> {
    semantic_model
        .enclosing_closure_at(param_range.start())
        .map(|closure| closure.get_syntax_id())
}
