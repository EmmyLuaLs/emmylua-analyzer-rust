mod false_or_nil_type;

use crate::{DbIndex, LuaType, TypeOps, get_real_type, semantic::type_check::is_sub_type_of};
pub use false_or_nil_type::{narrow_false_or_nil, remove_false_or_nil};

// need to be optimized
// `source` is the current/antecedent type, `target` is the narrowing candidate (e.g. assignment RHS).
pub fn narrow_down_type(db: &DbIndex, source: LuaType, target: LuaType) -> Option<LuaType> {
    if source == target {
        return Some(source);
    }

    let real_source_ref = get_real_type(db, &source).unwrap_or(&source);
    match &target {
        LuaType::Number => {
            if real_source_ref.is_number() {
                return Some(source);
            }
        }
        LuaType::Integer => {
            if real_source_ref.is_integer() {
                return Some(source);
            }
        }
        LuaType::String => {
            if real_source_ref.is_string() {
                return Some(source);
            }
        }
        LuaType::Boolean => {
            if real_source_ref.is_boolean() {
                return Some(source);
            }
        }
        LuaType::Table => match real_source_ref {
            LuaType::TableConst(_) => {
                return Some(source);
            }
            LuaType::Object(_) => {
                return Some(source);
            }
            LuaType::Table | LuaType::Userdata | LuaType::Any | LuaType::Unknown => {
                return Some(LuaType::Table);
            }
            // TODO: 应该根据模板约束进行精确匹配
            LuaType::TplRef(_) => return Some(source),
            LuaType::Global
            | LuaType::Array(_)
            | LuaType::Tuple(_)
            | LuaType::Generic(_)
            | LuaType::TableGeneric(_) => return Some(source),
            LuaType::Ref(type_decl_id) | LuaType::Def(type_decl_id) => {
                let type_decl = db.get_type_index().get_type_decl(type_decl_id)?;
                // enum 在实际使用时实际上是 enum.field, 并不等于 table
                if type_decl.is_enum() {
                    return None;
                }

                // 需要对`userdata`进行特殊处理
                if let Some(super_types) = db.get_type_index().get_super_types_iter(type_decl_id) {
                    for super_type in super_types {
                        if super_type.is_userdata() {
                            return None;
                        }
                    }
                }

                return Some(source);
            }
            _ => {}
        },
        LuaType::Function => {
            if real_source_ref.is_function() {
                return Some(source);
            }
        }
        LuaType::Signature(_) => {
            if real_source_ref.is_function() {
                // Prefer the assigned closure signature, even when the antecedent is a doc function.
                return Some(target.clone());
            }
        }
        LuaType::Thread => {
            if real_source_ref.is_thread() {
                return Some(source);
            }
        }
        LuaType::Userdata => {
            if real_source_ref.is_userdata() {
                return Some(source);
            }
        }
        LuaType::Nil => {
            if real_source_ref.is_nil() {
                return Some(source);
            }
        }
        LuaType::Any | LuaType::Unknown => {
            return Some(source);
        }
        LuaType::FloatConst(f) => {
            if real_source_ref.is_number() {
                return Some(LuaType::Number);
            } else if real_source_ref.is_unknown() {
                return Some(LuaType::FloatConst(*f));
            }
        }
        LuaType::IntegerConst(i) => match real_source_ref {
            LuaType::DocIntegerConst(i2) => {
                if i == i2 {
                    return Some(LuaType::IntegerConst(*i));
                }
            }
            LuaType::Number
            | LuaType::Integer
            | LuaType::Any
            | LuaType::Unknown
            | LuaType::IntegerConst(_) => {
                return Some(LuaType::Integer);
            }
            _ => {}
        },
        LuaType::StringConst(s) => match real_source_ref {
            LuaType::DocStringConst(s2) => {
                if s == s2 {
                    return Some(LuaType::DocStringConst(s.clone()));
                }
            }
            LuaType::String | LuaType::Any | LuaType::Unknown | LuaType::StringConst(_) => {
                return Some(LuaType::String);
            }
            _ => {}
        },
        LuaType::TableConst(t) => match real_source_ref {
            LuaType::TableConst(s) => {
                return Some(LuaType::TableConst(s.clone()));
            }
            LuaType::Table | LuaType::Userdata | LuaType::Any | LuaType::Unknown => {
                return Some(LuaType::TableConst(t.clone()));
            }
            LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::Global
            | LuaType::Array(_)
            | LuaType::Tuple(_)
            | LuaType::Generic(_)
            | LuaType::TableGeneric(_) => return Some(source),
            _ => {}
        },
        LuaType::Instance(base) => return narrow_down_type(db, source, base.get_base().clone()),
        LuaType::BooleanConst(_) => {
            if real_source_ref.is_boolean() {
                return Some(LuaType::Boolean);
            } else if real_source_ref.is_unknown() {
                return Some(LuaType::BooleanConst(true));
            }
        }
        LuaType::Union(target_u) => {
            let source_types = target_u
                .into_vec()
                .into_iter()
                .filter_map(|t| narrow_down_type(db, real_source_ref.clone(), t))
                .collect::<Vec<_>>();
            let mut result_type = LuaType::Unknown;
            for source_type in source_types {
                result_type = TypeOps::Union.apply(db, &result_type, &source_type);
            }
            return Some(result_type);
        }
        LuaType::Variadic(_) => return Some(source),
        LuaType::Def(type_id) | LuaType::Ref(type_id) => match real_source_ref {
            LuaType::Def(ref_id) | LuaType::Ref(ref_id) => {
                if is_sub_type_of(db, ref_id, type_id) || is_sub_type_of(db, type_id, ref_id) {
                    return Some(source);
                }
            }
            _ => {}
        },

        _ => {}
    }

    match real_source_ref {
        LuaType::Union(union) => {
            let union_types = union
                .into_vec()
                .into_iter()
                .filter_map(|t| narrow_down_type(db, t, target.clone()))
                .collect::<Vec<_>>();

            return Some(LuaType::from_vec(union_types));
        }
        LuaType::MultiLineUnion(multi_line_union) => {
            let union_types = multi_line_union
                .get_unions()
                .iter()
                .filter_map(|(ty, _)| narrow_down_type(db, ty.clone(), target.clone()))
                .collect::<Vec<_>>();

            return Some(LuaType::from_vec(union_types));
        }
        _ => {}
    }

    None
}
