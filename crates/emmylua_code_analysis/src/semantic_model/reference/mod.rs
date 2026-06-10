//! 引用分析模块
//!
//! 检查某个 AST 节点是否是对特定声明的引用。
//!
//! 核心方法：
//! - `is_reference_to()` — 检查节点是否引用给定声明
//! - `find_decl()` — 查找节点引用的声明（通过 salsa 名称解析）

use emmylua_parser::LuaSyntaxNode;

use crate::compilation::{SalsaNameUseResolutionSummary, SalsaSummaryDatabase};
use crate::{FileId, LuaDeclId, LuaSemanticDeclId, SemanticDeclLevel};

/// 检查 AST 节点是否是对目标声明的引用。
pub fn is_reference_to(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: &LuaSyntaxNode,
    decl_id: &LuaSemanticDeclId,
    level: SemanticDeclLevel,
) -> Option<bool> {
    let node_decl = find_decl(db, file_id, node, level)?;
    if &node_decl == decl_id {
        return Some(true);
    }

    // 成员引用：检查是否引用到相同成员
    match (&node_decl, decl_id) {
        (LuaSemanticDeclId::Member(_node_member), LuaSemanticDeclId::Member(_target_member)) => {
            // TODO: member owner comparison + origin trace（后续 phase）
        }
        _ => {}
    }

    Some(false)
}

/// 通过 salsa 名称解析查找节点引用的声明。
pub fn find_decl(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: &LuaSyntaxNode,
    _level: SemanticDeclLevel,
) -> Option<LuaSemanticDeclId> {
    let name_info = db.types().name(file_id, node.text_range().start())?;
    resolve_salsa_name(file_id, &name_info.name_use.resolution)
}

/// 将 salsa 名称解析结果转换为 LuaSemanticDeclId。
fn resolve_salsa_name(
    file_id: FileId,
    resolution: &SalsaNameUseResolutionSummary,
) -> Option<LuaSemanticDeclId> {
    match resolution {
        SalsaNameUseResolutionSummary::LocalDecl(salsa_decl_id) => {
            let lua_decl_id = LuaDeclId::new(file_id, salsa_decl_id.0);
            Some(LuaSemanticDeclId::LuaDecl(lua_decl_id))
        }
        SalsaNameUseResolutionSummary::Global => None,
    }
}
