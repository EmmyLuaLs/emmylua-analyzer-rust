//! # completion_data — Completion item data payload (used for resolve; serde-serializable).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionData {
    /// Triggering file.
    pub field_id: u32,
    /// Trigger offset.
    pub trigger_offset: Option<u32>,
    /// Payload type.
    pub typ: CompletionDataType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompletionDataType {
    /// Member completion: member declaration identity (file, key_range).
    Member { file_id: u32, range: (u32, u32) },
    /// Declaration completion: declaration name identity (file, name_range).
    Decl { file_id: u32, range: (u32, u32) },
    /// Name completion: the name text.
    Name(String),
    /// Other (no resolution).
    None,
}

impl CompletionData {
    pub fn to_value(&self) -> Option<serde_json::Value> {
        serde_json::to_value(self).ok()
    }
}
