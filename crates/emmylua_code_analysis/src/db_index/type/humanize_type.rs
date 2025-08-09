use itertools::Itertools;
use std::collections::HashSet;

use crate::{
    DbIndex, GenericTpl, LuaAliasCallType, LuaFunctionType, LuaGenericType, LuaInstanceType,
    LuaIntersectionType, LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaSignatureId,
    LuaStringTplType, LuaTupleType, LuaType, LuaTypeDeclId, LuaUnionType, VariadicType,
};

use super::{LuaAliasCallKind, LuaMultiLineUnion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLevel {
    Documentation,
    Verbose,
    Detailed,
    Simple,
    Normal,
    Brief,
    Minimal,
}

impl RenderLevel {
    pub fn next_level(self) -> RenderLevel {
        match self {
            RenderLevel::Documentation => RenderLevel::Simple,
            RenderLevel::Verbose => RenderLevel::Detailed,
            RenderLevel::Detailed => RenderLevel::Simple,
            RenderLevel::Simple => RenderLevel::Normal,
            RenderLevel::Normal => RenderLevel::Brief,
            RenderLevel::Brief => RenderLevel::Minimal,
            RenderLevel::Minimal => RenderLevel::Minimal,
        }
    }
}

pub fn humanize_type(db: &DbIndex, ty: &LuaType, level: RenderLevel) -> String {
    match ty {
        LuaType::Any => "any".to_string(),
        LuaType::Nil => "nil".to_string(),
        LuaType::Boolean => "boolean".to_string(),
        LuaType::Number => "number".to_string(),
        LuaType::String => "string".to_string(),
        LuaType::Table => "table".to_string(),
        LuaType::Function => "function".to_string(),
        LuaType::Thread => "thread".to_string(),
        LuaType::Userdata => "userdata".to_string(),
        LuaType::IntegerConst(i) => i.to_string(),
        LuaType::FloatConst(f) => {
            let s = f.to_string();
            // 如果字符串不包含小数点，添加 ".0"
            if !s.contains('.') {
                format!("{}.0", s)
            } else {
                s
            }
        }
        LuaType::TableConst(v) => {
            let member_owner = LuaMemberOwner::Element(v.clone());
            humanize_table_const_type(db, member_owner, level)
        }
        LuaType::Global => "global".to_string(),
        LuaType::Def(id) => humanize_def_type(db, id, level),
        LuaType::Union(union) => humanize_union_type(db, union, level),
        LuaType::Tuple(tuple) => humanize_tuple_type(db, tuple, level),
        LuaType::Unknown => "unknown".to_string(),
        LuaType::Integer => "integer".to_string(),
        LuaType::Io => "io".to_string(),
        LuaType::SelfInfer => "self".to_string(),
        LuaType::BooleanConst(b) => b.to_string(),
        LuaType::StringConst(s) => format!("\"{}\"", s),
        LuaType::DocStringConst(s) => format!("\"{}\"", s),
        LuaType::DocIntegerConst(i) => i.to_string(),
        LuaType::DocBooleanConst(b) => b.to_string(),
        LuaType::Ref(id) => {
            if let Some(type_decl) = db.get_type_index().get_type_decl(id) {
                let name = type_decl.get_full_name().to_string();
                humanize_simple_type(db, id, &name, level)
            } else {
                id.get_name().to_string()
            }
        }
        LuaType::Array(arr_inner) => humanize_array_type(db, arr_inner.get_base(), level),
        LuaType::Call(alias_call) => humanize_call_type(db, alias_call, level),
        LuaType::DocFunction(lua_func) => humanize_doc_function_type(db, lua_func, level),
        LuaType::Object(object) => humanize_object_type(db, object, level),
        LuaType::Intersection(inter) => humanize_intersect_type(db, inter, level),
        LuaType::Generic(generic) => humanize_generic_type(db, generic, level),
        LuaType::TableGeneric(table_generic_params) => {
            humanize_table_generic_type(db, table_generic_params, level)
        }
        LuaType::TplRef(tpl) => humanize_tpl_ref_type(tpl),
        LuaType::StrTplRef(str_tpl) => humanize_str_tpl_ref_type(str_tpl),
        LuaType::Variadic(multi) => humanize_variadic_type(db, multi, level),
        LuaType::Instance(ins) => humanize_instance_type(db, ins, level),
        LuaType::Signature(signature_id) => humanize_signature_type(db, signature_id, level),
        LuaType::Namespace(ns) => format!("{{ {} }}", ns),
        LuaType::MultiLineUnion(multi_union) => {
            humanize_multi_line_union_type(db, multi_union, level)
        }
        LuaType::TypeGuard(inner) => {
            let type_str = humanize_type(db, inner, level.next_level());
            format!("TypeGuard<{}>", type_str)
        }
        _ => "unknown".to_string(),
    }
}

fn humanize_def_type(db: &DbIndex, id: &LuaTypeDeclId, level: RenderLevel) -> String {
    let type_decl = match db.get_type_index().get_type_decl(id) {
        Some(type_decl) => type_decl,
        None => return id.get_name().to_string(),
    };

    let full_name = type_decl.get_full_name();
    let generic = match db.get_type_index().get_generic_params(id) {
        Some(generic) => generic,
        None => return humanize_simple_type(db, id, &full_name, level),
    };

    let generic_names = generic
        .iter()
        .map(|it| it.0.clone())
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}<{}>", full_name, generic_names)
}

