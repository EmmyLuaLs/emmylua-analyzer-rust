use emmylua_code_analysis::{LuaMember, LuaSignatureId, LuaType, SemanticModel};
use emmylua_parser::{LuaAstNode, LuaDocTagField, LuaDocType};

pub fn is_function(typ: &LuaType) -> bool {
    typ.is_function()
        || match &typ {
            LuaType::Union(union) => union
                .get_types()
                .iter()
                .all(|t| matches!(t, LuaType::DocFunction(_) | LuaType::Signature(_))),
            _ => false,
        }
}

/// 尝试从 @field 定义中提取函数类型的位置信息
pub fn try_extract_signature_id_from_field(
    semantic_model: &SemanticModel,
    member: &LuaMember,
) -> Option<LuaSignatureId> {
    // 检查是否是 field 定义
    if !member.is_field() {
        return None;
    }

    let root = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&member.get_file_id())?
        .get_red_root();
    let field_node = member.get_syntax_id().to_node_from_root(&root)?;

    // 尝试转换为 LuaDocTagField
    let field_tag = LuaDocTagField::cast(field_node)?;

    // 获取类型定义
    let type_node = field_tag.get_type()?;

    match &type_node {
        LuaDocType::Func(doc_func) => Some(LuaSignatureId::from_doc_func(
            member.get_file_id(),
            &doc_func,
        )),
        _ => None,
    }
}
