//! Salsa-tracked rich semantic queries.
//!
//! These are the real memoization/cycle boundaries for `SemanticModel::type_of_*`.
//! `SemanticModel` thin wrappers delegate here; recursive member/VM/flow dependencies
//! are handled by Salsa's `cycle_initial` / `cycle_fn` instead of manual depth counters.

use emmylua_parser::LuaSyntaxId;

use crate::salsa_builder::SalsaDb;
use crate::salsa_builder::def::SemanticId;
use crate::salsa_builder::inputs::{ConfigInput, SourceFileInput};
use crate::{LuaMemberKey, LuaType};

use super::SemanticModel;

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_decl_type_cycle_initial,
    cycle_fn = semantic_decl_type_cycle_recover
)]
pub(crate) fn semantic_decl_type(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    decl: SemanticId,
) -> Option<LuaType> {
    let model = SemanticModel::new(db.database(), file.file_id(db))?;
    model.type_of_decl_impl(&decl)
}

fn semantic_decl_type_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
) -> Option<LuaType> {
    None
}

fn semantic_decl_type_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &Option<LuaType>,
    value: Option<LuaType>,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
) -> Option<LuaType> {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_member_type_cycle_initial,
    cycle_fn = semantic_member_type_cycle_recover
)]
pub(crate) fn semantic_member_type(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    member: SemanticId,
) -> Option<LuaType> {
    let model = SemanticModel::new(db.database(), file.file_id(db))?;
    model.type_of_member_impl(&member)
}

fn semantic_member_type_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _member: SemanticId,
) -> Option<LuaType> {
    None
}

fn semantic_member_type_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &Option<LuaType>,
    value: Option<LuaType>,
    _file: SourceFileInput,
    _config: ConfigInput,
    _member: SemanticId,
) -> Option<LuaType> {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_expr_type_cycle_initial,
    cycle_fn = semantic_expr_type_cycle_recover
)]
pub(crate) fn semantic_expr_type(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    expr_syntax: LuaSyntaxId,
) -> LuaType {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return LuaType::Unknown;
    };
    model.type_of_expr_impl(expr_syntax)
}

fn semantic_expr_type_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _expr_syntax: LuaSyntaxId,
) -> LuaType {
    LuaType::Unknown
}

fn semantic_expr_type_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &LuaType,
    value: LuaType,
    _file: SourceFileInput,
    _config: ConfigInput,
    _expr_syntax: LuaSyntaxId,
) -> LuaType {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_decl_type_at_cycle_initial,
    cycle_fn = semantic_decl_type_at_cycle_recover
)]
pub(crate) fn semantic_decl_type_at(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    decl: SemanticId,
    offset: rowan::TextSize,
) -> LuaType {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return LuaType::Unknown;
    };
    model.type_of_decl_at_impl(&decl, offset)
}

fn semantic_decl_type_at_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    LuaType::Unknown
}

fn semantic_decl_type_at_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &LuaType,
    value: LuaType,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_member_type_at_cycle_initial,
    cycle_fn = semantic_member_type_at_cycle_recover
)]
pub(crate) fn semantic_member_type_at(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    member: SemanticId,
    offset: rowan::TextSize,
) -> LuaType {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return LuaType::Unknown;
    };
    model.type_of_member_at_impl(&member, offset)
}

fn semantic_member_type_at_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _member: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    LuaType::Unknown
}

fn semantic_member_type_at_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &LuaType,
    value: LuaType,
    _file: SourceFileInput,
    _config: ConfigInput,
    _member: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_expr_type_at_cycle_initial,
    cycle_fn = semantic_expr_type_at_cycle_recover
)]
pub(crate) fn semantic_expr_type_at(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    expr_syntax: LuaSyntaxId,
    offset: rowan::TextSize,
) -> LuaType {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return LuaType::Unknown;
    };
    model.type_of_expr_at_impl(expr_syntax, offset)
}

fn semantic_expr_type_at_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _expr_syntax: LuaSyntaxId,
    _offset: rowan::TextSize,
) -> LuaType {
    LuaType::Unknown
}

