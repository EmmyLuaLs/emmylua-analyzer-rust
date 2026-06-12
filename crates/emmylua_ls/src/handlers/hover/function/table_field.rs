use std::collections::HashMap;

use emmylua_code_analysis::{
    DbIndex, LuaSemanticDeclId, LuaType, LuaTypeDeclId, TypeSubstitutor, infer_table_should_be,
    instantiate_type_generic,
};
use emmylua_parser::{LuaAstNode, LuaTableExpr, LuaTableField};

use crate::handlers::hover::HoverBuilder;

use super::{
    HoverFunctionInfo, function_member_is_field, get_function_description, process_function_type,
    set_builder_contents,
};

type OwnerSubstitutorCache = HashMap<LuaTypeDeclId, Option<TypeSubstitutor>>;

pub(super) fn build_table_field_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
    function_name: &str,
    is_local: bool,
) -> Option<()> {
    let token = builder.get_trigger_token()?;
    let table_field = token.parent().and_then(LuaTableField::cast)?;
    let table_expr = table_field.get_parent::<LuaTableExpr>()?;
    let parent_table_type = infer_table_should_be(
        db,
        &mut builder.semantic_model.get_cache().borrow_mut(),
        table_expr,
    )
    .ok()?;

    let is_field = function_member_is_field(db, semantic_decls);
    let mut function_infos = Vec::new();
    let mut substitutor_cache = OwnerSubstitutorCache::new();
    for (semantic_decl, typ) in semantic_decls {
        let typ = resolve_semantic_decl_type(
            db,
            semantic_decl,
            typ,
            &parent_table_type,
            &mut substitutor_cache,
        );
        let function_member = match semantic_decl {
            LuaSemanticDeclId::Member(id) => db.get_member_index().get_member(id),
            _ => None,
        };

        let Some(contents) = process_function_type(
            builder,
            db,
            &typ,
            function_member,
            function_name,
            is_local,
            is_field,
        ) else {
            continue;
        };
        if contents.is_empty() {
            continue;
        }

        let description = get_function_description(builder, db, semantic_decl);
        function_infos.push(HoverFunctionInfo {
            primary: contents.first()?.clone(),
            overloads: if contents.len() > 1 {
                Some(contents[1..].to_vec())
            } else {
                None
            },
            description,
        });
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

    match substitutor {
        Some(substitutor) => instantiate_type_generic(db, typ, &substitutor),
        None => typ.clone(),
    }
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

    let substitutor = generic_substitutor_for_owner(db, parent_table_type, &owner_type_id)
        .or_else(|| unknown_substitutor_for_owner(db, &owner_type_id));
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

fn generic_substitutor_for_owner(
    db: &DbIndex,
    typ: &LuaType,
    owner_type_id: &LuaTypeDeclId,
) -> Option<TypeSubstitutor> {
    match typ {
        LuaType::Generic(generic) => {
            if generic.get_base_type_id_ref() == owner_type_id {
                Some(TypeSubstitutor::from_type_array(
                    generic.get_params().clone(),
                ))
            } else {
                None
            }
        }
        LuaType::Ref(id) | LuaType::Def(id) => {
            if id == owner_type_id {
                unknown_substitutor_for_owner(db, owner_type_id)
            } else {
                None
            }
        }
        LuaType::Union(union) => {
            let mut substitutor = None;
            for typ in union.into_vec() {
                let Some(generic_substitutor) =
                    generic_substitutor_for_owner(db, &typ, owner_type_id)
                else {
                    continue;
                };
                if substitutor.is_some() {
                    return None;
                }
                substitutor = Some(generic_substitutor);
            }
            substitutor
        }
        _ => None,
    }
}

fn unknown_substitutor_for_owner(
    db: &DbIndex,
    owner_type_id: &LuaTypeDeclId,
) -> Option<TypeSubstitutor> {
    let generic_params = db.get_type_index().get_generic_params(owner_type_id)?;
    if generic_params.is_empty() {
        return None;
    }
    Some(TypeSubstitutor::from_type_array(vec![
        LuaType::Unknown;
        generic_params.len()
    ]))
}
