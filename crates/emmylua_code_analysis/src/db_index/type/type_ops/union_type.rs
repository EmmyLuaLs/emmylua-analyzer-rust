use std::ops::Deref;

use crate::{LuaType, LuaUnionType};

pub fn union_type(source: LuaType, target: LuaType) -> LuaType {
    match (&source, &target) {
        // ANY | T = ANY
        (LuaType::Any, _) => LuaType::Any,
        (LuaType::Unknown, _) => target,
        (_, LuaType::Any | LuaType::Unknown) => source,
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
        // table | table const
        (LuaType::Table, LuaType::TableConst(_)) => LuaType::Table,
        (LuaType::TableConst(_), LuaType::Table) => LuaType::Table,
        // function | function const
        (LuaType::Function, LuaType::DocFunction(_) | LuaType::Signature(_)) => LuaType::Function,
        (LuaType::DocFunction(_) | LuaType::Signature(_), LuaType::Function) => LuaType::Function,
        // nullable
        (LuaType::Nullable(left), LuaType::Nil) => LuaType::Nullable(left.clone()),
        (LuaType::Nil, LuaType::Nullable(right)) => LuaType::Nullable(right.clone()),
        (LuaType::Nullable(left), right) if !right.is_optional() => {
            union_type(left.deref().clone(), right.clone())
        }
        (left, LuaType::Nullable(right)) if !left.is_optional() => {
            union_type(left.clone(), right.deref().clone())
        }
        // union
        (LuaType::Union(left), right) if !right.is_union() => {
            let left = left.deref().clone();
            let mut types = left
                .get_types()
                .iter()
                .map(|it| it.clone())
                .collect::<Vec<_>>();
            types.dedup();
            types.push(right.clone());

            LuaType::Union(LuaUnionType::new(types).into())
        }
        (left, LuaType::Union(right)) if !left.is_union() => {
            let right = right.deref().clone();
            let mut types = right
                .get_types()
                .iter()
                .map(|it| it.clone())
                .collect::<Vec<_>>();
            types.push(left.clone());
            types.dedup();
            LuaType::Union(LuaUnionType::new(types).into())
        }
        // two union
        (LuaType::Union(left), LuaType::Union(right)) => {
            let left = left.deref().clone();
            let right = right.deref().clone();
            let mut types = left
                .get_types()
                .iter()
                .map(|it| it.clone())
                .collect::<Vec<_>>();
            types.extend(right.get_types().iter().map(|it| it.clone()));
            types.dedup();
            LuaType::Union(LuaUnionType::new(types).into())
        }

        // same type
        (left, right) if left == right => source.clone(),
        _ => LuaType::Union(LuaUnionType::new(vec![source, target]).into()),
    }
}
