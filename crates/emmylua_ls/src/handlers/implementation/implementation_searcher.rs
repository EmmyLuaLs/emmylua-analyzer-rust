use std::collections::HashMap;

use emmylua_code_analysis::{
    LuaCompilation, LuaDeclId, LuaMemberId, LuaSemanticDeclId, LuaTypeDeclId, SemanticDeclLevel,
    SemanticModel,
};
use emmylua_parser::{LuaAstNode, LuaIndexExpr, LuaSyntaxToken};
use lsp_types::Location;

pub fn search_implementations(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    token: LuaSyntaxToken,
) -> Option<Vec<Location>> {
    let mut result = Vec::new();
    if let Some(semantic_decl) =
        semantic_model.find_decl(token.clone().into(), SemanticDeclLevel::NoTrace)
    {
        dbg!(&semantic_decl);
        match semantic_decl {
            LuaSemanticDeclId::TypeDecl(type_decl_id) => {
                search_type_implementations(semantic_model, compilation, type_decl_id, &mut result);
            }
            LuaSemanticDeclId::Member(member_id) => {
                search_member_implementations(
                    semantic_model,
                    compilation,
                    member_id,
                    token,
                    &mut result,
                );
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
    token: LuaSyntaxToken,
    result: &mut Vec<Location>,
) -> Option<()> {
    let member = semantic_model
        .get_db()
        .get_member_index()
        .get_member(&member_id)?;
    let key = member.get_key();

    let parent_node = token.parent()?;
    let index_expr = LuaIndexExpr::cast(parent_node)?;
    let prefix_type = semantic_model
        .infer_expr(index_expr.get_prefix_expr()?.into())
        .ok()?;
    let member_map = semantic_model.infer_member_map(&prefix_type)?;
    let member_infos = member_map.get(&key)?;

    let mut semantic_cache = HashMap::new();

    for member_info in member_infos {
        if let Some(LuaSemanticDeclId::Member(member_id)) = member_info.property_owner_id {
            let semantic_model =
                if let Some(semantic_model) = semantic_cache.get_mut(&member_id.file_id) {
                    semantic_model
                } else {
                    let semantic_model = compilation.get_semantic_model(member_id.file_id)?;
                    semantic_cache.insert(member_id.file_id, semantic_model);
                    semantic_cache.get_mut(&member_id.file_id)?
                };
            let document = semantic_model.get_document();
            let range = member_id.get_syntax_id().get_range();
            let location = document.to_lsp_location(range)?;
            result.push(location);
        }
    }

    Some(())
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