fn humanize_simple_type(
    db: &DbIndex,
    id: &LuaTypeDeclId,
    name: &str,
    level: RenderLevel,
) -> String {
    if !matches!(
        level,
        RenderLevel::Detailed | RenderLevel::Verbose | RenderLevel::Documentation
    ) {
        return name.to_string();
    }
    let max_display_count = 12;

    let member_owner = LuaMemberOwner::Type(id.clone());
    let member_index = db.get_member_index();
    let members = member_index
        .get_sorted_members(&member_owner)
        .unwrap_or_default();
    let mut member_vec = Vec::new();
    let mut function_vec = Vec::new();
    for member in members {
        let member_key = member.get_key();
        let type_cache = db.get_type_index().get_type_cache(&member.get_id().into());
        let type_cache = match type_cache {
            Some(type_cache) => type_cache,
            None => &super::LuaTypeCache::InferType(LuaType::Any),
        };
        if type_cache.is_function() {
            function_vec.push(member_key);
        } else {
            member_vec.push((member_key, type_cache.as_type()));
        }
    }

    if member_vec.is_empty() && function_vec.is_empty() {
        if matches!(level, RenderLevel::Detailed | RenderLevel::Verbose) {
            'expand: {
                let Some(type_decl) = db.get_type_index().get_type_decl(id) else {
                    break 'expand;
                };
                let Some(origin) = type_decl.get_alias_ref() else {
                    break 'expand;
                };
                return format!(
                    "{} {}",
                    name,
                    humanize_type(db, &origin, level.next_level())
                );
            }
        }

        return name.to_string();
    }
    let all_count = member_vec.len() + function_vec.len();

    let mut member_strings = String::new();
    let mut count = 0;
    for (member_key, typ) in member_vec {
        let member_string = build_table_member_string(
            member_key,
            typ,
            humanize_type(db, typ, level.next_level()),
            level,
        )
        .replace("\n", "\n    ");

        member_strings.push_str(&format!("    {},\n", member_string));
        count += 1;
        if count >= max_display_count {
            break;
        }
    }
    if count < all_count {
        for function_key in function_vec {
            let member_string = build_table_member_string(
                function_key,
                &LuaType::Function,
                "function".to_string(),
                level,
            )
            .replace("\n", "\n    ");

            member_strings.push_str(&format!("    {},\n", member_string));
            count += 1;
            if count >= max_display_count {
                break;
            }
        }
    }
    if count >= max_display_count {
        member_strings.push_str(&format!("    ...(+{})\n", all_count - max_display_count));
    }
    format!("{} {{\n{}}}", name, member_strings)
}

fn humanize_union_type(db: &DbIndex, union: &LuaUnionType, level: RenderLevel) -> String {
    format_union_type(union, level, |ty, level| {
        humanize_type(db, ty, level.next_level())
    })
}

