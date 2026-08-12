use std::sync::Arc;

use crate::{DbIndex, LuaFunctionType, LuaType, LuaUnionType, get_real_type};

pub fn union_type(db: &DbIndex, source: LuaType, target: LuaType) -> LuaType {
    let match_source = get_real_type(db, &source)
        .cloned()
        .unwrap_or_else(|| source.clone());
    canonicalize_callable_union(db, union_type_impl(&match_source, source, target))
}

/// Union a batch of types with the same semantics as repeated `union_type`.
///
/// Empty batches return `Never`.
pub fn union_type_all<I>(db: &DbIndex, types: I) -> LuaType
where
    I: IntoIterator<Item = LuaType>,
{
    let mut result_types = Vec::new();
    for typ in types {
        match typ {
            LuaType::Never => {}
            LuaType::Any => return LuaType::Any,
            _ => result_types.push(typ),
        }
    }
    if result_types.is_empty() {
        return LuaType::Never;
    }
    // `LuaType::from_vec` only does structural normalization. Use it only when
    // no pairwise union rule, alias lookup, or callable canonicalization can matter.
    if can_use_structural_union(&result_types) {
        return LuaType::from_vec(result_types);
    }

    let mut result = LuaType::Never;
    for typ in result_types {
        result = union_type(db, result, typ);
    }
    result
}

pub(crate) fn union_type_shallow(source: LuaType, target: LuaType) -> LuaType {
    let match_source = source.clone();
    union_type_impl(&match_source, source, target)
}

/// Return true when `LuaType::from_vec` is enough to match `Union.apply` folding.
///
/// This is a conservative whole-batch check. We can reject early when a member
/// needs semantic handling (`Ref`, callable variants), but we cannot
/// accept early because most union rules depend on pairs that may appear later:
/// `number | integer`, `string | "x"`, `true | false`, and `table | table const`.
fn can_use_structural_union(types: &[LuaType]) -> bool {
    let mut state = StructuralUnionState::default();
    types.iter().all(|typ| state.add(typ))
}

#[derive(Default)]
struct StructuralUnionState {
    has_number: bool,
    has_number_variant: bool,
    has_integer: bool,
    has_integer_const: bool,
    has_string: bool,
    has_string_const: bool,
    has_boolean: bool,
    boolean_const_count: usize,
    has_table: bool,
    has_table_const: bool,
}

impl StructuralUnionState {
    fn add(&mut self, typ: &LuaType) -> bool {
        match typ {
            LuaType::Union(union) => return union.all_members(|typ| self.add(typ)),
            LuaType::Ref(_)
            | LuaType::MultiLineUnion(_)
            | LuaType::DocFunction(_)
            | LuaType::Signature(_) => return false,
            LuaType::Number => self.has_number = true,
            LuaType::Integer => {
                self.has_number_variant = true;
                self.has_integer = true;
            }
            LuaType::IntegerConst(_) => {
                self.has_number_variant = true;
                self.has_integer_const = true;
            }
            LuaType::FloatConst(_) => {
                self.has_number_variant = true;
            }
            LuaType::DocIntegerConst(_) => {
                self.has_number_variant = true;
                self.has_integer_const = true;
            }
            LuaType::String => self.has_string = true,
            LuaType::StringConst(_) | LuaType::DocStringConst(_) => self.has_string_const = true,
            LuaType::Boolean => self.has_boolean = true,
            LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => self.boolean_const_count += 1,
            LuaType::Table => self.has_table = true,
            LuaType::TableConst(_) => self.has_table_const = true,
            _ => {}
        }

        !(self.has_number && self.has_number_variant
            || self.has_integer && self.has_integer_const
            || self.has_string && self.has_string_const
            || self.has_boolean && self.boolean_const_count > 0
            || self.boolean_const_count > 1
            || self.has_table && self.has_table_const)
    }
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
            let mut types = left.into_vec();
            if types.contains(right) {
                return source.clone();
            }

            types.push(right.clone());
            LuaType::Union(LuaUnionType::from_vec(types).into())
        }
        (left, LuaType::Union(right)) if !left.is_union() => {
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

    let members = union.into_vec();
    if !members
        .iter()
        .any(|ty| matches!(ty, LuaType::DocFunction(_) | LuaType::Signature(_)))
    {
        return LuaType::from_vec(members);
    }

    let mut types = Vec::new();
    for member in members {
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
