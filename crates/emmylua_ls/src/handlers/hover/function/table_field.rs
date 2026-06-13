use std::collections::HashMap;

use emmylua_code_analysis::{DbIndex, LuaSemanticDeclId, LuaType, LuaTypeDeclId, TypeSubstitutor};

use crate::handlers::hover::{HoverBuilder, HoverDeclContext};

use super::{
    define_hover::{HoverFunctionInfo, set_builder_contents},
    extract_function_member,
    generic::{instantiate_type_if_needed, owner_type_substitutor, unknown_type_substitutor},
    get_function_description,
    render::process_function_type,
};

type OwnerSubstitutorCache = HashMap<LuaTypeDeclId, Option<TypeSubstitutor>>;

pub(super) fn build_table_field_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    decl_context: &HoverDeclContext,
    parent_table_type: &LuaType,
) -> Option<()> {
    let mut function_infos = Vec::new();
    let mut substitutor_cache = OwnerSubstitutorCache::new();
    for decl_info in decl_context.ordered_decl_refs() {
        let semantic_decl_id = decl_info.id();
        let typ = resolve_semantic_decl_type(
            db,
            semantic_decl_id,
            decl_info.typ(),
            parent_table_type,
            &mut substitutor_cache,
        );
        let function_member = extract_function_member(db, semantic_decl_id);

        let Some(contents) =
            process_function_type(builder, db, &typ, semantic_decl_id, function_member)
        else {
            continue;
        };
        if contents.is_empty() {
            continue;
        }

        let description = get_function_description(builder, db, semantic_decl_id);
        if let Some(info) = HoverFunctionInfo::from_contents(contents, description) {
            function_infos.push(info);
        }
    }

    set_builder_contents(builder, &mut function_infos)
}

fn resolve_semantic_decl_type(
    db: &DbIndex,
    semantic_decl: &LuaSemanticDeclId,
    typ: &LuaType,
    parent_table_type: &LuaType,
    substitutor_cache: &mut OwnerSubstitutorCache,
) -> LuaType {
    if !typ.contain_tpl() {
        return typ.clone();
    }

    let Some(owner_type_id) = semantic_decl_owner_type_id(db, semantic_decl) else {
        return typ.clone();
    };
    let substitutor =
        cached_substitutor_for_owner(db, parent_table_type, owner_type_id, substitutor_cache);

    substitutor
        .and_then(|substitutor| instantiate_type_if_needed(db, typ, &substitutor))
        .unwrap_or_else(|| typ.clone())
}

fn cached_substitutor_for_owner(
    db: &DbIndex,
    parent_table_type: &LuaType,
    owner_type_id: LuaTypeDeclId,
    substitutor_cache: &mut OwnerSubstitutorCache,
) -> Option<TypeSubstitutor> {
    if let Some(substitutor) = substitutor_cache.get(&owner_type_id) {
        return substitutor.clone();
    }

    let substitutor = owner_type_substitutor(db, parent_table_type, &owner_type_id)
        .or_else(|| unknown_type_substitutor(db, &owner_type_id));
    substitutor_cache.insert(owner_type_id, substitutor.clone());
    substitutor
}

fn semantic_decl_owner_type_id(
    db: &DbIndex,
    semantic_decl: &LuaSemanticDeclId,
) -> Option<LuaTypeDeclId> {
    match semantic_decl {
        LuaSemanticDeclId::Member(id) => db
            .get_member_index()
            .get_current_owner(id)?
            .get_type_id()
            .cloned(),
        _ => None,
    }
}
