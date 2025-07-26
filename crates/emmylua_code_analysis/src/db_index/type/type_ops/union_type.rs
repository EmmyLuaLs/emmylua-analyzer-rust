use std::ops::Deref;

use crate::{DbIndex, LuaType, LuaUnionType, get_real_type};

pub fn union_type(db: &DbIndex, source: LuaType, target: LuaType) -> LuaType {
    let real_type = get_real_type(db, &source).unwrap_or(&source);

    match (&real_type, &target) {
        // ANY | T = ANY
        (LuaType::Any, _) => LuaType::Any,
        (_, LuaType::Any) => LuaType::Any,
        (LuaType::Never, _) => target,
        (_, LuaType::Never) => source,
        (LuaType::Unknown, _) => target,
        (_, LuaType::Unknown) => source,
        // int | int const
        (LuaType::Integer, LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)) => {
            LuaType::Integer
        }
        (LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_), LuaType::Integer) => {
            LuaType::Integer
        }
        // float | float const
        (LuaType::Number, right) if right.is_number() => LuaType::Number,
        (left, LuaType::Number) if left.is_number() => LuaType::Number,
        // string | string const
        (LuaType::String, LuaType::StringConst(_) | LuaType::DocStringConst(_)) => LuaType::String,
        (LuaType::StringConst(_) | LuaType::DocStringConst(_), LuaType::String) => LuaType::String,
        // boolean | boolean const
        (LuaType::Boolean, LuaType::BooleanConst(_)) => LuaType::Boolean,
        (LuaType::BooleanConst(_), LuaType::Boolean) => LuaType::Boolean,
        (LuaType::BooleanConst(left), LuaType::BooleanConst(right)) => {
            if left == right {
                LuaType::BooleanConst(left.clone())
            } else {
                LuaType::Boolean
            }
        }
        // table | table const
        (LuaType::Table, LuaType::TableConst(_)) => LuaType::Table,
        (LuaType::TableConst(_), LuaType::Table) => LuaType::Table,
        // function | function const
        (LuaType::Function, LuaType::DocFunction(_) | LuaType::Signature(_)) => LuaType::Function,
        (LuaType::DocFunction(_) | LuaType::Signature(_), LuaType::Function) => LuaType::Function,
        // class references
        (LuaType::Ref(id1), LuaType::Ref(id2)) => {
            if id1 == id2 {
                source.clone()
            } else {
                LuaType::from_vec(vec![source.clone(), target.clone()])
            }
        }
        (LuaType::MultiLineUnion(left), right) => {
            let include = match right {
                LuaType::StringConst(v) => {
                    left.get_unions().iter().any(|(t, _)| match (t, right) {
                        (LuaType::DocStringConst(a), _) => a == v,
                        _ => false,
                    })
                }
                LuaType::IntegerConst(v) => {
                    left.get_unions().iter().any(|(t, _)| match (t, right) {
                        (LuaType::DocIntegerConst(a), _) => a == v,
                        _ => false,
                    })
                }
                _ => false,
            };

            if include {
                return source;
            }
            LuaType::from_vec(vec![source, target])
        }
        // union
        (LuaType::Union(left), right) if !right.is_union() => {
            let left = left.deref().clone();
            let mut types = left.into_vec();
            if types.contains(right) {
                return source.clone();
            }

            types.push(right.clone());
            LuaType::Union(LuaUnionType::from_vec(types).into())
        }
        (left, LuaType::Union(right)) if !left.is_union() => {
            let right = right.deref().clone();
            let mut types = right.into_vec();
            if types.contains(left) {
                return target.clone();
            }

            types.push(source.clone());
            LuaType::Union(LuaUnionType::from_vec(types).into())
        }
        // two union
        (LuaType::Union(left), LuaType::Union(right)) => {
            let mut left = left.into_vec();
            let right = right.into_vec();
            left.extend(right);

            LuaType::from_vec(left)
        }

        // same type
        (left, right) if *left == right => source.clone(),
        _ => LuaType::from_vec(vec![source, target]),
    }
}
