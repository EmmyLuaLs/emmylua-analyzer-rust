use std::{ops::Deref, sync::Arc};

use crate::{DbIndex, LuaFunctionType, LuaType, LuaUnionType, get_real_type};

pub fn union_type(db: &DbIndex, source: LuaType, target: LuaType) -> LuaType {
    let match_source = get_real_type(db, &source)
        .cloned()
        .unwrap_or_else(|| source.clone());
    canonicalize_callable_union(db, union_type_impl(&match_source, source, target))
}

pub(crate) fn union_type_shallow(source: LuaType, target: LuaType) -> LuaType {
    let match_source = source.clone();
    union_type_impl(&match_source, source, target)
}

fn union_type_impl(match_source: &LuaType, source: LuaType, target: LuaType) -> LuaType {
    match (match_source, &target) {
        // ANY | T = ANY
        (LuaType::Any, _) => LuaType::Any,
        (_, LuaType::Any) => LuaType::Any,
        (LuaType::Never, _) => target,
        (_, LuaType::Never) => source,
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
        (LuaType::Boolean, right) if right.is_boolean() => LuaType::Boolean,
        (left, LuaType::Boolean) if left.is_boolean() => LuaType::Boolean,
        (
            LuaType::BooleanConst(left) | LuaType::DocBooleanConst(left),
            LuaType::BooleanConst(right) | LuaType::DocBooleanConst(right),
        ) => {
            if left == right {
                source.clone()
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
            if left == right {
                return source.clone();
            }

            let mut merged = left.into_vec();
            merged.extend(right.into_vec());

            LuaType::from_vec(merged)
        }

        // same type
        (left, right) if *left == *right => source.clone(),
        _ => LuaType::from_vec(vec![source, target]),
    }
}

// `Signature` and `DocFunction` carry the same callable shape through different variants.
// Collapse them after the normal union merge so the core merge logic stays simple.
fn canonicalize_callable_union(db: &DbIndex, ty: LuaType) -> LuaType {
    let LuaType::Union(union) = ty else {
        return ty;
    };

    let mut types = Vec::new();
    for member in union.into_vec() {
        let member_callable = as_callable_type(db, &member);
        if types.iter().any(|existing| {
            existing == &member
                || as_callable_type(db, existing)
                    .as_ref()
                    .zip(member_callable.as_ref())
                    .is_some_and(|(existing, member)| existing == member)
        }) {
            continue;
        }
        types.push(member);
    }

    LuaType::from_vec(types)
}

fn as_callable_type(db: &DbIndex, ty: &LuaType) -> Option<Arc<LuaFunctionType>> {
    match ty {
        LuaType::DocFunction(func) => Some(func.clone()),
        LuaType::Signature(signature_id) => db
            .get_signature_index()
            .get(signature_id)
            .filter(|signature| signature.is_resolve_return())
            .map(|signature| signature.to_doc_func_type()),
        _ => None,
    }
}
