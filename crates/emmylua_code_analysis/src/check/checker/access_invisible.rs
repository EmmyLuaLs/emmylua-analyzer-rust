//! # access_invisible — access to `@private/@protected/@package` members
//!
//! After a name/member use is resolved to a declaration, `is_visible` checks it.
//! M0 visibility rule: same file (Private/Protected/Package); cross-file access reports `AccessInvisible`.

use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaExpr};

use crate::DiagnosticCode;
use crate::LuaType;
use crate::salsa_builder::def::SemanticId;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct AccessInvisibleChecker;

impl Checker for AccessInvisibleChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::AccessInvisible];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for node in root.descendants().filter_map(LuaAst::cast) {
            match node {
                LuaAst::LuaNameExpr(name_expr) => {
                    let Some(decl) = semantic_model
                        .find_decl(rowan::NodeOrToken::Node(name_expr.syntax().clone()))
                    else {
                        continue;
                    };
                    let Some(name_token) = name_expr.get_name_token() else {
                        continue;
                    };
                    if !semantic_model.is_visible(
                        rowan::NodeOrToken::Token(name_token.syntax().clone()),
                        &decl,
                    ) {
                        context.add_diagnostic(
                            DiagnosticCode::AccessInvisible,
                            name_token.get_range(),
                            t!("The property is not accessible from this scope."),
                        );
                    }
                }
                LuaAst::LuaIndexExpr(index_expr) => {
                    let Some(resolved) = semantic_model.resolve_member(&index_expr) else {
                        continue;
                    };
                    let Some(member_id) = resolved.member_id else {
                        continue;
                    };
                    let Some(index_token) = index_expr.get_index_name_token() else {
                        continue;
                    };
                    // Re-export bridging: prefer visibility of the same-name @field in the declaration file
                    // (runtime implementation members default to Public and must not mask `@field private`).
                    let visibility_id = resolved
                        .file_id
                        .and_then(|file_id| {
                            let facts = semantic_model.file_facts_of(file_id)?;
                            facts
                                .members
                                .iter()
                                .find(|member| {
                                    member.key.name() == Some(resolved.name.as_str())
                                        && member.visibility
                                            != emmylua_parser::VisibilityKind::Public
                                })
                                .map(|member| member.id.clone())
                        })
                        .or_else(|| {
                            let prefix_expr = index_expr.get_prefix_expr()?;
                            module_private_member(semantic_model, &prefix_expr, &resolved.name)
                        })
                        .unwrap_or(member_id);
                    if !semantic_model.is_visible(
                        rowan::NodeOrToken::Token(index_token.clone()),
                        &visibility_id,
                    ) {
                        context.add_diagnostic(
                            DiagnosticCode::AccessInvisible,
                            index_token.text_range(),
                            t!("The property is not accessible from this scope."),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

/// `local Log = require("mod")` → same-name non-public member in the module file (bridged visibility).
fn module_private_member(
    semantic_model: &SemanticModel<'_>,
    prefix: &LuaExpr,
    name: &str,
) -> Option<SemanticId> {
    let LuaExpr::NameExpr(name_expr) = prefix else {
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
    let module_name = match semantic_model.type_of_expr(arg.get_syntax_id()) {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => s.as_ref().to_string(),
        _ => return None,
    };
    let module_file = semantic_model.module_file_of(&module_name)?;
    let module_facts = semantic_model.file_facts_of(module_file)?;
    module_facts
        .members
        .iter()
        .find(|member| {
            member.key.name() == Some(name)
                && member.visibility != emmylua_parser::VisibilityKind::Public
        })
        .map(|member| member.id.clone())
}
