//! Cast type mismatch — pure salsa.

use emmylua_parser::{LuaAst, LuaAstNode, LuaDocTagCast};
use hashbrown::HashSet;

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, DocTypeInferContext, LuaType, get_real_type, infer_doc_type};

use super::{DiagnosticContext, humanize_lint_type_salsa};

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    for node in model.get_root().descendants::<LuaAst>() {
        if let LuaAst::LuaDocTagCast(cast) = node {
            check_cast(context, model, &cast);
        }
    }
}

// fn expand_type(db: &crate::DbIndex, typ: &LuaType) -> Option<LuaType> {
//     let mut visited = HashSet::new();
//     expand_type_recursive(db, typ, &mut visited)
// }

// fn expand_type_recursive(
//     db: &crate::DbIndex,
//     typ: &LuaType,
//     visited: &mut HashSet<LuaType>,
// ) -> Option<LuaType> {
//     if visited.contains(typ) {
//         return Some(typ.clone());
//     }
//     visited.insert(typ.clone());
//     match get_real_type(db, typ).unwrap_or(typ) {
//         LuaType::Ref(id) | LuaType::Def(id) => {
//             let decl = db.get_type_index().get_type_decl(id)?;
//             if decl.is_enum()
//                 && let Some(_et) = decl.get_enum_field_type(db)
//             {
//                 return Some(LuaType::Ref(id.clone()));
//             }
//             Some(LuaType::Ref(id.clone()))
//         }
//         other => Some(other.clone()),
//     }
// }

fn check_cast(context: &mut DiagnosticContext, model: &SemanticModel, cast: &LuaDocTagCast) {
    let Some(_key_expr) = cast.get_key_expr() else {
        return;
    };
    // let Ok(typ) = model.infer_expr(key_expr) else {
    //     return;
    // };
    // let origin_type = expand_type(context.get_salsa_db(), &typ).unwrap_or(typ);

    // for op in cast.get_op_types() {
    //     if op.get_op().is_some() {
    //         continue;
    //     }
    //     let Some(target_doc) = op.get_type() else {
    //         continue;
    //     };
    //     let target_type = expand_type(context.get_salsa_db(), &target_doc).unwrap_or(target_doc);
    //     let range = op.get_range();
    //     if model.type_check(&origin_type, &target_type).is_err() {
    //         let origin = humanize_lint_type_salsa(context.get_salsa_db(), context.get_file_id(), &origin_type);
    //         let target = humanize_lint_type_salsa(context.get_salsa_db(), context.get_file_id(), &target_type);
    //         context.add_diagnostic(
    //             DiagnosticCode::CastTypeMismatch,
    //             range,
    //             t!(
    //                 "Cannot cast `%{origin}` to `%{target}`",
    //                 origin = origin,
    //                 target = target
    //             )
    //             .to_string(),
    //             None,
    //         );
    //     }
    // }
}
