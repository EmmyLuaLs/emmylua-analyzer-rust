use emmylua_code_analysis::{
    DbIndex, FileId, LuaCompilation, LuaDeclId, LuaMemberId, LuaSemanticDeclId, LuaTypeOwner,
    SemanticModel,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaGeneralToken, LuaStat, LuaTokenKind, LuaVarExpr,
    PathTrait,
};
use lsp_types::{CallHierarchyIncomingCall, CallHierarchyItem, Location, SymbolKind};
use rowan::TokenAtOffset;
use serde::{Deserialize, Serialize};

use crate::handlers::references::{search_decl_references, search_member_references};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallHierarchyItemData {
    pub semantic_decl: LuaSemanticDeclId,
    pub file_id: FileId,
}

pub fn build_call_hierarchy_item(
    semantic_model: &SemanticModel,
    semantic_decl: LuaSemanticDeclId,
) -> Option<CallHierarchyItem> {
    let db = semantic_model.get_db();
    let file_id = semantic_model.get_file_id();
    let data = CallHierarchyItemData {
        semantic_decl: semantic_decl.clone(),
        file_id,
    };
    match semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let decl = db.get_decl_index().get_decl(&decl_id)?;
            let range = decl.get_range();
            let file_id = decl.get_file_id();
            let document = semantic_model.get_document_by_file_id(file_id)?;
            let uri = document.get_uri();
            let name = decl.get_name().to_string();
            let lsp_range = document.to_lsp_range(range)?;

            Some(CallHierarchyItem {
                name,
                kind: get_kind(db, decl_id.into()),
                tags: None,
                detail: None,
                uri,
                range: lsp_range,
                selection_range: lsp_range,
                data: Some(serde_json::to_value(data).ok()?),
            })
        }
        LuaSemanticDeclId::Member(member_id) => {
            let member = db.get_member_index().get_member(&member_id)?;
            let range = member.get_range();
            let file_id = member.get_file_id();
            let document = semantic_model.get_document_by_file_id(file_id)?;
            let uri = document.get_uri();
            let name = member.get_key().get_name()?.to_string();
            let lsp_range = document.to_lsp_range(range)?;

            Some(CallHierarchyItem {
                name,
                kind: get_kind(db, member_id.into()),
                tags: None,
                detail: None,
                uri,
                range: lsp_range,
                selection_range: lsp_range,
                data: Some(serde_json::to_value(data).ok()?),
            })
        }
        _ => None,
    }
}

fn get_kind(db: &DbIndex, type_owner: LuaTypeOwner) -> SymbolKind {
    let type_cache = db.get_type_index().get_type_cache(&type_owner);
    match type_cache {
        Some(typ) => {
            if typ.is_function() {
                SymbolKind::FUNCTION
            } else if typ.is_ref() || typ.is_def() {
                SymbolKind::CLASS
            } else if typ.is_const() {
                SymbolKind::CONSTANT
            } else {
                SymbolKind::VARIABLE
            }
        }
        None => SymbolKind::VARIABLE,
    }
}

pub fn build_incoming_hierarchy(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    semantic_decl: LuaSemanticDeclId,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let mut result = vec![];
    let mut locations = vec![];
    match semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            search_decl_references(semantic_model, compilation, decl_id, &mut locations);
        }
        LuaSemanticDeclId::Member(member_id) => {
            search_member_references(semantic_model, compilation, member_id, true, &mut locations);
        }
        _ => return None,
    }

    for location in locations {
        build_incoming_hierarchy_item(compilation, &location, &mut result);
    }

    Some(result)
}

fn build_incoming_hierarchy_item(
    compilation: &LuaCompilation,
    location: &Location,
    result: &mut Vec<CallHierarchyIncomingCall>,
) -> Option<()> {
    let db = compilation.get_db();
    let uri = location.uri.clone();
    let range = location.range;
    let file_id = db.get_vfs().get_file_id(&uri)?;
    let tree = db.get_vfs().get_syntax_tree(&file_id)?;
    let root_chunk = tree.get_chunk_node();
    let document = db.get_vfs().get_document(&file_id)?;
    let pos = document.get_offset(range.start.line as usize, range.start.character as usize)?;
    let token = match root_chunk.syntax().token_at_offset(pos) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, right) => {
            if left.kind() == LuaTokenKind::TkName.into() {
                left
            } else {
                right
            }
        }
        TokenAtOffset::None => {
            return None;
        }
    };

    let general_token = LuaGeneralToken::cast(token)?;
    let blocks = general_token.ancestors::<LuaBlock>();
    for block in blocks {
        let block_parent = block.get_parent::<LuaAst>()?;
        match block_parent {
            LuaAst::LuaChunk(_) => {
                let item = CallHierarchyItem {
                    name: document.get_file_name()?,
                    kind: SymbolKind::MODULE,
                    tags: None,
                    detail: None,
                    uri: uri.clone(),
                    range: document.get_document_lsp_range(),
                    selection_range: document.get_document_lsp_range(),
                    data: None,
                };

                result.push(CallHierarchyIncomingCall {
                    from: item,
                    from_ranges: vec![range],
                });
            }
            LuaAst::LuaClosureExpr(closure) => {
                let closure_parent = match closure.get_parent::<LuaStat>() {
                    Some(stat) => stat,
                    None => continue,
                };

                match closure_parent {
                    LuaStat::FuncStat(func_stat) => {
                        let func_name = func_stat.get_func_name()?;
                        let name_lsp_range = document.to_lsp_range(func_name.get_range())?;
                        let access_path = func_name.get_access_path()?;
                        let semantic_decl = match func_name {
                            LuaVarExpr::IndexExpr(index_expr) => LuaSemanticDeclId::Member(
                                LuaMemberId::new(index_expr.get_syntax_id(), file_id),
                            ),
                            LuaVarExpr::NameExpr(name_expr) => LuaSemanticDeclId::LuaDecl(
                                LuaDeclId::new(file_id, name_expr.get_position()),
                            ),
                        };

                        let item = CallHierarchyItem {
                            name: access_path,
                            kind: SymbolKind::FUNCTION,
                            tags: None,
                            detail: None,
                            uri: uri.clone(),
                            range: name_lsp_range,
                            selection_range: name_lsp_range,
                            data: Some(
                                serde_json::to_value(CallHierarchyItemData {
                                    semantic_decl,
                                    file_id,
                                })
                                .ok()?,
                            ),
                        };

                        result.push(CallHierarchyIncomingCall {
                            from: item,
                            from_ranges: vec![range],
                        });
                    }
                    LuaStat::LocalFuncStat(local_func_stat) => {
                        let func_name = local_func_stat.get_local_name()?;
                        let name_lsp_range = document.to_lsp_range(func_name.get_range())?;
                        let name = func_name.get_name_token()?.get_text().to_string();
                        let semantic_decl = LuaSemanticDeclId::LuaDecl(LuaDeclId::new(
                            file_id,
                            func_name.get_position(),
                        ));

                        let item = CallHierarchyItem {
                            name,
                            kind: SymbolKind::FUNCTION,
                            tags: None,
                            detail: None,
                            uri: uri.clone(),
                            range: name_lsp_range,
                            selection_range: name_lsp_range,
                            data: Some(
                                serde_json::to_value(CallHierarchyItemData {
                                    semantic_decl,
                                    file_id,
                                })
                                .ok()?,
                            ),
                        };

                        result.push(CallHierarchyIncomingCall {
                            from: item,
                            from_ranges: vec![range],
                        });
                    }
                    _ => continue,
                }

                break;
            }
            _ => {
                return None;
            }
        }
    }

    Some(())
}
