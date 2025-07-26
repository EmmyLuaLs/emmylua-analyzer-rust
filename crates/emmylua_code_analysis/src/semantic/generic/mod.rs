mod instantiate_func_generic;
mod instantiate_special_generic;
mod instantiate_type_generic;
mod test;
mod tpl_context;
mod tpl_pattern;
mod type_substitutor;

use emmylua_parser::LuaAstNode;
use emmylua_parser::LuaExpr;
pub use instantiate_func_generic::build_self_type;
pub use instantiate_func_generic::infer_self_type;
pub use instantiate_func_generic::instantiate_func_generic;
pub use instantiate_type_generic::instantiate_doc_function;
pub use instantiate_type_generic::instantiate_type_generic;
use rowan::NodeOrToken;
pub use tpl_context::TplContext;
pub use tpl_pattern::tpl_pattern_match_args;
pub use type_substitutor::TypeSubstitutor;

use crate::DbIndex;
use crate::GenericTplId;
use crate::LuaDeclExtra;
use crate::LuaInferCache;
use crate::LuaMemberOwner;
use crate::LuaSemanticDeclId;
use crate::LuaType;
use crate::SemanticDeclLevel;
use crate::TypeOps;
use crate::infer_node_semantic_decl;
use crate::semantic::semantic_info::infer_token_semantic_decl;

pub fn get_tpl_ref_extend_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    arg_type: &LuaType,
    arg_expr: LuaExpr,
    depth: usize,
) -> Option<LuaType> {
    match arg_type {
        LuaType::TplRef(tpl_ref) => {
            let node_or_token = arg_expr.syntax().clone().into();
            let semantic_decl = match node_or_token {
                NodeOrToken::Node(node) => {
                    infer_node_semantic_decl(db, cache, node, SemanticDeclLevel::default())
                }
                NodeOrToken::Token(token) => {
                    infer_token_semantic_decl(db, cache, token, SemanticDeclLevel::default())
                }
            }?;

            match tpl_ref.get_tpl_id() {
                GenericTplId::Func(tpl_id) => {
                    if let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl {
                        let decl = db.get_decl_index().get_decl(&decl_id)?;
                        match decl.extra {
                            LuaDeclExtra::Param { signature_id, .. } => {
                                let signature = db.get_signature_index().get(&signature_id)?;
                                if let Some((_, param_type)) =
                                    signature.generic_params.get(tpl_id as usize)
                                {
                                    return param_type.clone();
                                }
                            }
                            _ => return None,
                        }
                    }
                    None
                }
                GenericTplId::Type(tpl_id) => {
                    if let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl {
                        let decl = db.get_decl_index().get_decl(&decl_id)?;
                        match decl.extra {
                            LuaDeclExtra::Param {
                                owner_member_id, ..
                            } => {
                                let owner_member_id = owner_member_id?;
                                let parent_owner =
                                    db.get_member_index().get_current_owner(&owner_member_id)?;
                                match parent_owner {
                                    LuaMemberOwner::Type(type_id) => {
                                        let generic_params =
                                            db.get_type_index().get_generic_params(&type_id)?;
                                        return generic_params.get(tpl_id as usize)?.1.clone();
                                    }
                                    _ => return None,
                                }
                            }
                            _ => return None,
                        }
                    }
                    None
                }
            }
        }
        LuaType::Union(union_type) => {
            if depth > 1 {
                return None;
            }
            let mut result = LuaType::Unknown;
            for union_member_type in union_type.into_vec().iter() {
                let extend_type = get_tpl_ref_extend_type(
                    db,
                    cache,
                    union_member_type,
                    arg_expr.clone(),
                    depth + 1,
                )
                .unwrap_or(union_member_type.clone());
                result = TypeOps::Union.apply(db, &result, &extend_type);
            }
            Some(result)
        }
        _ => None,
    }
}