fn semantic_expr_type_at_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &LuaType,
    value: LuaType,
    _file: SourceFileInput,
    _config: ConfigInput,
    _expr_syntax: LuaSyntaxId,
    _offset: rowan::TextSize,
) -> LuaType {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_decl_type_before_at_cycle_initial,
    cycle_fn = semantic_decl_type_before_at_cycle_recover
)]
pub(crate) fn semantic_decl_type_before_at(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    decl: SemanticId,
    offset: rowan::TextSize,
) -> LuaType {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return LuaType::Unknown;
    };
    model.type_of_decl_before_at_impl(&decl, offset)
}

fn semantic_decl_type_before_at_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    LuaType::Unknown
}

fn semantic_decl_type_before_at_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &LuaType,
    value: LuaType,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_decl_assign_target_at_cycle_initial,
    cycle_fn = semantic_decl_assign_target_at_cycle_recover
)]
pub(crate) fn semantic_decl_assign_target_at(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    decl: SemanticId,
    offset: rowan::TextSize,
) -> LuaType {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return LuaType::Unknown;
    };
    model.type_of_decl_assign_target_at_impl(&decl, offset)
}

fn semantic_decl_assign_target_at_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    LuaType::Unknown
}

fn semantic_decl_assign_target_at_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &LuaType,
    value: LuaType,
    _file: SourceFileInput,
    _config: ConfigInput,
    _decl: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_member_type_before_at_cycle_initial,
    cycle_fn = semantic_member_type_before_at_cycle_recover
)]
pub(crate) fn semantic_member_type_before_at(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    member: SemanticId,
    offset: rowan::TextSize,
) -> LuaType {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return LuaType::Unknown;
    };
    model.type_of_member_before_at_impl(&member, offset)
}

fn semantic_member_type_before_at_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _member: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    LuaType::Unknown
}

fn semantic_member_type_before_at_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &LuaType,
    value: LuaType,
    _file: SourceFileInput,
    _config: ConfigInput,
    _member: SemanticId,
    _offset: rowan::TextSize,
) -> LuaType {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_member_infos_cycle_initial,
    cycle_fn = semantic_member_infos_cycle_recover
)]
pub(crate) fn semantic_member_infos(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    prefix_type: LuaType,
) -> Vec<super::member::MemberInfo> {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return Vec::new();
    };
    super::member::member_infos(&model, &prefix_type)
}

fn semantic_member_infos_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _prefix_type: LuaType,
) -> Vec<super::member::MemberInfo> {
    Vec::new()
}

fn semantic_member_infos_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &Vec<super::member::MemberInfo>,
    value: Vec<super::member::MemberInfo>,
    _file: SourceFileInput,
    _config: ConfigInput,
    _prefix_type: LuaType,
) -> Vec<super::member::MemberInfo> {
    value
}

#[salsa::tracked(
    returns(clone),
    cycle_initial = semantic_member_info_cycle_initial,
    cycle_fn = semantic_member_info_cycle_recover
)]
pub(crate) fn semantic_member_info(
    db: &dyn SalsaDb,
    file: SourceFileInput,
    _config: ConfigInput,
    prefix_type: LuaType,
    key: LuaMemberKey,
) -> Option<super::member::MemberInfo> {
    let Some(model) = SemanticModel::new(db.database(), file.file_id(db)) else {
        return None;
    };
    super::member::member_info_impl(&model, &prefix_type, &key)
}

fn semantic_member_info_cycle_initial(
    _db: &dyn SalsaDb,
    _id: salsa::Id,
    _file: SourceFileInput,
    _config: ConfigInput,
    _prefix_type: LuaType,
    _key: LuaMemberKey,
) -> Option<super::member::MemberInfo> {
    None
}

fn semantic_member_info_cycle_recover(
    _db: &dyn SalsaDb,
    _cycle: &salsa::Cycle,
    _last: &Option<super::member::MemberInfo>,
    value: Option<super::member::MemberInfo>,
    _file: SourceFileInput,
    _config: ConfigInput,
    _prefix_type: LuaType,
    _key: LuaMemberKey,
) -> Option<super::member::MemberInfo> {
    value
}
