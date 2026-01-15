use emmylua_parser::LuaChunk;
use rowan::TextRange;

use crate::{DiagnosticCode, LuaDecl, LuaReferenceIndex, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct UnusedChecker;

impl Checker for UnusedChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::Unused];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let file_id = semantic_model.get_file_id();
        let Some(decl_tree) = semantic_model
            .get_db()
            .get_decl_index()
            .get_decl_tree(&file_id)
        else {
            return;
        };

        let root = semantic_model.get_root();
        let ref_index = semantic_model.get_db().get_reference_index();
        for (_, decl) in decl_tree.get_decls().iter() {
            if decl.is_global() || decl.is_param() && decl.get_name() == "..." {
                continue;
            }

            if let Err(result) = get_unused_check_result(ref_index, decl, root) {
                let name = decl.get_name();
                if name.starts_with('_') {
                    continue;
                }
                match result {
                    UnusedCheckResult::Unused(range) => {
                        context.add_diagnostic(
                        DiagnosticCode::Unused,
                        range,
                        t!(
                            "%{name} is never used, if this is intentional, prefix it with an underscore: _%{name}",
                            name = name
                        ).to_string(),
                        None)
                    }
                    // UnusedCheckResult::AssignedButNotRead(range) => {
                    //     context.add_diagnostic(
                    //         DiagnosticCode::Unused,
                    //         range,
                    //         t!(
                    //             "Variable '%{name}' is assigned a value but this value is never read, use _%{name} to indicate this is intentional",
                    //             name = name
                    //         ).to_string(),
                    //         None)
                    // }
                    UnusedCheckResult::UnusedSelf(range) => {
                        context.add_diagnostic(
                            DiagnosticCode::Unused,
                            range,
                            t!(
                                "Implicit self is never used, if this is intentional, please use '.' instead of ':' to define the method",
                            ).to_string(),
                            None,
                        );
                    }
                }
            }
        }
    }
}

enum UnusedCheckResult {
    Unused(TextRange),
    // AssignedButNotRead(TextRange),
    UnusedSelf(TextRange),
}

fn get_unused_check_result(
    ref_index: &LuaReferenceIndex,
    decl: &LuaDecl,
    _root: &LuaChunk,
) -> Result<(), UnusedCheckResult> {
    let decl_range = decl.get_range();
    let file_id = decl.get_file_id();
    let decl_ref = match ref_index.get_decl_references(&file_id, &decl.get_id()) {
        Some(decl_ref) => decl_ref,
        None => {
            if decl.is_implicit_self() {
                return Err(UnusedCheckResult::UnusedSelf(decl_range));
            }
            return Err(UnusedCheckResult::Unused(decl_range));
        }
    };

    if decl_ref.cells.is_empty() {
        return Err(UnusedCheckResult::Unused(decl_range));
    }

    // if decl_ref.mutable {
    //     let last_ref_cell = decl_ref
    //         .cells
    //         .last()
    //         .ok_or(UnusedCheckResult::Unused(decl_range))?;

    //     if last_ref_cell.is_write
    //         && let Some(result) =
    //             check_last_mutable_is_read(decl_range.start(), decl_ref, last_ref_cell.range, root)
    //     {
    //         return Err(result);
    //     }
    // }

    Ok(())
}

// remove for future implement
// fn check_last_mutable_is_read(
//     decl_position: TextSize,
//     decl_ref: &DeclReference,
//     range: TextRange,
//     root: &LuaChunk,
// ) -> Option<UnusedCheckResult> {
//     let syntax_id = LuaSyntaxId::new(LuaSyntaxKind::NameExpr.into(), range);
//     let node = LuaNameExpr::cast(syntax_id.to_node_from_root(root.syntax())?)?;

//     for ancestor_node in node.ancestors::<LuaAst>() {
//         // decl's parent
//         if ancestor_node.syntax().text_range().contains(decl_position) {
//             return Some(UnusedCheckResult::AssignedButNotRead(range));
//         }

//         if let Some(loop_stat) = LuaLoopStat::cast(ancestor_node.syntax().clone()) {
//             // in a loop stat
//             let loop_range = loop_stat.syntax().text_range();
//             for ref_cell in decl_ref.cells.iter() {
//                 if !ref_cell.is_write && loop_range.contains(ref_cell.range.start()) {
//                     return None;
//                 }
//             }
//         } else if ancestor_node.syntax().kind() == LuaSyntaxKind::ClosureExpr.into() {
//             return None;
//         }
//     }

//     // not in a loop stat
//     Some(UnusedCheckResult::AssignedButNotRead(range))
// }