pub fn format_union_type<F>(
    union: &LuaUnionType,
    level: RenderLevel,
    mut type_formatter: F,
) -> String
where
    F: FnMut(&LuaType, RenderLevel) -> String,
{
    let types = union.into_vec();
    let num = match level {
        RenderLevel::Documentation => 500,
        RenderLevel::Verbose => 10,
        RenderLevel::Detailed => 8,
        RenderLevel::Simple => 6,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => 2,
    };
    // 需要确保顺序
    let mut seen = HashSet::new();
    let mut type_strings = Vec::new();
    let mut has_nil = false;
    let mut has_function = false;
    for ty in types.iter() {
        if ty.is_nil() {
            has_nil = true;
            continue;
        } else if ty.is_function() {
            has_function = true;
        }
        let type_str = type_formatter(ty, level.next_level());
        if seen.insert(type_str.clone()) {
            type_strings.push(type_str);
        }
    }
    let dots = if type_strings.len() > num { "..." } else { "" };
    let display_types: Vec<_> = type_strings.into_iter().take(num).collect();
    let type_str = display_types.join(" | ");

    if display_types.len() == 1 {
        if has_function && has_nil {
            format!("({})?", type_str)
        } else {
            format!("{}{}", type_str, if has_nil { "?" } else { "" })
        }
    } else {
        format!("({}{}){}", type_str, dots, if has_nil { "?" } else { "" })
    }
}

