use std::collections::HashSet;

use crate::{DbIndex, InferGuard, LuaType, LuaTypeDeclId};

pub fn is_sub_type_of(
    db: &DbIndex,
    sub_type_ref_id: &LuaTypeDeclId,
    super_type_ref_id: &LuaTypeDeclId,
) -> bool {
    // let mut infer_guard = InferGuard::new();
    // check_sub_type_of(db, sub_type_ref_id, super_type_ref_id, &mut infer_guard).unwrap_or(false)
    check_sub_type_of_iterative(db, sub_type_ref_id, super_type_ref_id).unwrap_or(false)
}

fn check_sub_type_of_iterative(
    db: &DbIndex,
    sub_type_ref_id: &LuaTypeDeclId,
    super_type_ref_id: &LuaTypeDeclId,
) -> Option<bool> {
    if sub_type_ref_id == super_type_ref_id {
        return Some(true);
    }

    let type_index = db.get_type_index();
    let mut stack: Vec<LuaTypeDeclId> = Vec::new();
    let mut visited: HashSet<LuaTypeDeclId> = HashSet::new();

    stack.push(sub_type_ref_id.clone());

    while let Some(current_id) = stack.pop() {
        if !visited.insert(current_id.clone()) {
            continue;
        }

        let supers = match type_index.get_super_types(&current_id) {
            Some(s) => s,
            None => continue,
        };

        for super_type in supers {
            match &super_type {
                LuaType::Ref(super_id) => {
                    // TODO: 不相等时可以判断必要字段是否全部匹配, 如果匹配则认为相等
                    if super_id == super_type_ref_id {
                        return Some(true);
                    }
                    if !visited.contains(super_id) {
                        stack.push(super_id.clone());
                    }
                }
                _ => {
                    if let Some(base_id) = get_base_type_id(&super_type) {
                        if base_id == *super_type_ref_id {
                            return Some(true);
                        }
                    }
                }
            }
        }
    }

    Some(false)
}

#[allow(unused)]
fn check_sub_type_of(
    db: &DbIndex,
    sub_type_ref_id: &LuaTypeDeclId,
    super_type_ref_id: &LuaTypeDeclId,
    infer_guard: &mut InferGuard,
) -> Option<bool> {
    infer_guard.check(super_type_ref_id).ok()?;

    let supers = db.get_type_index().get_super_types(sub_type_ref_id)?;
    // dbg!(sub_type_ref_id, super_type_ref_id, &supers);
    for super_type in supers {
        if let LuaType::Ref(super_id) = &super_type {
            if super_id == super_type_ref_id {
                return Some(true);
            }

            if check_sub_type_of(db, sub_type_ref_id, &super_id, infer_guard).unwrap_or(false) {
                return Some(true);
            }
        }
        if let Some(super_base_type_id) = get_base_type_id(&super_type) {
            if super_base_type_id == *super_type_ref_id {
                return Some(true);
            }
        }
    }

    Some(false)
}

pub fn get_base_type_id(typ: &LuaType) -> Option<LuaTypeDeclId> {
    if typ.is_integer() {
        return Some(LuaTypeDeclId::new("integer"));
    } else if typ.is_number() {
        return Some(LuaTypeDeclId::new("number"));
    } else if typ.is_boolean() {
        return Some(LuaTypeDeclId::new("boolean"));
    } else if typ.is_string() {
        return Some(LuaTypeDeclId::new("string"));
    } else if typ.is_table() {
        return Some(LuaTypeDeclId::new("table"));
    } else if typ.is_function() {
        return Some(LuaTypeDeclId::new("function"));
    } else if typ.is_thread() {
        return Some(LuaTypeDeclId::new("thread"));
    } else if typ.is_userdata() {
        return Some(LuaTypeDeclId::new("userdata"));
    } else if typ.is_io() {
        return Some(LuaTypeDeclId::new("io"));
    } else if typ.is_global() {
        return Some(LuaTypeDeclId::new("global"));
    } else if typ.is_self_infer() {
        return Some(LuaTypeDeclId::new("self"));
    } else if typ.is_nil() {
        return Some(LuaTypeDeclId::new("nil"));
    }
    None
}
