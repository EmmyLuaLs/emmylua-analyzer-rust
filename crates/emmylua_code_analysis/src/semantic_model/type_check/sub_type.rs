//! Subtype (inheritance) determination: iteratively traverses salsa `super_names`.
//!
//! `is_sub_type_of(sub, super)`: whether sub's type definition (including the inheritance chain) contains super.

use std::collections::HashSet;

use crate::{LuaType, LuaTypeDeclId};

use super::context::TypeCheckContext;

pub fn is_sub_type_of(
    context: &TypeCheckContext,
    sub: &LuaTypeDeclId,
    sup: &LuaTypeDeclId,
) -> bool {
    if sub == sup {
        return true;
    }
    let mut stack = vec![sub.clone()];
    let mut visited = HashSet::new();
    visited.insert(sub.clone());
    while let Some(current) = stack.pop() {
        for super_type in context.super_types_of(&current) {
            if let LuaType::Ref(super_id) = super_type {
                if &super_id == sup {
                    return true;
                }
                if visited.insert(super_id.clone()) {
                    stack.push(super_id);
                }
            }
        }
    }
    false
}

/// Base type name (`integer`/`string`/`table`/…) → global id.
pub fn get_base_type_id(typ: &LuaType) -> Option<LuaTypeDeclId> {
    base_type_name(typ).map(LuaTypeDeclId::global)
}

/// Base type name.
pub fn base_type_name(typ: &LuaType) -> Option<&'static str> {
    use crate::LuaType::*;
    match typ {
        Integer | IntegerConst(_) | DocIntegerConst(_) => Some("integer"),
        Number | FloatConst(_) => Some("number"),
        Boolean | BooleanConst(_) | DocBooleanConst(_) => Some("boolean"),
        String | StringConst(_) | DocStringConst(_) => Some("string"),
        Table | TableGeneric(_) | TableConst(_) | Tuple(_) | Array(_) | Object(_) => Some("table"),
        Intersection(intersection) => intersection.get_types().iter().find_map(base_type_name),
        DocFunction(_) | Function | Signature(_) => Some("function"),
        Thread => Some("thread"),
        Userdata => Some("userdata"),
        Io => Some("io"),
        Global => Some("global"),
        SelfInfer => Some("self"),
        Nil => Some("nil"),
        _ => None,
    }
}
