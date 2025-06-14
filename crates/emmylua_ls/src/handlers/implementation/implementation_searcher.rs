use std::collections::HashMap;

use emmylua_code_analysis::{
    LuaCompilation, LuaDeclId, LuaMemberId, LuaSemanticDeclId, LuaType, LuaTypeDeclId,
    SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{
    LuaAstNode, LuaDocTagField, LuaExpr, LuaIndexExpr, LuaStat, LuaSyntaxNode, LuaSyntaxToken,
    LuaTableField,
};
use lsp_types::Location;

use crate::handlers::hover::find_member_origin_owner;

pub fn search_implementations(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    token: LuaSyntaxToken,
) -> Option<Vec<Location>> {
    let mut result = Vec::new();
    if let Some(semantic_decl) =
        semantic_model.find_decl(token.clone().into(), SemanticDeclLevel::NoTrace)
    {
        match semantic_decl {
            LuaSemanticDeclId::TypeDecl(type_decl_id) => {
                search_type_implementations(semantic_model, compilation, type_decl_id, &mut result);
            }
            LuaSemanticDeclId::Member(member_id) => {
                search_member_implementations(semantic_model, compilation, member_id, &mut result);
            }
            LuaSemanticDeclId::LuaDecl(decl_id) => {
                search_decl_implementations(semantic_model, compilation, decl_id, &mut result);
            }
            _ => {}
        }
    }

    Some(result)
}

pub fn search_member_implementations(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    member_id: LuaMemberId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let member = semantic_model
        .get_db()
        .get_member_index()
        .get_member(&member_id)?;
    let member_key = member.get_key();

    let index_references = semantic_model
        .get_db()
        .get_reference_index()
        .get_index_references(&member_key)?;

    let mut semantic_cache = HashMap::new();

    let property_owner = find_member_origin_owner(semantic_model, member_id)
        .unwrap_or(LuaSemanticDeclId::Member(member_id));
    for in_filed_syntax_id in index_references {
        let semantic_model =
            if let Some(semantic_model) = semantic_cache.get_mut(&in_filed_syntax_id.file_id) {
                semantic_model
            } else {
                let semantic_model = compilation.get_semantic_model(in_filed_syntax_id.file_id)?;
                semantic_cache.insert(in_filed_syntax_id.file_id, semantic_model);
                semantic_cache.get_mut(&in_filed_syntax_id.file_id)?
            };
        let root = semantic_model.get_root();
        let node = in_filed_syntax_id.value.to_node_from_root(root.syntax())?;
        if let Some(is_signature) = check_member_reference(&semantic_model, node.clone()) {
            if !semantic_model.is_reference_to(
                node,
                property_owner.clone(),
                SemanticDeclLevel::default(),
            ) {
                continue;
            }

            let document = semantic_model.get_document();
            let range = in_filed_syntax_id.value.get_range();
            let location = document.to_lsp_location(range)?;
            // 由于允许函数声明重载, 所以需要将签名放在前面
            if is_signature {
                result.insert(0, location);
            } else {
                result.push(location);
            }
        }
    }
    Some(())
}

/// 检查成员引用是否符合实现
fn check_member_reference(semantic_model: &SemanticModel, node: LuaSyntaxNode) -> Option<bool> {
    match &node {
        expr_node if LuaIndexExpr::can_cast(expr_node.kind().into()) => {
            let expr = LuaIndexExpr::cast(expr_node.clone())?;
            let prefix_type = semantic_model
                .infer_expr(expr.get_prefix_expr()?.into())
                .ok()?;
            let mut is_signature = false;
            if let Some(current_type) = semantic_model
                .infer_expr(LuaExpr::IndexExpr(expr.clone()))
                .ok()
            {
                if current_type.is_signature() {
                    is_signature = true;
                }
            }
            // TODO: 需要实现更复杂的逻辑, 即当为`Ref`时, 针对指定的实例定义到其实现
            /*
               ---@class A
               ---@field a number -- 这里寻找实现只匹配到`A.a`, 不能穿透到`a.a`与`b.a`
               local A = {}
               A.a = 1

               ---@type A
               local a = {}
               a.a = 1 -- 这里寻找实现不能匹配到`b.a`

               ---@type A
               local b = a
               b.a = 2 -- 这里寻找实现不能匹配到`a.a`
            */
            match prefix_type {
                LuaType::Ref(_) => {
                    return None;
                }
                _ => {}
            };
            // 往上寻找 stat 节点
            let stat = expr.ancestors::<LuaStat>().next()?;
            match stat {
                LuaStat::FuncStat(_) => {
                    return Some(is_signature);
                }
                LuaStat::AssignStat(assign_stat) => {
                    // 判断是否在左侧
                    let (vars, _) = assign_stat.get_var_and_expr_list();
                    for var in vars {
                        if var
                            .syntax()
                            .text_range()
                            .contains(node.text_range().start())
                        {
                            return Some(is_signature);
                        }
                    }
                    return None;
                }
                _ => {
                    return None;
                }
            }
        }
        tag_field_node if LuaDocTagField::can_cast(tag_field_node.kind().into()) => {
            return Some(false);
        }
        table_field_node if LuaTableField::can_cast(table_field_node.kind().into()) => {
            let table_field = LuaTableField::cast(table_field_node.clone())?;
            if table_field.is_assign_field() {
                return Some(false);
            } else {
                return None;
            }
        }
        _ => {}
    }

    Some(false)
}

pub fn search_type_implementations(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    type_decl_id: LuaTypeDeclId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let db = semantic_model.get_db();
    let type_index = db.get_type_index();
    let type_decl = type_index.get_type_decl(&type_decl_id)?;
    let locations = type_decl.get_locations();
    let mut semantic_cache = HashMap::new();
    for location in locations {
        let semantic_model = if let Some(semantic_model) = semantic_cache.get_mut(&location.file_id)
        {
            semantic_model
        } else {
            let semantic_model = compilation.get_semantic_model(location.file_id)?;
            semantic_cache.insert(location.file_id, semantic_model);
            semantic_cache.get_mut(&location.file_id)?
        };
        let document = semantic_model.get_document();
        let range = location.range;
        let location = document.to_lsp_location(range)?;
        result.push(location);
    }

    Some(())
}

pub fn search_decl_implementations(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    decl_id: LuaDeclId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;

    if decl.is_local() {
        let document = semantic_model.get_document();
        let range = decl.get_range();
        let location = document.to_lsp_location(range)?;
        result.push(location);
        return Some(());
    } else {
        let name = decl.get_name();
        let global_decl_ids = semantic_model
            .get_db()
            .get_global_index()
            .get_global_decl_ids(name)?;

        let mut semantic_cache = HashMap::new();

        for global_decl_id in global_decl_ids {
            let semantic_model =
                if let Some(semantic_model) = semantic_cache.get_mut(&global_decl_id.file_id) {
                    semantic_model
                } else {
                    let semantic_model = compilation.get_semantic_model(global_decl_id.file_id)?;
                    semantic_cache.insert(global_decl_id.file_id, semantic_model);
                    semantic_cache.get_mut(&global_decl_id.file_id)?
                };
            let Some(decl) = semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&global_decl_id)
            else {
                continue;
            };

            let document = semantic_model.get_document();
            let range = decl.get_range();
            let location = document.to_lsp_location(range)?;
            result.push(location);
        }
    }

    Some(())
}
