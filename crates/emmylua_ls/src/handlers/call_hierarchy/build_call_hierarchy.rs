use emmylua_code_analysis::{SalsaDatabase, SalsaSemanticModel, SemanticId};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaStat, LuaTokenKind, PathTrait};
use lsp_types::{CallHierarchyIncomingCall, CallHierarchyItem, Location, SymbolKind};
use rowan::{TextRange, TokenAtOffset};
use serde::{Deserialize, Serialize};

use crate::handlers::common::{decl_reference_ranges, member_reference_ranges};

/// Serializable encoding for `SemanticId` (identity = file + range).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemanticIdData {
    pub kind: String,
    pub file_id: u32,
    pub range: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallHierarchyItemData {
    pub semantic_decl: SemanticIdData,
    pub file_id: u32,
}

impl From<&SemanticId> for SemanticIdData {
    fn from(id: &SemanticId) -> Self {
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
}

impl SemanticIdData {
    pub fn to_semantic_id(&self) -> Option<SemanticId> {
        let file_id = emmylua_code_analysis::FileId::new(self.file_id);
        let range = TextRange::new(self.range.0.into(), self.range.1.into());
        match self.kind.as_str() {
            "Decl" => Some(SemanticId::decl(file_id, range)),
            "Member" => Some(SemanticId::member(file_id, range)),
            _ => None,
        }
    }
}

pub fn build_call_hierarchy_item(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    semantic_decl: &SemanticId,
) -> Option<CallHierarchyItem> {
    let data = CallHierarchyItemData {
        semantic_decl: semantic_decl.into(),
        file_id: model.file_id().id,
    };
    match semantic_decl {
        SemanticId::Decl(key) => {
            let decl_model = model_of(salsa, key.file_id);
            let decl = decl_model
                .decls()?
                .iter()
                .find(|d| d.id == *semantic_decl)?
                .clone();
            let document = salsa.document(key.file_id)?;
            let uri = document.get_uri()?;
            let lsp_range = document.to_lsp_range(decl.name_range)?;
            Some(CallHierarchyItem {
                name: decl.name.to_string(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                detail: None,
                uri,
                range: lsp_range,
                selection_range: lsp_range,
                data: Some(serde_json::to_value(data).ok()?),
            })
        }
        SemanticId::Member(key) => {
            let decl_model = model_of(salsa, key.file_id);
            let member = decl_model
                .members()?
                .iter()
                .find(|m| m.id == *semantic_decl)?
                .clone();
            let document = salsa.document(key.file_id)?;
            let uri = document.get_uri()?;
            let lsp_range = document.to_lsp_range(key.key_range)?;
            Some(CallHierarchyItem {
                name: member.key.to_path(),
                kind: SymbolKind::FUNCTION,
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

pub fn build_incoming_hierarchy(
    salsa: &SalsaDatabase,
    semantic_decl: &SemanticId,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let mut result = vec![];
    let ranges = match semantic_decl {
        SemanticId::Decl(_) => decl_reference_ranges(salsa, semantic_decl, true),
        SemanticId::Member(_) => member_reference_ranges(salsa, semantic_decl, true),
        _ => return None,
    };
    let mut seen = std::collections::HashSet::new();
    for (file_id, range) in ranges {
        if !seen.insert((file_id, range)) {
            continue;
        }
        if let Some(document) = salsa.document(file_id)
            && let Some(uri) = document.get_uri()
            && let Some(lsp_range) = document.to_lsp_range(range)
        {
            let location = Location {
                uri,
                range: lsp_range,
            };
            build_incoming_hierarchy_item(salsa, &location, &mut result);
        }
    }
    Some(result)
}

fn build_incoming_hierarchy_item(
    salsa: &SalsaDatabase,
    location: &Location,
    result: &mut Vec<CallHierarchyIncomingCall>,
) -> Option<()> {
    let file_id = salsa.lookup_file_id(&location.uri)?;
    let model = SalsaSemanticModel::new(salsa, file_id)?;
    let document = salsa.document(file_id)?;
    let chunk = model.chunk()?;
    let pos = document.get_offset(
        location.range.start.line as usize,
        location.range.start.character as usize,
    )?;
    let token = match chunk.syntax().token_at_offset(pos) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, right) => {
            if left.kind() == LuaTokenKind::TkName.into() {
                left
            } else {
                right
            }
        }
        TokenAtOffset::None => return None,
    };

    // Find the nearest enclosing FuncStat / LocalFuncStat (closest first).
    for stat in token.parent_ancestors().filter_map(LuaStat::cast) {
        match &stat {
            LuaStat::FuncStat(func_stat) => {
                let func_name = func_stat.get_func_name()?;
                let name_lsp_range = document.to_lsp_range(func_name.get_range())?;
                let semantic_decl = model.find_decl(func_name.syntax().clone().into())?;
                push_incoming_item(
                    salsa,
                    result,
                    location,
                    &model,
                    document.get_uri()?,
                    &semantic_decl,
                    func_name.get_access_path()?,
                    name_lsp_range,
                );
                return Some(());
            }
            LuaStat::LocalFuncStat(local_func_stat) => {
                let func_name = local_func_stat.get_local_name()?;
                let name_lsp_range = document.to_lsp_range(func_name.get_range())?;
                let name_token = func_name.get_name_token()?;
                let semantic_decl = model.decl_by_offset(name_token.get_position())?;
                push_incoming_item(
                    salsa,
                    result,
                    location,
                    &model,
                    document.get_uri()?,
                    &semantic_decl,
                    name_token.get_text().to_string(),
                    name_lsp_range,
                );
                return Some(());
            }
            _ => {}
        }
    }

    // Top-level (directly wrapped by chunk) -> module-level incoming.
    result.push(CallHierarchyIncomingCall {
        from: CallHierarchyItem {
            name: document
                .path
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            kind: SymbolKind::MODULE,
            tags: None,
            detail: None,
            uri: location.uri.clone(),
            range: lsp_types::Range::default(),
            selection_range: lsp_types::Range::default(),
            data: None,
        },
        from_ranges: vec![location.range],
    });
    Some(())
}

#[allow(clippy::too_many_arguments)]
fn push_incoming_item(
    _salsa: &SalsaDatabase,
    result: &mut Vec<CallHierarchyIncomingCall>,
    location: &Location,
    model: &SalsaSemanticModel<'_>,
    uri: lsp_types::Uri,
    semantic_decl: &SemanticId,
    name: String,
    name_lsp_range: lsp_types::Range,
) {
    let data = CallHierarchyItemData {
        semantic_decl: semantic_decl.into(),
        file_id: model.file_id().id,
    };
    result.push(CallHierarchyIncomingCall {
        from: CallHierarchyItem {
            name,
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri,
            range: name_lsp_range,
            selection_range: name_lsp_range,
            data: serde_json::to_value(data).ok(),
        },
        from_ranges: vec![location.range],
    });
}

fn model_of<'a>(
    salsa: &'a SalsaDatabase,
    file_id: emmylua_code_analysis::FileId,
) -> SalsaSemanticModel<'a> {
    SalsaSemanticModel::new(salsa, file_id).expect("salsa model must exist")
}
