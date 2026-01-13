use std::collections::HashSet;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaClosureExpr, LuaComment, LuaDocTagReturnCast, LuaNameExpr,
};
use rowan::TextRange;

use crate::{DiagnosticCode, LuaSignatureId, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct UndefinedGlobalChecker;

impl Checker for UndefinedGlobalChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UndefinedGlobal];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        let mut use_range_set = HashSet::new();
        calc_name_expr_ref(semantic_model, &mut use_range_set);
        for name_expr in root.descendants::<LuaNameExpr>() {
            check_name_expr(context, semantic_model, &mut use_range_set, name_expr);
        }
    }
}

fn calc_name_expr_ref(
    semantic_model: &SemanticModel,
    use_range_set: &mut HashSet<TextRange>,
) -> Option<()> {
    let file_id = semantic_model.get_file_id();
    let db = semantic_model.get_db();
    let refs_index = db.get_reference_index().get_local_reference(&file_id)?;
    for decl_refs in refs_index.get_decl_references_map().values() {
        for decl_ref in &decl_refs.cells {
            use_range_set.insert(decl_ref.range);
        }
    }

    None
}

fn check_name_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    use_range_set: &mut HashSet<TextRange>,
    name_expr: LuaNameExpr,
) -> Option<()> {
    let name_range = name_expr.get_range();
    if use_range_set.contains(&name_range) {
        return Some(());
    }

    let name_text = name_expr.get_name_text()?;
    if name_text == "_" {
        return Some(());
    }

    if semantic_model
        .get_db()
        .get_global_index()
        .is_exist_global_decl(&name_text)
    {
        return Some(());
    }

    if context
        .config
        .global_disable_set
        .contains(name_text.as_str())
    {
        return Some(());
    }

    if context
        .config
        .global_disable_glob
        .iter()
        .any(|re| re.is_match(&name_text))
    {
        return Some(());
    }

    if name_text == "self" && check_self_name(semantic_model, name_expr).is_some() {
        return Some(());
    }

    context.add_diagnostic(
        DiagnosticCode::UndefinedGlobal,
        name_range,
        t!("undefined global variable: %{name}", name = name_text).to_string(),
        None,
    );

    Some(())
}

fn check_self_name(semantic_model: &SemanticModel, name_expr: LuaNameExpr) -> Option<()> {
    // Check if self is in a method context (regular Lua code)
    let closure_expr = name_expr.ancestors::<LuaClosureExpr>();
    for closure_expr in closure_expr {
        let signature_id =
            LuaSignatureId::from_closure(semantic_model.get_file_id(), &closure_expr);
        let signature = semantic_model
            .get_db()
            .get_signature_index()
            .get(&signature_id)?;
        if signature.is_method(semantic_model, None) {
            return Some(());
        }

        // Check if self is a parameter of this function (from @param self)
        if signature.find_param_idx("self").is_some() {
            return Some(());
        }
    }

    // Check if self is in @return_cast tag
    // The name_expr might be inside a doc comment, not inside actual Lua code
    for ancestor in name_expr.syntax().ancestors() {
        if let Some(return_cast_tag) = LuaDocTagReturnCast::cast(ancestor.clone()) {
            // Find the LuaComment that contains this tag
            for comment_ancestor in return_cast_tag.syntax().ancestors() {
                if let Some(comment) = LuaComment::cast(comment_ancestor) {
                    // Get the owner (function) of this comment
                    if let Some(owner) = comment.get_owner() {
                        if let LuaAst::LuaClosureExpr(closure) = owner {
                            let sig_id = LuaSignatureId::from_closure(
                                semantic_model.get_file_id(),
                                &closure,
                            );
                            if let Some(sig) =
                                semantic_model.get_db().get_signature_index().get(&sig_id)
                            {
                                // Check if the owner function is a method
                                if sig.is_method(semantic_model, None) {
                                    return Some(());
                                }
                                // Check if self is a parameter
                                if sig.find_param_idx("self").is_some() {
                                    return Some(());
                                }
                            }
                        }
                    }
                    break;
                }
            }
            break;
        }
    }

    None
}
