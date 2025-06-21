use super::{get_buildin_type_map_type_id, RawGetMemberTypeResult};
use crate::semantic::check_type_compact;
use crate::{
    DbIndex, InferFailReason, InferGuard, LuaMemberKey, LuaMemberOwner, LuaObjectType,
    LuaTupleType, LuaType, LuaTypeDeclId, TypeOps,
};
use smol_str::SmolStr;
use std::sync::Arc;

#[allow(unused)]
pub fn infer_raw_member_type(
    db: &DbIndex,
    prefix_type: &LuaType,
    member_key: &LuaMemberKey,
) -> RawGetMemberTypeResult {
    infer_raw_member_type_guard(db, prefix_type, member_key, &mut InferGuard::new())
}

fn infer_raw_member_type_guard(
    db: &DbIndex,
    prefix_type: &LuaType,
    member_key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> RawGetMemberTypeResult {
    match prefix_type {
        LuaType::Table | LuaType::Any | LuaType::Unknown => Ok(LuaType::Any),
        LuaType::TableConst(id) => {
            let owner = LuaMemberOwner::Element(id.clone());
            infer_owner_raw_member_type(db, owner, member_key)
        }
        LuaType::String | LuaType::Io | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            let decl_id =
                get_buildin_type_map_type_id(&prefix_type).ok_or(InferFailReason::None)?;
            let owner = LuaMemberOwner::Type(decl_id);
            infer_owner_raw_member_type(db, owner, member_key)
        }
        LuaType::Ref(type_id) => {
            infer_custom_type_raw_member_type(db, type_id, member_key, infer_guard)
        }
        LuaType::Def(type_id) => {
            infer_custom_type_raw_member_type(db, type_id, member_key, infer_guard)
        }
        LuaType::Tuple(tuple) => infer_tuple_raw_member_type(tuple, member_key),
        LuaType::Object(object) => infer_object_raw_member_type(object, member_key),
        LuaType::Array(array_type) => infer_array_member_type(db, array_type, member_key),
        LuaType::TableGeneric(table_generic) => {
            infer_table_generic_member_type(db, table_generic, member_key)
        }
        // other do not support now
        _ => Err(InferFailReason::None),
    }
}

fn infer_table_generic_member_type(
    db: &DbIndex,
    table_params: &Arc<Vec<LuaType>>,
    member_key: &LuaMemberKey,
) -> RawGetMemberTypeResult {
    if table_params.len() != 2 {
        return Err(InferFailReason::None);
    }
    let key_type = &table_params[0];
    let value_type = &table_params[1];
    let access_key_type = match member_key {
        LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
        LuaMemberKey::Name(name) => LuaType::StringConst(SmolStr::new(name.as_str()).into()),
        _ => {
            return Err(InferFailReason::None);
        }
    };
    if check_type_compact(db, key_type, &access_key_type).is_ok() {
        return Ok(value_type.clone());
    }

    Err(InferFailReason::None)
}

fn infer_array_member_type(
    db: &DbIndex,
    array_type: &LuaType,
    member_key: &LuaMemberKey,
) -> RawGetMemberTypeResult {
    let expression_type = if db.get_emmyrc().strict.array_index {
        TypeOps::Union.apply(db, array_type, &LuaType::Nil)
    } else {
        array_type.clone()
    };
    match member_key {
        LuaMemberKey::Integer(_) => Ok(expression_type),
        _ => Err(InferFailReason::None),
    }
}

fn infer_owner_raw_member_type(
    db: &DbIndex,
    member_owner: LuaMemberOwner,
    member_key: &LuaMemberKey,
) -> RawGetMemberTypeResult {
    let member_item = db
        .get_member_index()
        .get_member_item(&member_owner, member_key)
        .ok_or(InferFailReason::FieldNotFound)?;
    member_item.resolve_type(db)
}

fn infer_custom_type_raw_member_type(
    db: &DbIndex,
    type_id: &LuaTypeDeclId,
    member_key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> RawGetMemberTypeResult {
    infer_guard.check(type_id)?;
    let type_index = db.get_type_index();
    let type_decl = type_index
        .get_type_decl(&type_id)
        .ok_or(InferFailReason::None)?;
    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
            return infer_raw_member_type_guard(db, &origin_type, member_key, infer_guard);
        } else {
            return Err(InferFailReason::None);
        }
    }

    let owner = LuaMemberOwner::Type(type_id.clone());
    if let Some(member_item) = db.get_member_index().get_member_item(&owner, member_key) {
        return member_item.resolve_type(db);
    }

    if type_decl.is_class() {
        if let Some(super_types) = type_index.get_super_types(&type_id) {
            for super_type in super_types {
                let result = infer_raw_member_type_guard(db, &super_type, member_key, infer_guard);

                match result {
                    Ok(member_type) => {
                        return Ok(member_type);
                    }
                    Err(InferFailReason::FieldNotFound) => {}
                    Err(err) => return Err(err),
                }
            }
        }
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_tuple_raw_member_type(
    tuple: &LuaTupleType,
    member_key: &LuaMemberKey,
) -> RawGetMemberTypeResult {
    if let LuaMemberKey::Integer(i) = &member_key {
        let i = *i;
        let index = if i > 0 { i - 1 } else { 0 };
        return match tuple.get_type(index as usize) {
            Some(typ) => Ok(typ.clone()),
            None => Err(InferFailReason::FieldNotFound),
        };
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_object_raw_member_type(
    object: &LuaObjectType,
    member_key: &LuaMemberKey,
) -> RawGetMemberTypeResult {
    if let Some(member_type) = object.get_field(&member_key) {
        return Ok(member_type.clone());
    }

    // donot support now
    // let index_accesses = object.get_index_access();
    // for (key, value) in index_accesses {
    //     let result = infer_index_metamethod(db, &member_key, &key, value);
    //     match result {
    //         Ok(typ) => {
    //             return Ok(typ);
    //         }
    //         Err(InferFailReason::FieldNotFound) => {}
    //         Err(err) => {
    //             return Err(err);
    //         }
    //     }
    // }

    Err(InferFailReason::FieldNotFound)
}
