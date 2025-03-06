use crate::{
    DbIndex, GenericTpl, LuaAliasCallType, LuaFunctionType, LuaGenericType, LuaInstanceType,
    LuaIntersectionType, LuaMemberKey, LuaMemberOwner, LuaMemberPathExistType, LuaMultiReturn,
    LuaObjectType, LuaSignatureId, LuaStringTplType, LuaTupleType, LuaType, LuaTypeDeclId,
    LuaUnionType, TypeSubstitutor,
};

use super::LuaMultiLineUnion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLevel {
    Detailed,
    Simple,
    Normal,
    Brief,
    Minimal,
}

impl RenderLevel {
    pub fn next_level(self) -> RenderLevel {
        match self {
            RenderLevel::Detailed => RenderLevel::Simple,
            RenderLevel::Simple => RenderLevel::Normal,
            RenderLevel::Normal => RenderLevel::Brief,
            RenderLevel::Brief => RenderLevel::Minimal,
            RenderLevel::Minimal => RenderLevel::Minimal,
        }
    }
}

#[allow(unused)]
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
        LuaType::FloatConst(f) => f.to_string(),
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
        LuaType::Ref(id) => {
            if let Some(type_decl) = db.get_type_index().get_type_decl(id) {
                let name = type_decl.get_full_name().to_string();
                humanize_simple_type(db, id, &name, level).unwrap_or(name)
            } else {
                id.get_name().to_string()
            }
        }
        LuaType::Module(module_path) => humanize_module_type(db, module_path, level),
        LuaType::Array(arr_inner) => humanize_array_type(db, arr_inner, level),
        LuaType::Call(alias_call) => humanize_call_type(db, alias_call, level),
        LuaType::Nullable(inner) => humanize_nullable_type(db, inner, level),
        LuaType::DocFunction(lua_func) => humanize_doc_function_type(db, lua_func, level),
        LuaType::Object(object) => humanize_object_type(db, object, level),
        LuaType::Intersection(inter) => humanize_intersect_type(db, inter, level),
        LuaType::Generic(generic) => humanize_generic_type(db, generic, level),
        LuaType::TableGeneric(table_generic_params) => {
            humanize_table_generic_type(db, table_generic_params, level)
        }
        LuaType::TplRef(tpl) => humanize_tpl_ref_type(tpl),
        LuaType::StrTplRef(str_tpl) => humanize_str_tpl_ref_type(str_tpl),
        LuaType::MuliReturn(multi) => humanize_multi_return_type(db, multi, level),
        LuaType::MemberPathExist(exist_field) => humanize_exist_field_type(db, exist_field, level),
        LuaType::Instance(ins) => humanize_instance_type(db, ins, level),
        LuaType::Signature(signature_id) => humanize_signature_type(db, signature_id, level),
        LuaType::Namespace(ns) => format!("{{ {} }}", ns),
        LuaType::Variadic(inner) => format!("{}...", humanize_type(db, inner, level.next_level())),
        LuaType::MultiLineUnion(multi_union) => {
            humanize_multi_line_union_type(db, multi_union, level)
        }
        _ => "unknown".to_string(),
    }
}

fn humanize_def_type(db: &DbIndex, id: &LuaTypeDeclId, level: RenderLevel) -> String {
    let type_decl = db.get_type_index().get_type_decl(id);
    if type_decl.is_none() {
        return id.get_name().to_string();
    }

    let type_decl = type_decl.unwrap();
    let full_name = type_decl.get_full_name();
    let generic = db.get_type_index().get_generic_params(id);
    if generic.is_none() {
        return humanize_simple_type(db, id, &full_name, level).unwrap_or(full_name.to_string());
    }

    let generic_names = generic
        .unwrap()
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
) -> Option<String> {
    if level != RenderLevel::Detailed {
        return Some(name.to_string());
    }

    let member_owner = LuaMemberOwner::Type(id.clone());
    let member_index = db.get_member_index();
    let member_map = member_index.get_member_map(member_owner)?;
    let mut member_vec = Vec::new();
    for (member_key, member_id) in member_map {
        let member = member_index.get_member(member_id)?;
        let typ = member.get_decl_type();
        if !typ.is_signature() {
            member_vec.push((member_key, typ));
        }
    }

    if member_vec.is_empty() {
        return Some(name.to_string());
    }

    let mut member_strings = String::new();
    member_vec.sort_by(|a, b| a.0.cmp(&b.0));

    let mut count = 0;
    for (member_key, typ) in member_vec {
        let member_string = build_table_member_string(
            member_key,
            typ,
            humanize_type(db, typ, level.next_level()),
            level,
        );

        member_strings.push_str(&format!("    {},\n", member_string));
        count += 1;
        if count >= 12 {
            member_strings.push_str("    ...\n");
            break;
        }
    }

    Some(format!("{} {{\n{}}}", name, member_strings))
}

