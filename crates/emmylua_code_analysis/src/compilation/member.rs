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
    let index = db.get_type_def_reverse_index();
    index.by_name.get(&type_name)
        .and_then(|defs| defs.first())
        .map(|(_, def)| def.kind.clone())
}

pub(crate) fn type_def_is_class(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> bool {
    get_type_def_kind(db, type_decl_id) == Some(SalsaDocTypeDefKindSummary::Class)
}

pub(crate) fn type_def_is_alias(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> bool {
    get_type_def_kind(db, type_decl_id) == Some(SalsaDocTypeDefKindSummary::Alias)
}

pub(crate) fn type_def_is_enum(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> bool {
    get_type_def_kind(db, type_decl_id) == Some(SalsaDocTypeDefKindSummary::Enum)
}

pub(crate) fn type_def_alias_origin(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
) -> Option<LuaType> {
    let type_name: SmolStr = type_decl_id.get_name().into();
    let index = db.get_type_def_reverse_index();
    for (file_id, type_def) in index.by_name.get(&type_name)? {
        if type_def.kind != SalsaDocTypeDefKindSummary::Alias {
            continue;
        }
        let value_type_offset = type_def.value_type_offset?;
        let origin = infer_compilation_doc_type_key_with_owner(
            db,
            *file_id,
            Some(type_def.syntax_offset),
            value_type_offset,
        )?;
        return Some(origin);
    }
    None
}
