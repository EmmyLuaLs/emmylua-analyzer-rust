//! 可见性检查模块
//!
//! 检查某个声明在给定 token 位置是否可见。

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaBlock, LuaClosureExpr, LuaFuncStat, LuaGeneralToken, LuaIndexExpr,
    LuaSyntaxToken, LuaVarExpr,
};
use std::sync::Arc;

use crate::compilation::{SalsaDocVisibilityKindSummary, SalsaSummaryDatabase};
use crate::{Emmyrc, FileId, LuaSemanticDeclId, LuaType};

use crate::semantic_model::type_check;
use crate::semantic_model::InferQuery;

/// 检查指定可见性在给定 token 位置是否允许访问。
pub fn check_visibility(
    db: &SalsaSummaryDatabase,
    infer: &InferQuery,
    file_id: FileId,
    emmyrc: &Arc<Emmyrc>,
    token: LuaSyntaxToken,
    property_owner: &LuaSemanticDeclId,
    visibility: Option<&SalsaDocVisibilityKindSummary>,
) -> Option<bool> {
    match visibility {
        Some(SalsaDocVisibilityKindSummary::Public) => Some(true),
        Some(SalsaDocVisibilityKindSummary::Protected | SalsaDocVisibilityKindSummary::Private) => {
            let vis = visibility?;
            Some(check_class_visibility(db, infer, &token, property_owner, vis).unwrap_or(false))
        }
        Some(SalsaDocVisibilityKindSummary::Package) => {
            Some(property_owner.get_file_id().is_none_or(|f| f == file_id))
        }
        Some(SalsaDocVisibilityKindSummary::Internal) => {
            // TODO: workspace 检查需要 VFS 桥接
            Some(true)
        }
        None => Some(check_private_name_pattern(emmyrc, property_owner)),
    }
}

/// Protected/Private：检查当前 token 是否在允许的类上下文中。
fn check_class_visibility(
    db: &SalsaSummaryDatabase,
    infer: &InferQuery,
    token: &LuaSyntaxToken,
    property_owner: &LuaSemanticDeclId,
    visibility: &SalsaDocVisibilityKindSummary,
) -> Option<bool> {
    let is_protected = matches!(visibility, SalsaDocVisibilityKindSummary::Protected);
    let gen_token = LuaGeneralToken::cast(token.clone())?;

    if let Some(index_expr) = gen_token.get_parent::<LuaIndexExpr>() {
        if let Some(prefix_expr) = index_expr.get_prefix_expr() {
            let prefix_type = infer.infer_expr(prefix_expr).ok()?;
            if check_type_matches_owner(&prefix_type, property_owner, is_protected) {
                return Some(true);
            }
        }
    }

    for block in gen_token.ancestors::<LuaBlock>() {
        if let Some(closure) = block.get_parent::<LuaClosureExpr>() {
            if let Some(func_stat) = closure.get_parent::<LuaFuncStat>() {
                if let Some(func_name) = func_stat.get_func_name() {
                    if let LuaVarExpr::IndexExpr(index_expr) = func_name {
                        if let Some(prefix_expr) = index_expr.get_prefix_expr() {
                            let prefix_type = infer.infer_expr(prefix_expr).ok()?;
                            if check_type_matches_owner(&prefix_type, property_owner, is_protected) {
                                return Some(true);
                            }
                        }
                    }
                }
            }
        }
    }

    Some(false)
}

fn check_type_matches_owner(
    ty: &LuaType,
    owner: &LuaSemanticDeclId,
    is_protected: bool,
) -> bool {
    match (ty, owner) {
        (LuaType::Def(def_id), LuaSemanticDeclId::TypeDecl(type_id)) => {
            if def_id == type_id {
                return true;
            }
            if is_protected {
                return type_check::is_sub_type_of(def_id, type_id);
            }
            false
        }
        _ => false,
    }
}

/// 检查 emmyrc 配置的 private_name 模式。
///
/// NOTE: 当前简化实现 — `LuaMemberId` 不直接暴露成员名称，
/// 需要通过 VFS 或 member 索引获取。待后续 phase 完善。
fn check_private_name_pattern(
    _emmyrc: &Arc<Emmyrc>,
    property_owner: &LuaSemanticDeclId,
) -> bool {
    match property_owner {
        LuaSemanticDeclId::Member(_member_id) => {
            // TODO: 获取成员名称后检查 emmyrc.doc.private_name 模式
            true
        }
        _ => true,
    }
}

/// 检查模块可见性。
pub fn check_module_visibility(
    module_visibility: &crate::ModuleVisibility,
    module_workspace_id: Option<&crate::WorkspaceId>,
    current_workspace_id: Option<&crate::WorkspaceId>,
) -> bool {
    match module_visibility {
        crate::ModuleVisibility::Public | crate::ModuleVisibility::Default => true,
        crate::ModuleVisibility::Hide => false,
        crate::ModuleVisibility::Internal => {
            match (current_workspace_id, module_workspace_id) {
                (Some(cur), Some(mod_ws)) => {
                    (!mod_ws.is_library() && !cur.is_library()) || mod_ws == cur
                }
                _ => true,
            }
        }
    }
}
