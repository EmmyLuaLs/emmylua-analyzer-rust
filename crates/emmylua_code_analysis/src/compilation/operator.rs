//! Operator query projection layer.
//!
//! Provides a centralized entry point for operator-related queries, replacing direct
//! reads of `LuaOperatorIndex` in `semantic/`. Currently delegates to the legacy
//! operator index; will be migrated to Salsa-backed queries when the analyzer write
//! path for operators is replaced.
//!
//! Phase 3a.1 — thin projection over legacy operator_index.

use crate::{
    DbIndex,
    db_index::{LuaOperator, LuaOperatorId, LuaOperatorMetaMethod, LuaOperatorOwner},
};

/// Returns operator IDs registered for the given owner and meta-method.
///
/// Replaces `db.get_operator_index().get_operators(owner, method)`.
pub(crate) fn get_operators(
    db: &DbIndex,
    owner: &LuaOperatorOwner,
    method: LuaOperatorMetaMethod,
) -> Option<Vec<LuaOperatorId>> {
    db.get_operator_index()
        .get_operators(owner, method).cloned()
}

/// Returns an operator by its ID.
///
/// Replaces `db.get_operator_index().get_operator(id)`.
pub(crate) fn get_operator<'a>(db: &'a DbIndex, id: &LuaOperatorId) -> Option<&'a LuaOperator> {
    db.get_operator_index().get_operator(id)
}
