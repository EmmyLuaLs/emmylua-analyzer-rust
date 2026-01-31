use std::ops::Deref;

use crate::{DbIndex, LuaType, get_real_type};

pub fn intersect_type(db: &DbIndex, source: LuaType, target: LuaType) -> LuaType {
    let real_type = get_real_type(db, &source).unwrap_or(&source);

    match (&real_type, &target) {
        // ANY & T = T
        (LuaType::Any, _) => target.clone(),
        (_, LuaType::Any) => real_type.clone(),
        (LuaType::Never, _) => LuaType::Never,
        (_, LuaType::Never) => LuaType::Never,
        (LuaType::Unknown, _) => target,
        (_, LuaType::Unknown) => source,
        // int | int const
        (LuaType::Integer, LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i)) => {
            LuaType::IntegerConst(*i)
        }
        (LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i), LuaType::Integer) => {
            LuaType::IntegerConst(*i)
        }
        // float | float const
        (LuaType::Number, right) if right.is_number() => LuaType::Number,
        (left, LuaType::Number) if left.is_number() => LuaType::Number,
        // string | string const
        (LuaType::String, LuaType::StringConst(s) | LuaType::DocStringConst(s)) => {
            LuaType::StringConst(s.clone())
        }
        (LuaType::StringConst(s) | LuaType::DocStringConst(s), LuaType::String) => {
            LuaType::StringConst(s.clone())
        }
        // boolean | boolean const
        (LuaType::Boolean, LuaType::BooleanConst(b)) => LuaType::BooleanConst(*b),
        (LuaType::BooleanConst(b), LuaType::Boolean) => LuaType::BooleanConst(*b),
        (LuaType::BooleanConst(left), LuaType::BooleanConst(right)) => {
            if left == right {
                LuaType::BooleanConst(*left)
            } else {
                LuaType::Never
            }
        }
        // table | table const
        (LuaType::Table, LuaType::TableConst(t)) => LuaType::TableConst(t.clone()),
        (LuaType::TableConst(t), LuaType::Table) => LuaType::TableConst(t.clone()),
        // function | function const
        (LuaType::Function, LuaType::DocFunction(_) | LuaType::Signature(_)) => target.clone(),
        (LuaType::DocFunction(_) | LuaType::Signature(_), LuaType::Function) => real_type.clone(),
        // class references
        (LuaType::Ref(id1), LuaType::Ref(id2)) => {
            if id1 == id2 {
                source.clone()
            } else {
                LuaType::Never
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
        // union ∩ non-union: (A | B) ∩ C = (A ∩ C) | (B ∩ C)
        (LuaType::Union(left), right) if !right.is_union() => {
            let left_types = left.deref().clone().into_vec();
            let mut result_types = Vec::new();

            for left_type in left_types {
                let intersected = intersect_type(db, left_type, right.clone());
                if !matches!(intersected, LuaType::Never) {
                    result_types.push(intersected);
                }
            }

            if result_types.is_empty() {
                LuaType::Never
            } else {
                LuaType::from_vec(result_types)
            }
        }
        // non-union ∩ union: A ∩ (B | C) = (A ∩ B) | (A ∩ C)
        (left, LuaType::Union(right)) if !left.is_union() => {
            let right_types = right.deref().clone().into_vec();
            let mut result_types = Vec::new();

            for right_type in right_types {
                let intersected = intersect_type(db, real_type.clone(), right_type);
                if !matches!(intersected, LuaType::Never) {
                    result_types.push(intersected);
                }
            }

            if result_types.is_empty() {
                LuaType::Never
            } else {
                LuaType::from_vec(result_types)
            }
        }
        // union ∩ union: (A | B) ∩ (C | D) = (A ∩ C) | (A ∩ D) | (B ∩ C) | (B ∩ D)
        (LuaType::Union(left), LuaType::Union(right)) => {
            let left_types = left.deref().clone().into_vec();
            let right_types = right.deref().clone().into_vec();
            let mut result_types = Vec::new();

            for left_type in left_types {
                for right_type in &right_types {
                    let intersected = intersect_type(db, left_type.clone(), right_type.clone());
                    if !matches!(intersected, LuaType::Never) {
                        result_types.push(intersected);
                    }
                }
            }

            if result_types.is_empty() {
                LuaType::Never
            } else {
                LuaType::from_vec(result_types)
            }
        }

        // same type
        (left, right) if *left == right => source.clone(),
        _ => LuaType::Never,
    }
}