fn humanize_union_type(db: &DbIndex, union: &LuaUnionType, level: RenderLevel) -> String {
    let types = union.get_types();
    let num = match level {
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => {
            return "union<...>".to_string();
        }
    };
    let dots = if types.len() > num { "..." } else { "" };

    let type_str = types
        .iter()
        .take(num)
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join("|");
    format!("({}{})", type_str, dots)
}

fn humanize_multi_line_union_type(
    db: &DbIndex,
    multi_union: &LuaMultiLineUnion,
    level: RenderLevel,
) -> String {
    let members = multi_union.get_unions();
    let num = match level {
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => {
            return "union<...>".to_string();
        }
    };
    let dots = if members.len() > num { "..." } else { "" };

    let type_str = members
        .iter()
        .take(num)
        .map(|(ty, _)| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join("|");

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
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => {
            return "tuple<...>".to_string();
        }
    };

    let dots = if types.len() > num { "..." } else { "" };

    let type_str = types
        .iter()
        .take(num)
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(",");
    format!("({}{})", type_str, dots)
}

fn humanize_module_type(db: &DbIndex, module_path: &str, level: RenderLevel) -> String {
    let module = db.get_module_index().find_module(module_path);
    if module.is_none() {
        return format!("module({})", module_path);
    }

    let module = module.unwrap();
    if module.export_type.is_none() {
        return format!("module({})", module_path);
    }

    humanize_type(db, &module.export_type.clone().unwrap(), level)
}

fn humanize_array_type(db: &DbIndex, inner: &LuaType, level: RenderLevel) -> String {
    let element_type = humanize_type(db, inner, level.next_level());
    format!("{}[]", element_type)
}

#[allow(unused)]
fn humanize_call_type(db: &DbIndex, inner: &LuaAliasCallType, level: RenderLevel) -> String {
    // if level == RenderLevel::Minimal {
    //     return "(keyof)".to_string();
    // }
    // let element_type = humanize_type(db, inner, level.next_level());
    // format!("keyof {}", element_type)
    "(call)".to_string()
}

fn humanize_nullable_type(db: &DbIndex, inner: &LuaType, level: RenderLevel) -> String {
    let element_type = humanize_type(db, inner, level.next_level());
    format!("{}?", element_type)
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

    let rets = lua_func.get_ret();

    if rets.is_empty() {
        return format!("{}({})", prev, params);
    }

    let ret_strs = rets
        .iter()
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(", ");

    if rets.len() > 1 {
        return format!("{}({}) -> ({})", prev, params, ret_strs);
    }
    format!("{}({}) -> {}", prev, params, ret_strs)
}

