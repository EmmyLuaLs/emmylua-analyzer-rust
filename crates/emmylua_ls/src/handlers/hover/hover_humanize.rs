use std::collections::HashSet;

use emmylua_code_analysis::{
    DbIndex, LuaDocReturnInfo, LuaFunctionType, LuaMember, LuaMemberKey, LuaMemberOwner,
    LuaMultiLineUnion, LuaPropertyOwnerId, LuaSignature, LuaSignatureId, LuaType, LuaUnionType,
    RenderLevel,
};

use emmylua_code_analysis::humanize_type;

use super::hover_builder::HoverBuilder;

pub fn hover_const_type(db: &DbIndex, typ: &LuaType) -> String {
    let const_value = humanize_type(db, typ, RenderLevel::Detailed);

    match typ {
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
            format!("integer = {}", const_value)
        }
        LuaType::FloatConst(_) => format!("number = {}", const_value),
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => format!("string = {}", const_value),
        _ => const_value,
    }
}

pub fn hover_function_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: &LuaType,
    function_member: Option<&LuaMember>,
    func_name: &str,
    is_local: bool,
) {
    match typ {
        LuaType::Function => builder.set_type_description(format!("function {}()", func_name)),
        LuaType::DocFunction(lua_func) => builder.set_type_description(hover_doc_function_type(
            builder,
            db,
            &lua_func,
            function_member,
            func_name,
        )),
        LuaType::Signature(signature_id) => hover_signature_type(
            builder,
            db,
            signature_id.clone(),
            function_member,
            func_name,
            is_local,
        )
        .unwrap_or_else(|| {
            builder.set_type_description(format!("function {}", func_name));
            builder.signature_overload = None;
        }),
        LuaType::Union(union) => {
            // 泛型处理
            if let Some(call) = builder.get_call_signature() {
                builder.set_type_description(hover_doc_function_type(
                    builder,
                    db,
                    &call,
                    function_member,
                    func_name,
                ))
            } else {
                // 将最后一个作为 type_description
                let mut overloads = Vec::new();
                for typ in union.get_types() {
                    if let LuaType::DocFunction(lua_func) = typ {
                        overloads.push(hover_doc_function_type(
                            builder,
                            db,
                            &lua_func,
                            function_member,
                            func_name,
                        ));
                    }
                }
                let signature = overloads.pop().unwrap();
                builder.set_type_description(signature);
                for overload in overloads {
                    builder.add_signature_overload(overload);
                }
            }
        }
        _ => builder.set_type_description(format!("function {}", func_name)),
    }
}

