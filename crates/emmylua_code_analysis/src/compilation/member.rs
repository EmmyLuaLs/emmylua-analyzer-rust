use crate::{
    DbIndex, LuaType, LuaTypeDeclId,
    compilation::decl::infer_compilation_doc_type_key_with_owner,
    SalsaDocTypeDefKindSummary,
};
use smol_str::SmolStr;

pub(crate) fn get_type_def_kind(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
) -> Option<SalsaDocTypeDefKindSummary> {
    let type_name: SmolStr = type_decl_id.get_name().into();
    for file_id in db.get_vfs().get_all_file_ids() {
        if let Some(type_def) = db
            .get_summary_db()
            .doc()
            .type_def_by_name(file_id, type_name.clone())
        {
            return Some(type_def.kind);
        }
    }
    None
}

pub(crate) fn type_def_is_class(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
) -> bool {
    get_type_def_kind(db, type_decl_id)
        == Some(SalsaDocTypeDefKindSummary::Class)
}

pub(crate) fn type_def_is_alias(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
) -> bool {
    get_type_def_kind(db, type_decl_id)
        == Some(SalsaDocTypeDefKindSummary::Alias)
}

pub(crate) fn type_def_is_enum(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
) -> bool {
    get_type_def_kind(db, type_decl_id)
        == Some(SalsaDocTypeDefKindSummary::Enum)
}

pub(crate) fn type_def_alias_origin(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
) -> Option<LuaType> {
    let type_name: SmolStr = type_decl_id.get_name().into();
    for file_id in db.get_vfs().get_all_file_ids() {
        let type_def = db
            .get_summary_db()
            .doc()
            .type_def_by_name(file_id, type_name.clone())?;
        if type_def.kind != SalsaDocTypeDefKindSummary::Alias {
            continue;
        }
        let value_type_offset = type_def.value_type_offset?;
        let origin = infer_compilation_doc_type_key_with_owner(
            db,
            file_id,
            Some(type_def.syntax_offset),
            value_type_offset,
        )?;
        return Some(origin);
    }
    None
}