fn humanize_object_type(db: &DbIndex, object: &LuaObjectType, level: RenderLevel) -> String {
    let num = match level {
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
        .take(num)
        .map(|field| {
            let name = field.0.clone();
            let ty_str = humanize_type(db, field.1, level.next_level());
            match name {
                LuaMemberKey::Integer(i) => format!("[{}]: {}", i, ty_str),
                LuaMemberKey::Name(s) => format!("{}: {}", s, ty_str),
                LuaMemberKey::None => ty_str,
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    let access = object
        .get_index_access()
        .iter()
        .map(|(key, value)| {
            let key_str = humanize_type(db, key, level.next_level());
            let value_str = humanize_type(db, value, level.next_level());
            format!("[{}]: {}", key_str, value_str)
        })
        .collect::<Vec<_>>()
        .join(",");

    if access.is_empty() {
        return format!("{{ {}{} }}", fields, dots);
    }
    format!("{{ {}, {}{} }}", fields, access, dots)
}

fn humanize_intersect_type(
    db: &DbIndex,
    inter: &LuaIntersectionType,
    level: RenderLevel,
) -> String {
    let num = match level {
        RenderLevel::Detailed => 10,
        RenderLevel::Simple => 8,
        RenderLevel::Normal => 4,
        RenderLevel::Brief => 2,
        RenderLevel::Minimal => {
            return "intersect<...>".to_string();
        }
    };

    let types = inter.get_types();
    let dots = if types.len() > num { ", ..." } else { "" };

    let type_str = types
        .iter()
        .take(num)
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join("&");
    format!("({}{})", type_str, dots)
}

fn humanize_generic_type(db: &DbIndex, generic: &LuaGenericType, level: RenderLevel) -> String {
    let base_id = generic.get_base_type_id();
    let type_decl = db.get_type_index().get_type_decl(&base_id);
    if type_decl.is_none() {
        return base_id.get_name().to_string();
    }

    let type_decl = type_decl.unwrap();
    let simple_name = type_decl.get_name();
    match level {
        RenderLevel::Brief => {
            if type_decl.is_alias() {
                let params = generic
                    .get_params()
                    .iter()
                    .map(|ty| ty.clone())
                    .collect::<Vec<_>>();
                let substitutor = TypeSubstitutor::from_type_array(params);
                if let Some(origin) = type_decl.get_alias_origin(db, Some(&substitutor)) {
                    return humanize_type(db, &origin, level.next_level());
                }
            }
        }
        RenderLevel::Minimal => {
            return format!("{}<...>", simple_name);
        }
        _ => {}
    }

    let generic_params = generic
        .get_params()
        .iter()
        .map(|ty| humanize_type(db, ty, level.next_level()))
        .collect::<Vec<_>>()
        .join(",");

    format!("{}<{}>", simple_name, generic_params)
}

fn humanize_table_const_type_detail_and_simple(
    db: &DbIndex,
    member_owned: LuaMemberOwner,
    level: RenderLevel,
) -> Option<String> {
    let member_index = db.get_member_index();
    let member_map = member_index.get_member_map(member_owned)?;

    // sort by key
    let mut member_vec = member_map.into_iter().collect::<Vec<_>>();
    member_vec.sort_by(|a, b| a.0.cmp(&b.0));

    let mut total_length = 0;
    let mut total_line = 0;
    let mut members_string = String::new();
    for (member_key, member_id) in member_vec {
        let member_value = match member_index.get_member(member_id) {
            Some(value) => value,
            None => continue,
        };
        let typ = member_value.get_decl_type();
        let member_string = build_table_member_string(
            member_key,
            typ,
            humanize_type(db, typ, level.next_level()),
            level,
        );

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

                if total_length + member_string_len > 54 {
                    members_string.push_str(", ...");
                    break;
                }

                if !members_string.is_empty() {
                    members_string.push_str(", ");
                    total_length += 2; // account for ", "
                }
                total_length += member_string_len;
                members_string.push_str(&member_string);
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
        .join(",");

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

fn humanize_multi_return_type(db: &DbIndex, multi: &LuaMultiReturn, level: RenderLevel) -> String {
    match multi {
        LuaMultiReturn::Base(base) => {
            let base_str = humanize_type(db, base, level);
            format!("{} ...", base_str)
        }
        LuaMultiReturn::Multi(types) => {
            let num = match level {
                RenderLevel::Detailed => 10,
                RenderLevel::Simple => 8,
                RenderLevel::Normal => 4,
                RenderLevel::Brief => 2,
                RenderLevel::Minimal => {
                    return "multi<...>".to_string();
                }
            };

            let dots = if types.len() > num { ", ..." } else { "" };
            let type_str = types
                .iter()
                .take(num)
                .map(|ty| humanize_type(db, ty, level.next_level()))
                .collect::<Vec<_>>()
                .join(",");
            format!("({}{})", type_str, dots)
        }
    }
}

// optimize
fn humanize_exist_field_type(
    db: &DbIndex,
    exist_field: &LuaMemberPathExistType,
    level: RenderLevel,
) -> String {
    humanize_type(db, exist_field.get_origin(), level)
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

    let signature = db.get_signature_index().get(signature_id);
    if signature.is_none() {
        return "unknown".to_string();
    }

    let signature = signature.unwrap();
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

    let rets = signature
        .return_docs
        .iter()
        .map(|ret| humanize_type(db, &ret.type_ref, level.next_level()))
        .collect::<Vec<_>>();

    let generic_str = if generics.is_empty() {
        "".to_string()
    } else {
        format!("<{}>", generics)
    };

    let ret_str = if rets.is_empty() {
        "".to_string()
    } else {
        format!(" -> {}", rets.join(","))
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
    }
}
