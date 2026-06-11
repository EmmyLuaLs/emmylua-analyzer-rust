//! Salsa workspace signature queries — salsa equivalent of old LuaSignatureIndex.
//!
//! Wraps `SalsaSignatureExplainSummary` to provide an old LuaSignature-compatible API.

use std::sync::Arc;

use crate::compilation::{
    SalsaDocTagPropertyEntrySummary, SalsaSignatureExplainSummary, SalsaSignatureParamSummary,
    SalsaSummaryDatabase,
};
use crate::{FileId, LuaType};

/// Workspace-level signature info — wraps salsa `SalsaSignatureExplainSummary`.
/// Provides the equivalent of old `LuaSignature` API.
pub struct SignatureInfo {
    explain: SalsaSignatureExplainSummary,
}

impl SignatureInfo {
    /// Query signature by file + offset. Returns None if not found.
    pub fn query(db: &SalsaSummaryDatabase, file_id: FileId, offset: rowan::TextSize) -> Option<Self> {
        db.doc().signature().explain(file_id, offset).map(|e| Self { explain: e })
    }

    /// Parameter names only (like old `get_params()`).
    pub fn param_names(&self) -> Vec<String> {
        self.explain.signature.params.iter().map(|p| p.name.to_string()).collect()
    }

    /// Parameters with resolved types (like old `get_type_params()`).
    pub fn type_params(&self) -> Vec<(String, Option<LuaType>)> {
        self.explain.params.iter().map(|p| {
            let ty = p.doc_type.as_ref().and_then(|dt| {
                crate::semantic_model::infer::lowered_node_to_lua_type(dt.lowered.as_ref()?)
            });
            (p.name.to_string(), ty)
        }).collect()
    }

    /// Return type (like old `get_ret()` / `get_return_type()`).
    pub fn return_type(&self) -> LuaType {
        self.explain.returns.first()
            .and_then(|r| r.items.first())
            .and_then(|item| {
                crate::semantic_model::infer::lowered_node_to_lua_type(item.doc_type.lowered.as_ref()?)
            })
            .unwrap_or(LuaType::Unknown)
    }

    /// Whether this is a method.
    pub fn is_method(&self) -> bool {
        self.explain.signature.is_method
    }

    /// Whether defined with colon syntax.
    pub fn is_colon_define(&self) -> bool {
        // Salsa sets is_method based on whether the function was defined with `:` syntax
        self.explain.signature.is_method
    }

    /// Whether variadic.
    pub fn is_variadic(&self) -> bool {
        self.explain.signature.params.iter().any(|p| p.is_vararg)
    }

    /// Async state from tag properties.
    pub fn is_async(&self) -> bool {
        self.explain.tag_properties.iter().any(|tag| {
            tag.entries.iter().any(|e| matches!(e, SalsaDocTagPropertyEntrySummary::Async))
        })
    }

    /// Nodiscard message, if any.
    pub fn nodiscard(&self) -> Option<String> {
        self.explain.tag_properties.iter().find_map(|tag| {
            tag.entries.iter().find_map(|e| match e {
                SalsaDocTagPropertyEntrySummary::Nodiscard(Some(msg)) => Some(msg.to_string()),
                SalsaDocTagPropertyEntrySummary::Nodiscard(None) => Some("no discard".to_string()),
                _ => None,
            })
        })
    }

    /// Return type resolved from doc.
    pub fn resolve_return(&self) -> SignatureReturnStatus {
        // Salsa returns are always doc-resolved when explain is available
        if self.explain.returns.is_empty() {
            SignatureReturnStatus::None
        } else {
            SignatureReturnStatus::DocResolve
        }
    }
}

/// Return status for signature (mirrors old SignatureReturnStatus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureReturnStatus {
    None,
    DocResolve,
}
