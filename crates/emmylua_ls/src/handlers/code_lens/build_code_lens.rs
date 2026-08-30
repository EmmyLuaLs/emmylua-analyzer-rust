use emmylua_code_analysis::{DocumentView, SalsaSemanticModel, SemanticId};
use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaFuncStat, LuaLocalFuncStat, LuaVarExpr};
use lsp_types::CodeLens;
use serde::{Deserialize, Serialize};

/// Serializable encoding of `SemanticId` (file + range).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticIdData {
    pub kind: String,
    pub file_id: u32,
    pub range: (u32, u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeLensData {
    Member(SemanticIdData),
    DeclId(SemanticIdData),
}

impl SemanticIdData {
    pub fn from_semantic_id(id: &SemanticId) -> Self {
        match id {
            SemanticId::Decl(key) => SemanticIdData {
                kind: "Decl".to_string(),
                file_id: key.file_id.id,
                range: (key.name_range.start().into(), key.name_range.end().into()),
            },
            SemanticId::Member(key) => SemanticIdData {
                kind: "Member".to_string(),
                file_id: key.file_id.id,
                range: (key.key_range.start().into(), key.key_range.end().into()),
            },
            _ => SemanticIdData {
                kind: "Unknown".to_string(),
                file_id: 0,
                range: (0, 0),
            },
        }
    }

    pub fn to_semantic_id(&self) -> Option<SemanticId> {
        let file_id = emmylua_code_analysis::FileId::new(self.file_id);
        let range = rowan::TextRange::new(self.range.0.into(), self.range.1.into());
        match self.kind.as_str() {
            "Decl" => Some(SemanticId::decl(file_id, range)),
            "Member" => Some(SemanticId::member(file_id, range)),
            _ => None,
        }
    }
}

pub fn build_code_lens(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
) -> Option<Vec<CodeLens>> {
    let mut result = Vec::new();
    let root = model.chunk()?;
    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaFuncStat(func_stat) => {
                add_func_stat_code_lens(model, document, &mut result, func_stat)?;
            }
            LuaAst::LuaLocalFuncStat(local_func_stat) => {
                add_local_func_stat_code_lens(model, document, &mut result, local_func_stat)?;
            }
            _ => {}
        }
    }

    Some(result)
}

fn add_func_stat_code_lens(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    result: &mut Vec<CodeLens>,
    func_stat: LuaFuncStat,
) -> Option<()> {
    let func_name = func_stat.get_func_name()?;
    match func_name {
        LuaVarExpr::IndexExpr(index_expr) => {
            let semantic_id = model.find_decl(index_expr.syntax().clone().into())?;
            let data = CodeLensData::Member(SemanticIdData::from_semantic_id(&semantic_id));
            let index_name_token = index_expr.get_index_name_token()?;
            let range = document.to_lsp_range(index_name_token.text_range())?;
            result.push(CodeLens {
                range,
                command: None,
                data: Some(serde_json::to_value(data).ok()?),
            });
        }
        LuaVarExpr::NameExpr(name_expr) => {
            let name_token = name_expr.get_name_token()?;
            let semantic_id = model.decl_by_offset(name_token.get_position())?;
            let data = CodeLensData::DeclId(SemanticIdData::from_semantic_id(&semantic_id));
            let range = document.to_lsp_range(name_token.get_range())?;
            result.push(CodeLens {
                range,
                command: None,
                data: Some(serde_json::to_value(data).ok()?),
            });
        }
    }

    Some(())
}

fn add_local_func_stat_code_lens(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    result: &mut Vec<CodeLens>,
    local_func_stat: LuaLocalFuncStat,
) -> Option<()> {
    let func_name = local_func_stat.get_local_name()?;
    let range = document.to_lsp_range(func_name.get_range())?;
    let name_token = func_name.get_name_token()?;
    let semantic_id = model.decl_by_offset(name_token.get_position())?;
    let data = CodeLensData::DeclId(SemanticIdData::from_semantic_id(&semantic_id));
    result.push(CodeLens {
        range,
        command: None,
        data: Some(serde_json::to_value(data).ok()?),
    });
    Some(())
}