fn hover_doc_function_type(
    builder: &HoverBuilder,
    db: &DbIndex,
    lua_func: &LuaFunctionType,
    owner_member: Option<&LuaMember>,
    func_name: &str,
) -> String {
    let async_label = if lua_func.is_async() { "async " } else { "" };
    let mut type_label = "function ";
    // 有可能来源于类. 例如: `local add = class.add`, `add()`应被视为类方法
    let full_name = if let Some(owner_member) = owner_member {
        let global_name = builder.infer_prefix_global_name(owner_member);
        let mut name = String::new();
        let parent_owner = owner_member.get_owner();
        if let LuaMemberOwner::Type(ty) = &parent_owner.clone() {
            // 如果是全局定义, 则使用定义时的名称
            if let Some(global_name) = global_name {
                name.push_str(global_name);
            } else {
                name.push_str(ty.get_simple_name());
            }
            if owner_member.is_field().is_some() {
                type_label = "(field) ";
            }
        }
        if lua_func.is_colon_define() || lua_func.first_param_is_self() {
            type_label = "(method) ";
            name.push_str(":");
        } else {
            name.push_str(".");
        }
        if let LuaMemberKey::Name(n) = owner_member.get_key() {
            name.push_str(n.as_str());
        }
        name
    } else {
        func_name.to_string()
    };

    let params = lua_func
        .get_params()
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let name = param.0.clone();
            if index == 0 && param.1.is_some() && param.1.as_ref().unwrap().is_self_infer() {
                "".to_string()
            } else if let Some(ty) = &param.1 {
                format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Normal))
            } else {
                name.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let rets = {
        let rets = lua_func.get_ret();
        if rets.is_empty() {
            "".to_string()
        } else {
            format!(
                " -> {}",
                rets.iter()
                    .map(|ty| humanize_type(db, ty, RenderLevel::Simple))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    };
    format_function_type(type_label, async_label, full_name, params, rets)
}

fn hover_signature_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    signature_id: LuaSignatureId,
    owner_member: Option<&LuaMember>,
    func_name: &str,
    is_local: bool,
) -> Option<()> {
    let signature = db.get_signature_index().get(&signature_id)?;
    let call_signature = builder.get_call_signature();

    let mut type_label = "function ";
    // 有可能来源于类. 例如: `local add = class.add`, `add()`应被视为类定义的内容
    let full_name = if let Some(owner_member) = owner_member {
        let global_name = builder.infer_prefix_global_name(owner_member);
        let mut name = String::new();
        if let LuaMemberOwner::Type(ty) = &owner_member.get_owner() {
            // 如果是全局定义, 则使用定义时的名称
            if let Some(global_name) = global_name {
                name.push_str(global_name);
            } else {
                name.push_str(ty.get_simple_name());
            }
            // `field`定义的function也被视为`signature`, 因此这里需要额外处理
            if signature.is_colon_define || signature.first_param_is_self() {
                type_label = "(method) ";
                name.push_str(":");
            } else {
                name.push_str(".");
            }
        }
        if let LuaMemberKey::Name(n) = owner_member.get_key() {
            name.push_str(n.as_str());
        }
        name
    } else {
        if is_local {
            type_label = "local function ";
        }
        func_name.to_string()
    };

    // 构建 signature
    let signature_info = {
        let async_label = db
            .get_property_index()
            .get_property(LuaPropertyOwnerId::Signature(signature_id))
            .map(|prop| if prop.is_async { "async " } else { "" })
            .unwrap_or("");
        let params = signature
            .get_type_params()
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let name = param.0.clone();
                if index == 0 && param.1.is_some() && param.1.as_ref().unwrap().is_self_infer() {
                    "".to_string()
                } else if let Some(ty) = &param.1 {
                    format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
                } else {
                    name
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        let rets = build_signature_rets(builder, signature, builder.is_completion, None);
        let result = format_function_type(type_label, async_label, full_name.clone(), params, rets);
        // 由于 @field 定义的`docfunction`会被视为`signature`, 因此这里额外处理
        if let Some(call_signature) = &call_signature {
            if call_signature.get_params() == signature.get_type_params() {
                // 如果具有完全匹配的签名, 那么将其设置为当前签名, 且不显示重载
                builder.set_type_description(result);
                builder.signature_overload = None;
                return Some(());
            }
        }
        result
    };
    // 构建所有重载
    let overloads: Vec<String> = {
        let mut overloads = Vec::new();
        for (_, overload) in signature.overloads.iter().enumerate() {
            let async_label = if overload.is_async() { "async " } else { "" };
            let params = overload
                .get_params()
                .iter()
                .enumerate()
                .map(|(index, param)| {
                    let name = param.0.clone();
                    if index == 0 && param.1.is_some() && param.1.as_ref().unwrap().is_self_infer()
                    {
                        "".to_string()
                    } else if let Some(ty) = &param.1 {
                        format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
                    } else {
                        name
                    }
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            let rets =
                build_signature_rets(builder, signature, builder.is_completion, Some(overload));
            let result =
                format_function_type(type_label, async_label, full_name.clone(), params, rets);

            if let Some(call_signature) = &call_signature {
                if *call_signature == **overload {
                    // 如果具有完全匹配的签名, 那么将其设置为当前签名, 且不显示重载
                    builder.set_type_description(result);
                    builder.signature_overload = None;
                    return Some(());
                }
            };
            overloads.push(result);
        }
        overloads
    };

    builder.set_type_description(signature_info);
    for overload in overloads {
        builder.add_signature_overload(overload);
    }
    Some(())
}

fn build_signature_rets(
    builder: &mut HoverBuilder,
    signature: &LuaSignature,
    is_completion: bool,
    overload: Option<&LuaFunctionType>,
) -> String {
    let db = builder.semantic_model.get_db();
    let mut result = String::new();
    // overload 的返回值固定为单行
    let overload_rets_string = if let Some(overload) = overload {
        let rets = overload.get_ret();
        let rets_string = rets
            .iter()
            .map(|typ| humanize_type(db, typ, RenderLevel::Simple))
            .collect::<Vec<_>>()
            .join(", ");
        if rets_string.is_empty() {
            "".to_string()
        } else {
            format!(" -> {}", rets_string)
        }
    } else {
        "".to_string()
    };

    fn build_signature_ret_type(
        builder: &mut HoverBuilder,
        ret_info: &LuaDocReturnInfo,
        i: usize,
    ) -> String {
        let type_expansion_count = builder.get_type_expansion_count();
        let type_text =
            hover_type(builder, &ret_info.type_ref, Some(RenderLevel::Simple)).unwrap_or_default();
        if builder.get_type_expansion_count() > type_expansion_count {
            // 重新设置`type_expansion`
            if let Some(pop_type_expansion) =
                builder.pop_type_expansion(type_expansion_count, builder.get_type_expansion_count())
            {
                let mut new_type_expansion = format!("return #{}", i + 1);
                let mut seen = HashSet::new();
                for type_expansion in pop_type_expansion {
                    for line in type_expansion.lines().skip(1) {
                        if seen.insert(line.to_string()) {
                            new_type_expansion.push('\n');
                            new_type_expansion.push_str(line);
                        }
                    }
                }
                builder.add_type_expansion(new_type_expansion);
            }
        };
        type_text
    }

    if is_completion {
        let rets = if !overload_rets_string.is_empty() {
            overload_rets_string
        } else {
            let rets = &signature.return_docs;
            if rets.is_empty() {
                "".to_string()
            } else {
                format!(
                    " -> {}",
                    rets.iter()
                        .enumerate()
                        .map(|(i, ret)| build_signature_ret_type(builder, ret, i))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        };
        result.push_str(rets.as_str());
    } else {
        let rets = if !overload_rets_string.is_empty() {
            overload_rets_string
        } else {
            let rets = &signature.return_docs;
            if rets.is_empty() {
                "".to_string()
            } else {
                let mut rets_string_multiline = String::new();
                rets_string_multiline.push_str("\n");

                for (i, ret) in rets.iter().enumerate() {
                    let type_text = build_signature_ret_type(builder, ret, i);
                    let prefix = if i == 0 {
                        "-> ".to_string()
                    } else {
                        format!("{}. ", i + 1)
                    };
                    let name = ret.name.clone().unwrap_or_default();

                    rets_string_multiline.push_str(&format!(
                        "  {}{}{}\n",
                        prefix,
                        if !name.is_empty() {
                            format!("{}: ", name)
                        } else {
                            "".to_string()
                        },
                        type_text,
                    ));
                }
                rets_string_multiline
            }
        };
        result.push_str(rets.as_str());
    };
    result
}

fn format_function_type(
    type_label: &str,
    async_label: &str,
    full_name: String,
    params: String,
    rets: String,
) -> String {
    let prefix = if type_label.starts_with("function") {
        format!("{}{}", async_label, type_label)
    } else {
        format!("{}{}", type_label, async_label)
    };
    format!("{}{}({}){}", prefix, full_name, params, rets)
}

/// 有限的处理
pub fn hover_type(
    builder: &mut HoverBuilder,
    ty: &LuaType,
    fallback_level: Option<RenderLevel>, // 当有值时, 若获取类型描述为空会回退到使用`humanize_type()`
) -> Option<String> {
    let db = builder.semantic_model.get_db();
    let type_text = match ty {
        LuaType::Ref(type_decl_id) => {
            let type_decl = db.get_type_index().get_type_decl(type_decl_id)?;

            if type_decl.is_alias() {
                let origin = type_decl.get_alias_origin(db, None);
                match origin {
                    Some(LuaType::MultiLineUnion(multi_union)) => hover_multi_line_union_type(
                        builder,
                        db,
                        multi_union.as_ref(),
                        Some(type_decl.get_full_name()),
                    ),
                    _ => None,
                }
            } else {
                None
            }
        }
        LuaType::MultiLineUnion(multi_union) => {
            hover_multi_line_union_type(builder, db, multi_union.as_ref(), None)
        }
        LuaType::Union(union) => Some(hover_union_type(builder, union, RenderLevel::Detailed)),
        _ => None,
    };
    match (fallback_level, type_text) {
        (Some(level), Some(text)) if text.is_empty() => Some(humanize_type(db, ty, level)),
        (Some(level), None) => Some(humanize_type(db, ty, level)),
        (_, text) => text,
    }
}

fn hover_union_type(
    builder: &mut HoverBuilder,
    union: &LuaUnionType,
    level: RenderLevel,
) -> String {
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
    let type_str = types
        .iter()
        .take(num)
        .filter_map(|ty| hover_type(builder, ty, None))
        .collect::<Vec<_>>()
        .join("|");
    if type_str.is_empty() {
        return "".to_string();
    }
    let dots = if types.len() > num { "..." } else { "" };
    format!("({}{})", type_str, dots)
}

fn hover_multi_line_union_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    multi_union: &LuaMultiLineUnion,
    ty_name: Option<&str>,
) -> Option<String> {
    let members = multi_union.get_unions();
    let type_name = if ty_name.is_none() {
        let members = multi_union.get_unions();
        let type_str = members
            .iter()
            .take(10)
            .map(|(ty, _)| humanize_type(db, ty, RenderLevel::Simple))
            .collect::<Vec<_>>()
            .join("|");
        Some(format!("({})", type_str))
    } else {
        ty_name.map(|name| name.to_string())
    };
    let mut text = format!("{}:\n", type_name.clone().unwrap_or_default());
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
    builder.add_type_expansion(text);
    type_name
}
