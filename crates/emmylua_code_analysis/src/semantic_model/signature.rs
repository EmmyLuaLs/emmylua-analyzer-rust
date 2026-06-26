//! Salsa workspace signature queries — salsa equivalent of old LuaSignatureIndex.
//!
//! Wraps `SalsaSignatureExplainSummary` to provide an old LuaSignature-compatible API.

use crate::compilation::{
    SalsaDocTagPropertyEntrySummary, SalsaSignatureExplainSummary, SalsaSummaryDatabase,
};
use crate::{FileId, LuaType, VariadicType};

/// Workspace-level signature info — wraps salsa `SalsaSignatureExplainSummary`.
/// Provides the equivalent of old `LuaSignature` API.
pub struct SignatureInfo {
    explain: SalsaSignatureExplainSummary,
}

impl SignatureInfo {
    /// Query signature by file + offset. Returns None if not found.
    pub fn query(
        db: &SalsaSummaryDatabase,
        file_id: FileId,
        offset: rowan::TextSize,
    ) -> Option<Self> {
        db.doc()
            .signature()
            .explain(file_id, offset)
            .map(|e| Self { explain: e })
    }

    /// Parameter names only (like old `get_params()`).
    pub fn param_names(&self) -> Vec<String> {
        self.explain
            .signature
            .params
            .iter()
            .map(|p| p.name.to_string())
            .collect()
    }

    /// Parameters with resolved types (like old `get_type_params()`).
    pub fn type_params(&self) -> Vec<(String, Option<LuaType>)> {
        self.explain
            .params
            .iter()
            .map(|p| {
                let ty = p.doc_type.as_ref().and_then(|dt| {
                    crate::semantic_model::infer::lowered_node_to_lua_type(dt.lowered.as_ref()?)
                });
                (p.name.to_string(), ty)
            })
            .collect()
    }

    /// Return type (like old `get_ret()` / `get_return_type()`).
    /// Aggregates ALL return items from all return groups into the proper
    /// `Variadic(Multi(...))` or `Variadic(Base(...))` representation.
    pub fn return_type(&self) -> LuaType {
        let mut types: Vec<LuaType> = Vec::new();

        for ret_group in &self.explain.returns {
            for item in &ret_group.items {
                let ty = item
                    .doc_type
                    .lowered
                    .as_ref()
                    .and_then(crate::semantic_model::infer::lowered_node_to_lua_type)
                    .unwrap_or(LuaType::Unknown);

                // Variadic return detected via name "..." or type-level Variadic
                let is_variadic =
                    item.name.as_deref() == Some("...") || matches!(ty, LuaType::Variadic(_));

                if is_variadic {
                    if matches!(ty, LuaType::Variadic(_)) {
                        types.push(ty);
                    } else {
                        types.push(LuaType::Variadic(VariadicType::Base(ty).into()));
                    }
                } else {
                    types.push(ty);
                }
            }
        }

        match types.len() {
            0 => LuaType::Unknown,
            1 => types.pop().unwrap_or(LuaType::Unknown),
            _ => LuaType::Variadic(VariadicType::Multi(types).into()),
        }
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
            tag.entries
                .iter()
                .any(|e| matches!(e, SalsaDocTagPropertyEntrySummary::Async))
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

    /// 泛型参数列表：(name, constraint, default)。
    pub fn generics(&self) -> Vec<(String, Option<LuaType>, Option<LuaType>)> {
        self.explain
            .generics
            .iter()
            .flat_map(|g| &g.params)
            .map(|param| {
                let constraint = param.bound_type.as_ref().and_then(|bt| {
                    crate::semantic_model::infer::lowered_node_to_lua_type(bt.lowered.as_ref()?)
                });
                let default = param.default_type.as_ref().and_then(|dt| {
                    crate::semantic_model::infer::lowered_node_to_lua_type(dt.lowered.as_ref()?)
                });
                (param.name.to_string(), constraint, default)
            })
            .collect()
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
