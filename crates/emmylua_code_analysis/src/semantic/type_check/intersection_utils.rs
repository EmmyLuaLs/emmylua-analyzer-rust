use std::collections::HashMap;

use crate::{DbIndex, LuaIntersectionType, LuaObjectType, LuaType, semantic::member::find_members};

pub(super) fn intersection_to_object(
    db: &DbIndex,
    intersection: &LuaIntersectionType,
) -> Option<LuaObjectType> {
    let intersection_type: LuaType = intersection.clone().into();
    let members = find_members(db, &intersection_type)?;
    let mut fields: HashMap<_, _> = HashMap::new();
    for member in members {
        fields
            .entry(member.key.clone())
            .or_insert(member.typ.clone());
    }
    Some(LuaObjectType::new_with_fields(fields, Vec::new()))
}
