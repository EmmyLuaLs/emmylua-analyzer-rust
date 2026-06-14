use emmylua_code_analysis::{FileId, LuaSemanticDeclId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::completion_builder::CompletionBuilder;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionData {
    pub field_id: FileId,
    pub trigger_offset: Option<u32>,
    pub typ: CompletionDataType,
}

#[allow(unused)]
impl CompletionData {
    pub fn from_property_owner_id(
        builder: &CompletionBuilder,
        id: LuaSemanticDeclId,
    ) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            trigger_offset: Some(builder.position_offset.into()),
            typ: CompletionDataType::PropertyOwnerId(id),
        };
        Some(serde_json::to_value(data).unwrap())
    }

    pub fn from_overload(
        builder: &CompletionBuilder,
        id: LuaSemanticDeclId,
        index: usize,
    ) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            trigger_offset: Some(builder.position_offset.into()),
            typ: CompletionDataType::Overload((id, index)),
        };
        Some(serde_json::to_value(data).unwrap())
    }

    pub fn from_module(builder: &CompletionBuilder, module: String) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            trigger_offset: Some(builder.position_offset.into()),
            typ: CompletionDataType::Module(module),
        };
        Some(serde_json::to_value(data).unwrap())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompletionDataType {
    PropertyOwnerId(LuaSemanticDeclId),
    Module(String),
    Overload((LuaSemanticDeclId, usize)),
}
