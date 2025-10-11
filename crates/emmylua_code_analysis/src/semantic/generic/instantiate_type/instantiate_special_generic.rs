use std::{ops::Deref, vec};

use crate::{
    DbIndex, LuaAliasCallKind, LuaAliasCallType, LuaMemberInfo, LuaMemberKey, LuaTupleStatus,
    LuaTupleType, LuaType, TypeOps, VariadicType, get_member_map,
    semantic::{
        generic::key_type_to_member_key,
        member::{find_members, infer_raw_member_type},
        type_check,
    },
};

use super::{TypeSubstitutor, instantiate_type_generic};

pub fn instantiate_alias_call(
    db: &DbIndex,
    alias_call: &LuaAliasCallType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let operands = alias_call
        .get_operands()
        .iter()
        .map(|it| instantiate_type_generic(db, it, substitutor))
        .collect::<Vec<_>>();

    match alias_call.get_call_kind() {
        LuaAliasCallKind::Sub => {
            if operands.len() != 2 {
                return LuaType::Unknown;
            }
            // 如果类型为`Union`且只有一个类型, 则会解开`Union`包装
            TypeOps::Remove.apply(db, &operands[0], &operands[1])
        }
        LuaAliasCallKind::Add => {
            if operands.len() != 2 {
                return LuaType::Unknown;
            }

            TypeOps::Union.apply(db, &operands[0], &operands[1])
        }
        LuaAliasCallKind::KeyOf => {
            if operands.len() != 1 {
                return LuaType::Unknown;
            }
            // let is_tuple = operands.len() == 1 && operands[0].is_tuple();

            let members = get_keyof_members(db, &operands[0]).unwrap_or_default();
            let member_key_types = members
                .iter()
                .filter_map(|m| match &m.key {
                    LuaMemberKey::Integer(i) => Some(LuaType::DocIntegerConst(*i)),
                    LuaMemberKey::Name(s) => Some(LuaType::DocStringConst(s.clone().into())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            LuaType::Tuple(LuaTupleType::new(member_key_types, LuaTupleStatus::InferResolve).into())
            // if is_tuple {
            //     LuaType::Tuple(
            //         LuaTupleType::new(member_key_types, LuaTupleStatus::InferResolve).into(),
            //     )
            // } else {
            //     LuaType::from_vec(member_key_types)
            // }
        }
        // 条件类型不在此处理
        LuaAliasCallKind::Extends => {
            if operands.len() != 2 {
                return LuaType::Unknown;
            }

            let compact = type_check::check_type_compact(db, &operands[0], &operands[1]).is_ok();
            LuaType::BooleanConst(compact)
        }
        LuaAliasCallKind::Select => {
            if operands.len() != 2 {
                return LuaType::Unknown;
            }

            instantiate_select_call(&operands[0], &operands[1])
        }
        LuaAliasCallKind::Unpack => instantiate_unpack_call(db, &operands),
        LuaAliasCallKind::RawGet => {
            if operands.len() != 2 {
                return LuaType::Unknown;
            }

            instantiate_rawget_call(db, &operands[0], &operands[1])
        }
        LuaAliasCallKind::Index => {
            if operands.len() != 2 {
                return LuaType::Unknown;
            }

            instantiate_index_call(db, &operands[0], &operands[1])
        }
    }
}

#[derive(Debug)]
enum NumOrLen {
    Num(i64),
    Len,
    LenUnknown,
}

fn instantiate_select_call(source: &LuaType, index: &LuaType) -> LuaType {
    let num_or_len = match index {
        LuaType::DocIntegerConst(i) => {
            if *i == 0 {
                return LuaType::Unknown;
            }
            NumOrLen::Num(*i)
        }
        LuaType::IntegerConst(i) => {
            if *i == 0 {
                return LuaType::Unknown;
            }
            NumOrLen::Num(*i)
        }
        LuaType::DocStringConst(s) => {
            if s.as_str() == "#" {
                NumOrLen::Len
            } else {
                NumOrLen::LenUnknown
            }
        }
        LuaType::StringConst(s) => {
            if s.as_str() == "#" {
                NumOrLen::Len
            } else {
                NumOrLen::LenUnknown
            }
        }
        _ => return LuaType::Unknown,
    };

    let multi_return = if let LuaType::Variadic(multi) = source {
        multi.deref()
    } else {
        &VariadicType::Base(source.clone())
    };

    match num_or_len {
        NumOrLen::Num(i) => match multi_return {
            VariadicType::Base(_) => LuaType::Variadic(multi_return.clone().into()),
            VariadicType::Multi(_) => {
                let Some(total_len) = multi_return.get_min_len() else {
                    return source.clone();
                };

                let start = if i < 0 { total_len as i64 + i } else { i - 1 };
                if start < 0 || start >= (total_len as i64) {
                    return source.clone();
                }

                let multi = multi_return.get_new_variadic_from(start as usize);
                LuaType::Variadic(multi.into())
            }
        },
        NumOrLen::Len => {
            let len = multi_return.get_min_len();
            if let Some(len) = len {
                LuaType::IntegerConst(len as i64)
            } else {
                LuaType::Integer
            }
        }
        NumOrLen::LenUnknown => LuaType::Integer,
    }
}

fn instantiate_unpack_call(db: &DbIndex, operands: &[LuaType]) -> LuaType {
    if operands.is_empty() {
        return LuaType::Unknown;
    }

    let need_unpack_type = &operands[0];
    let mut start = -1;
    // todo use end
    #[allow(unused)]
    let mut end = -1;
    if operands.len() > 1 {
        if let LuaType::DocIntegerConst(i) = &operands[1] {
            start = *i - 1;
        } else if let LuaType::IntegerConst(i) = &operands[1] {
            start = *i - 1;
        }
    }

    #[allow(unused)]
    if operands.len() > 2 {
        if let LuaType::DocIntegerConst(i) = &operands[2] {
            end = *i;
        } else if let LuaType::IntegerConst(i) = &operands[2] {
            end = *i;
        }
    }

    match &need_unpack_type {
        LuaType::Tuple(tuple) => {
            let mut types = tuple.get_types().to_vec();
            if start > 0 {
                if start as usize > types.len() {
                    return LuaType::Unknown;
                }

                if start < types.len() as i64 {
                    types = types[start as usize..].to_vec();
                }
            }

            LuaType::Variadic(VariadicType::Multi(types).into())
        }
        LuaType::Array(array_type) => LuaType::Variadic(
            VariadicType::Base(TypeOps::Union.apply(db, array_type.get_base(), &LuaType::Nil))
                .into(),
        ),
        LuaType::TableGeneric(table) => {
            if table.len() != 2 {
                return LuaType::Unknown;
            }

            let value = table[1].clone();
            LuaType::Variadic(
                VariadicType::Base(TypeOps::Union.apply(db, &value, &LuaType::Nil)).into(),
            )
        }
        LuaType::Unknown | LuaType::Any => LuaType::Unknown,
        _ => {
            // may cost many
            let mut multi_types = vec![];
            let members = match get_member_map(db, need_unpack_type) {
                Some(members) => members,
                None => return LuaType::Unknown,
            };

            for i in 1..10 {
                let member_key = LuaMemberKey::Integer(i);
                if let Some(member_info) = members.get(&member_key) {
                    let mut member_type = LuaType::Unknown;
                    for sub_member_info in member_info {
                        member_type = TypeOps::Union.apply(db, &member_type, &sub_member_info.typ);
                    }
                    multi_types.push(member_type);
                } else {
                    break;
                }
            }

            LuaType::Variadic(VariadicType::Multi(multi_types).into())
        }
    }
}

fn instantiate_rawget_call(db: &DbIndex, owner: &LuaType, key: &LuaType) -> LuaType {
    let member_key = match key {
        LuaType::DocStringConst(s) => LuaMemberKey::Name(s.deref().clone()),
        LuaType::StringConst(s) => LuaMemberKey::Name(s.deref().clone()),
        LuaType::DocIntegerConst(i) => LuaMemberKey::Integer(*i),
        LuaType::IntegerConst(i) => LuaMemberKey::Integer(*i),
        _ => return LuaType::Unknown,
    };

    infer_raw_member_type(db, owner, &member_key).unwrap_or(LuaType::Unknown)
}

fn instantiate_index_call(db: &DbIndex, owner: &LuaType, key: &LuaType) -> LuaType {
    if let LuaType::Variadic(variadic) = owner {
        match variadic.deref() {
            VariadicType::Base(base) => {
                return base.clone();
            }
            VariadicType::Multi(types) => {
                if let LuaType::IntegerConst(key) | LuaType::DocIntegerConst(key) = key {
                    return types.get(*key as usize).cloned().unwrap_or(LuaType::Never);
                }
            }
        }
    }

    if let Some(member_key) = key_type_to_member_key(key) {
        infer_raw_member_type(db, owner, &member_key).unwrap_or(LuaType::Never)
    } else {
        LuaType::Never
    }
}

fn get_keyof_members(db: &DbIndex, prefix_type: &LuaType) -> Option<Vec<LuaMemberInfo>> {
    match prefix_type {
        LuaType::Variadic(variadic) => match variadic.deref() {
            VariadicType::Base(base) => Some(vec![LuaMemberInfo {
                property_owner_id: None,
                key: LuaMemberKey::Integer(0),
                typ: base.clone(),
                feature: None,
                overload_index: None,
            }]),
            VariadicType::Multi(types) => {
                let mut members = Vec::new();
                for (idx, typ) in types.iter().enumerate() {
                    members.push(LuaMemberInfo {
                        property_owner_id: None,
                        key: LuaMemberKey::Integer(idx as i64),
                        typ: typ.clone(),
                        feature: None,
                        overload_index: None,
                    });
                }

                Some(members)
            }
        },
        _ => find_members(db, prefix_type),
    }
}