fn humanize_multi_line_union_type(
    db: &DbIndex,
    multi_union: &LuaMultiLineUnion,
    level: RenderLevel,
) -> String {
    let members = multi_union.get_unions();
    let num = match level {
        RenderLevel::Documentation => 500,
        RenderLevel::Verbose => 15,
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => 2,
    };
    let dots = if members.len() > num { "..." } else { "" };

    let type_str = members
        .iter()
        .take(num)
        .map(|(ty, _)| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(" | ");

    let mut text = format!("({}{})", type_str, dots);
    if level != RenderLevel::Detailed {
        return text;
    }

    text.push_str("\n");
    for (typ, description) in members {
        let type_humanize_text = humanize_type(db, &typ, RenderLevel::Minimal);
        if let Some(description) = description {
            text.push_str(&format!(
                "    | {} -- {}\n",
                type_humanize_text, description
            ));
        } else {
            text.push_str(&format!("    | {}\n", type_humanize_text));
        }
    }

    text
}

fn humanize_tuple_type(db: &DbIndex, tuple: &LuaTupleType, level: RenderLevel) -> String {
    let types = tuple.get_types();
    let num = match level {
        RenderLevel::Documentation => 500,
        RenderLevel::Verbose => 15,
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => 2,
    };

    let dots = if types.len() > num { "..." } else { "" };

    let type_str = types
        .iter()
        .take(num)
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({}{})", type_str, dots)
}

fn humanize_array_type(db: &DbIndex, inner: &LuaType, level: RenderLevel) -> String {
    let element_type = humanize_type(db, inner, level.next_level());
    format!("{}[]", element_type)
}

#[allow(unused)]
fn humanize_call_type(db: &DbIndex, inner: &LuaAliasCallType, level: RenderLevel) -> String {
    let basic = match inner.get_call_kind() {
        LuaAliasCallKind::Sub => "sub",
        LuaAliasCallKind::Add => "add",
        LuaAliasCallKind::KeyOf => "keyof",
        LuaAliasCallKind::Extends => "extends",
        LuaAliasCallKind::Select => "select",
        LuaAliasCallKind::Unpack => "unpack",
        LuaAliasCallKind::Index => "index",
        LuaAliasCallKind::RawGet => "rawget",
    };
    let operands = inner
        .get_operands()
        .iter()
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(", ");

    format!("{}<{}>", basic, operands)
}

fn humanize_doc_function_type(
    db: &DbIndex,
    lua_func: &LuaFunctionType,
    level: RenderLevel,
) -> String {
    if level == RenderLevel::Minimal {
        return "fun(...) -> ...".to_string();
    }

    let prev = if lua_func.is_async() {
        "async fun"
    } else {
        "fun"
    };
    let params = lua_func
        .get_params()
        .iter()
        .map(|param| {
            let name = param.0.clone();
            if let Some(ty) = &param.1 {
                format!("{}: {}", name, humanize_type(db, ty, level.next_level()))
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let ret_type = lua_func.get_ret();
    let return_nil = match ret_type {
        LuaType::Variadic(variadic) => match variadic.get_type(0) {
            Some(LuaType::Nil) => true,
            _ => false,
        },
        _ => ret_type.is_nil(),
    };

    if return_nil {
        return format!("{}({})", prev, params);
    }

    let ret_str = humanize_type(db, ret_type, level.next_level());

    format!("{}({}) -> {}", prev, params, ret_str)
}

fn humanize_object_type(db: &DbIndex, object: &LuaObjectType, level: RenderLevel) -> String {
    let num = match level {
        RenderLevel::Documentation => 500,
        RenderLevel::Verbose => 15,
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => {
            return "{...}".to_string();
        }
    };

    let dots = if object.get_fields().len() > num {
        ", ..."
    } else {
        ""
    };

    let fields = object
        .get_fields()
        .iter()
        .sorted_by(|a, b| a.0.cmp(&b.0))
        .take(num)
        .map(|field| {
            let name = field.0.clone();
            let ty_str = humanize_type(db, field.1, level.next_level());
            match name {
                LuaMemberKey::Integer(i) => format!("[{}]: {}", i, ty_str),
                LuaMemberKey::Name(s) => format!("{}: {}", s, ty_str),
                LuaMemberKey::None => ty_str,
                LuaMemberKey::ExprType(_) => ty_str,
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let access = object
        .get_index_access()
        .iter()
        .map(|(key, value)| {
            let key_str = humanize_type(db, key, level.next_level());
            let value_str = humanize_type(db, value, level.next_level());
            format!("[{}]: {}", key_str, value_str)
        })
        .collect::<Vec<_>>()
        .join(", ");

    if access.is_empty() {
        return format!("{{ {}{} }}", fields, dots);
    } else if fields.is_empty() {
        return format!("{{ {}{} }}", access, dots);
    }
    format!("{{ {}, {}{} }}", fields, access, dots)
}

fn humanize_intersect_type(
    db: &DbIndex,
    inter: &LuaIntersectionType,
    level: RenderLevel,
) -> String {
    let num = match level {
        RenderLevel::Documentation => 500,
        RenderLevel::Verbose => 15,
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => 2,
    };

    let types = inter.get_types();
    let dots = if types.len() > num { ", ..." } else { "" };

    let type_str = types
        .iter()
        .take(num)
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(" & ");
    format!("({}{})", type_str, dots)
}

fn humanize_generic_type(db: &DbIndex, generic: &LuaGenericType, level: RenderLevel) -> String {
    let base_id = generic.get_base_type_id();
    let type_decl = match db.get_type_index().get_type_decl(&base_id) {
        Some(type_decl) => type_decl,
        None => return base_id.get_name().to_string(),
    };

    let full_name = type_decl.get_full_name();

    let generic_params = generic
        .get_params()
        .iter()
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(", ");

    format!("{}<{}>", full_name, generic_params)
}

fn humanize_table_const_type_detail_and_simple(
    db: &DbIndex,
    member_owned: LuaMemberOwner,
    level: RenderLevel,
) -> Option<String> {
    let member_index = db.get_member_index();
    let members = member_index.get_sorted_members(&member_owned)?;

    let mut total_length = 0;
    let mut total_line = 0;
    let mut members_string = String::new();
    for member in members {
        let key = member.get_key();
        let type_cache = db.get_type_index().get_type_cache(&member.get_id().into());
        let type_cache = match type_cache {
            Some(type_cache) => type_cache,
            None => &super::LuaTypeCache::InferType(LuaType::Any),
        };
        let member_string = build_table_member_string(
            key,
            &type_cache.as_type(),
            humanize_type(db, &type_cache.as_type(), level.next_level()),
            level,
        )
        .replace("\n", "\n    ");

        match level {
            RenderLevel::Detailed => {
                total_line += 1;
                members_string.push_str(&format!("    {},\n", member_string));
                if total_line >= 12 {
                    members_string.push_str("    ...\n");
                    break;
                }
            }
            RenderLevel::Simple => {
                let member_string_len = member_string.chars().count();
                if total_length != 0 {
                    members_string.push_str(", ");
                    total_length += 2; // account for ", "
                }

                total_length += member_string_len;
                members_string.push_str(&member_string);
                if total_length > 54 {
                    members_string.push_str(", ...");
                    break;
                }
            }
            _ => return None,
        }
    }

    match level {
        RenderLevel::Detailed => Some(format!("{{\n{}}}", members_string)),
        RenderLevel::Simple => Some(format!("{{ {} }}", members_string)),
        _ => None,
    }
}

fn humanize_table_const_type(
    db: &DbIndex,
    member_owned: LuaMemberOwner,
    level: RenderLevel,
) -> String {
    match level {
        RenderLevel::Detailed | RenderLevel::Simple => {
            humanize_table_const_type_detail_and_simple(db, member_owned, level)
                .unwrap_or("table".to_string())
        }
        _ => "table".to_string(),
    }
}

fn humanize_table_generic_type(
    db: &DbIndex,
    table_generic_params: &Vec<LuaType>,
    level: RenderLevel,
) -> String {
    let num = match level {
        RenderLevel::Documentation => 500,
        RenderLevel::Verbose => 15,
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => {
            return "table<...>".to_string();
        }
    };

    let dots = if table_generic_params.len() > num {
        ", ..."
    } else {
        ""
    };

    let generic_params = table_generic_params
        .iter()
        .take(num)
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(", ");

    format!("table<{}{}>", generic_params, dots)
}

fn humanize_tpl_ref_type(tpl: &GenericTpl) -> String {
    tpl.get_name().to_string()
}

fn humanize_str_tpl_ref_type(str_tpl: &LuaStringTplType) -> String {
    let prefix = str_tpl.get_prefix();
    if prefix.is_empty() {
        str_tpl.get_name().to_string()
    } else {
        format!("{}`{}`", prefix, str_tpl.get_name())
    }
}

fn humanize_variadic_type(db: &DbIndex, multi: &VariadicType, level: RenderLevel) -> String {
    match multi {
        VariadicType::Base(base) => {
            let base_str = humanize_type(db, base, level);
            format!("{} ...", base_str)
        }
        VariadicType::Multi(types) => {
            let max_num = match level {
                RenderLevel::Documentation => 500,
                RenderLevel::Verbose => 15,
                RenderLevel::Detailed => 10,
                RenderLevel::Simple => 8,
                RenderLevel::Normal => 4,
                RenderLevel::Brief => 2,
                RenderLevel::Minimal => {
                    return "multi<...>".to_string();
                }
            };

            let dots = if types.len() > max_num { ", ..." } else { "" };
            let type_str = types
                .iter()
                .take(max_num)
                .map(|ty| humanize_type(db, ty, level.next_level()))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}{})", type_str, dots)
        }
    }
}

fn humanize_instance_type(db: &DbIndex, ins: &LuaInstanceType, level: RenderLevel) -> String {
    humanize_type(db, ins.get_base(), level)
}

fn humanize_signature_type(
    db: &DbIndex,
    signature_id: &LuaSignatureId,
    level: RenderLevel,
) -> String {
    if level == RenderLevel::Minimal {
        return "fun(...) -> ...".to_string();
    }

    let signature = match db.get_signature_index().get(signature_id) {
        Some(sig) => sig,
        None => return "unknown".to_string(),
    };

    let params = signature
        .get_type_params()
        .iter()
        .map(|param| {
            let name = param.0.clone();
            if let Some(ty) = &param.1 {
                format!("{}: {}", name, humanize_type(db, ty, level.next_level()))
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let generics = signature
        .generic_params
        .iter()
        .map(|(name, _)| name.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let generic_str = if generics.is_empty() {
        "".to_string()
    } else {
        format!("<{}>", generics)
    };

    let ret_str = {
        let ret_type = signature.get_return_type();
        let return_nil = match ret_type {
            LuaType::Variadic(variadic) => match variadic.get_type(0) {
                Some(LuaType::Nil) => true,
                _ => false,
            },
            _ => ret_type.is_nil(),
        };

        if return_nil {
            "".to_string()
        } else {
            let rets = signature
                .return_docs
                .iter()
                .map(|ret| humanize_type(db, &ret.type_ref, level.next_level()))
                .collect::<Vec<_>>();
            if rets.is_empty() {
                "".to_string()
            } else {
                format!(" -> {}", rets.join(", "))
            }
        }
    };

    format!("fun{}({}){}", generic_str, params, ret_str)
}

fn build_table_member_string(
    member_key: &LuaMemberKey,
    ty: &LuaType,
    member_value_string: String,
    level: RenderLevel,
) -> String {
    let (member_value, separator) = if level == RenderLevel::Detailed {
        let val = match ty {
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
                format!("integer = {member_value_string}")
            }
            LuaType::FloatConst(_) => format!("number = {member_value_string}"),
            LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
                format!("string = {member_value_string}")
            }
            LuaType::BooleanConst(_) => format!("boolean = {member_value_string}"),
            _ => member_value_string,
        };
        (val, ": ")
    } else {
        (member_value_string, " = ")
    };

    match member_key {
        LuaMemberKey::Name(name) => format!("{name}{separator}{member_value}"),
        LuaMemberKey::Integer(i) => format!("[{i}]{separator}{member_value}"),
        LuaMemberKey::None => member_value,
        LuaMemberKey::ExprType(_) => member_value,
    }
}
