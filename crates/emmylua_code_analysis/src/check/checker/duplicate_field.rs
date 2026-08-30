//! # duplicate_field — duplicate member definitions in types / objects
//!
//! M0: group this file's members by owner; duplicate `@field` on a TypeDef -> `DuplicateDocField`;
//! duplicate runtime members on a Decl (same name defined multiple times) -> `DuplicateSetField`.

use std::collections::HashMap;

use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaSyntaxNode, LuaVarExpr};

use crate::DiagnosticCode;
use crate::LuaType;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct DuplicateFieldChecker;

impl Checker for DuplicateFieldChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::DuplicateDocField,
        DiagnosticCode::DuplicateSetField,
    ];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(facts) = semantic_model.file_facts() else {
            return;
        };
        if let Some(tree) = semantic_model.syntax_tree() {
            let root = tree.get_red_root();
            check_cross_file_member_assign(context, semantic_model, &root);
        }
        let mut by_owner: HashMap<
            &crate::salsa_builder::def::SemanticId,
            Vec<&crate::salsa_builder::def::Member>,
        > = HashMap::new();
        for member in &facts.members {
            by_owner.entry(&member.owner).or_default().push(member);
        }
        for (owner, members) in by_owner {
            // Group by key.
            let mut by_key: HashMap<String, Vec<&&crate::salsa_builder::def::Member>> =
                HashMap::new();
            for member in &members {
                by_key.entry(member.key.to_path()).or_default().push(member);
            }
            let is_type_def = matches!(owner, crate::salsa_builder::def::SemanticId::TypeDef(_));
            for (name, dupes) in by_key {
                if dupes.len() <= 1 {
                    continue;
                }
                // `@field a fun()` may be declared repeatedly (function overloads); other duplicate @field entries report DuplicateDocField.
                if is_type_def
                    && dupes.iter().all(|member| {
                        semantic_model.type_of_member(&member.id).is_some_and(|ty| {
                            matches!(ty, LuaType::DocFunction(_) | LuaType::Signature(_))
                        })
                    })
                {
                    continue;
                }
                let code = if is_type_def {
                    DiagnosticCode::DuplicateDocField
                } else {
                    DiagnosticCode::DuplicateSetField
                };
                for member in dupes {
                    if let Some(range) = member.id.member_key_range() {
                        context.add_diagnostic(
                            code,
                            range,
                            t!("Duplicate field `%{name}`.", name = name),
                        );
                    }
                }
            }
        }
    }
}

/// `local A = require("mod"); A.execute = function() end`:
/// the assignment target resolves to an existing method member in the module file -> cross-file duplicate definition DuplicateSetField.
fn check_cross_file_member_assign(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    root: &LuaSyntaxNode,
) {
    let current_file = semantic_model.file_id();
    for assign_stat in root.descendants().filter_map(LuaAssignStat::cast) {
        let (vars, exprs) = assign_stat.get_var_and_expr_list();
        for (index, var) in vars.iter().enumerate() {
            let LuaVarExpr::IndexExpr(index_expr) = var else {
                continue;
            };
            let Some(prefix) = index_expr.get_prefix_expr() else {
                continue;
            };
            let Some(resolved) = semantic_model.resolve_member(index_expr) else {
                continue;
            };
            // The target must be a require'd module export (cross-file definition).
            let module_file = required_module_file(semantic_model, &prefix)
                .or_else(|| resolved.file_id.filter(|file| *file != current_file));
            let Some(module_file) = module_file else {
                continue;
            };
            if module_file == current_file {
                continue;
            }
            // The module file already has a same-named runtime function member -> this assignment is a duplicate implementation.
            let Some(module_facts) = semantic_model.file_facts_of(module_file) else {
                continue;
            };
            if !module_facts.members.iter().any(|member| {
                member.key.name() == Some(resolved.name.as_str()) && member.value_syntax.is_some()
            }) {
                continue;
            }
            // Only function/closure right-hand sides count as "reimplementing an existing method"; plain field writes are not reported.
            let Some(rhs) = exprs.get(index) else {
                continue;
            };
            let rhs_ty = semantic_model.type_of_expr(rhs.get_syntax_id());
            if !matches!(
                rhs_ty,
                LuaType::DocFunction(_) | LuaType::Signature(_) | LuaType::Function
            ) {
                continue;
            }
            let Some(key_range) = index_expr.get_index_key().and_then(|key| key.get_range()) else {
                continue;
            };
            context.add_diagnostic(
                DiagnosticCode::DuplicateSetField,
                key_range,
                t!("Duplicate field `%{name}`.", name = resolved.name),
            );
        }
    }
}

/// require local variable -> module file (`local A = require("mod")`).
fn required_module_file(
    semantic_model: &SemanticModel<'_>,
    prefix: &emmylua_parser::LuaExpr,
) -> Option<crate::FileId> {
    let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix else {
        return None;
    };
    let decl = semantic_model.resolve_name(name_expr.get_position())?;
    let facts = semantic_model.file_facts()?;
    let decl = facts.decl_by_id(&decl)?;
    let call_syntax = decl.value_expr_syntax?;
    let tree = semantic_model.syntax_tree()?;
    let node = call_syntax.to_node_from_root(&tree.get_red_root())?;
    let call = emmylua_parser::LuaCallExpr::cast(node)?;
    let arg = call.get_args_list()?.get_args().next()?;
    let ty = semantic_model.type_of_expr(arg.get_syntax_id());
    let module_name = match ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => s.as_ref().to_string(),
        _ => return None,
    };
    semantic_model.module_file_of(&module_name)
}
