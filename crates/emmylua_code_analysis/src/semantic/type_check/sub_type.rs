use hashbrown::HashSet;

use crate::{DbIndex, LuaType, LuaTypeDeclId, LuaTypeIdentifier};

/// 检查子类型关系.
///
/// 假设现在有一个类型定义`---@class C: A, B`, 那么`sub_type_ref_id`为`C`, `super_type_ref_id`可以为`A`或`B`.
pub fn is_sub_type_of(
    db: &DbIndex,
    sub_type_ref_id: &LuaTypeDeclId,
    super_type_ref_id: &LuaTypeDeclId,
) -> bool {
    check_sub_type_of_iterative(db, sub_type_ref_id, super_type_ref_id)
}

fn check_sub_type_of_iterative(
    db: &DbIndex,
    sub_type_ref_id: &LuaTypeDeclId,
    super_type_ref_id: &LuaTypeDeclId,
) -> bool {
    if sub_type_ref_id == super_type_ref_id {
        return true;
    }

    let type_index = db.get_type_index();
    let mut stack = Vec::with_capacity(4);
    let mut visited = HashSet::with_capacity(4);

    stack.push(sub_type_ref_id);
    visited.insert(sub_type_ref_id);
    while let Some(current_id) = stack.pop() {
        let supers_iter = match type_index.get_super_types_iter(current_id) {
            Some(iter) => iter,
            None => continue,
        };

        for super_type in supers_iter {
            match super_type {
                LuaType::Ref(super_id) => {
                    // TODO: 不相等时可以判断必要字段是否全部匹配, 如果匹配则认为相等
                    if super_id == super_type_ref_id {
                        return true;
                    }
                    if visited.insert(super_id) {
                        stack.push(super_id);
                    }
                }
                // TODO: 应该检查泛型参数是否匹配
                LuaType::Generic(generic) => {
                    let base_type_id = generic.get_base_type_id_ref();
                    if base_type_id == super_type_ref_id {
                        return true;
                    }
                    if visited.insert(base_type_id) {
                        stack.push(base_type_id);
                    }
                }
                _ => {
                    if is_base_type_id(super_type, super_type_ref_id) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

pub fn get_base_type_id(typ: &LuaType) -> Option<LuaTypeDeclId> {
    base_type_name(typ).map(LuaTypeDeclId::global)
}

fn is_base_type_id(typ: &LuaType, type_id: &LuaTypeDeclId) -> bool {
    let LuaTypeIdentifier::Global(type_name) = type_id.get_id() else {
        return false;
    };
    let type_name: &str = type_name.as_ref();

    base_type_name(typ).is_some_and(|base_name| base_name == type_name)
}

fn base_type_name(typ: &LuaType) -> Option<&'static str> {
    match typ {
        LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
            Some("integer")
        }
        LuaType::Number | LuaType::FloatConst(_) => Some("number"),
        LuaType::Boolean | LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => {
            Some("boolean")
        }
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => Some("string"),
        LuaType::Table
        | LuaType::TableGeneric(_)
        | LuaType::TableConst(_)
        | LuaType::Tuple(_)
        | LuaType::Array(_)
        | LuaType::Object(_) => Some("table"),
        LuaType::Intersection(intersection) => {
            intersection.get_types().iter().find_map(base_type_name)
        }
        LuaType::DocFunction(_) | LuaType::Function | LuaType::Signature(_) => Some("function"),
        LuaType::Thread => Some("thread"),
        LuaType::Userdata => Some("userdata"),
        LuaType::Io => Some("io"),
        LuaType::Global => Some("global"),
        LuaType::SelfInfer => Some("self"),
        LuaType::Nil => Some("nil"),
        _ => None,
    }
}
