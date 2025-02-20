use emmylua_code_analysis::{
    DbIndex, LuaFunctionType, LuaMember, LuaMemberKey, LuaMemberOwner, LuaPropertyOwnerId,
    LuaSignature, LuaSignatureId, LuaType, RenderLevel,
};

use emmylua_code_analysis::humanize_type;

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

pub enum HoverFunctionTypeResult {
    String(String),
    Signature(String, Vec<String>), // 函数签名, 重载签名与描述
}

pub fn hover_function_type(
    db: &DbIndex,
    typ: &LuaType,
    function_member: Option<&LuaMember>,
    func_name: &str,
    is_completion: bool,
) -> HoverFunctionTypeResult {
    match typ {
        LuaType::Function => HoverFunctionTypeResult::String(format!("function {}()", func_name)),
        LuaType::DocFunction(lua_func) => HoverFunctionTypeResult::String(hover_doc_function_type(
            db,
            &lua_func,
            function_member,
            func_name,
        )),
        LuaType::Signature(signature_id) =>
            hover_signature_type(
                db,
                signature_id.clone(),
                function_member,
                func_name,
                is_completion,
            )
            .unwrap_or(HoverFunctionTypeResult::String(format!("function {}", func_name))),
        _ => HoverFunctionTypeResult::String(format!("function {}", func_name)),
    }
}

#[allow(unused)]
fn hover_doc_function_type(
    db: &DbIndex,
    lua_func: &LuaFunctionType,
    owner_member: Option<&LuaMember>,
    func_name: &str,
) -> String {
    let async_prev = if lua_func.is_async() { "async " } else { "" };
    let mut type_prev = "function ";
    // 有可能来源于类. 例如: `local add = class.add`, `add()`应被视为类方法
    let full_func_name = if let Some(owner_member) = owner_member {
        let mut name = String::new();
        let parent_owner = owner_member.get_owner();
        if let LuaMemberOwner::Type(ty) = &parent_owner {
            name.push_str(ty.get_simple_name());
            if owner_member.is_field().is_some() {
                type_prev = "(field) ";
            }
        }
        match owner_member.get_decl_type() {
            LuaType::DocFunction(func) => {
                if func.is_colon_define()
                    || func.get_params().first().and_then(|param| {
                        param.1.as_ref().map(|ty| {
                            param.0 == "self"
                                && humanize_type(db, ty, RenderLevel::Normal) == "self"
                        })
                    }) == Some(true)
                {
                    type_prev = "(method) ";
                    name.push_str(":");
                } else {
                    name.push_str(".");
                }
            }
            _ => {}
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
            if index == 0
                && param.1.is_some()
                && name == "self"
                && humanize_type(db, param.1.as_ref().unwrap(), RenderLevel::Normal) == "self"
            {
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

    let rets = lua_func.get_ret();

    let mut result = String::new();
    result.push_str(type_prev);
    result.push_str(async_prev);
    result.push_str(&full_func_name);
    result.push_str("(");
    result.push_str(params.as_str());
    result.push_str(")");

    if !rets.is_empty() {
        result.push_str(" -> ");
        if rets.len() > 1 {
            result.push_str("(");
        }
        result.push_str(
            &rets
                .iter()
                .map(|ty| humanize_type(db, ty, RenderLevel::Normal))
                .collect::<Vec<_>>()
                .join(", "),
        );
        if rets.len() > 1 {
            result.push_str(")");
        }
    }

    result
}

fn hover_signature_type(
    db: &DbIndex,
    signature_id: LuaSignatureId,
    owner_member: Option<&LuaMember>,
    func_name: &str,
    is_completion: bool, // 是否在补全时使用
) -> Option<HoverFunctionTypeResult> {
    let signature = db.get_signature_index().get(&signature_id)?;

    let mut type_label = "function ";
    // 有可能来源于类. 例如: `local add = class.add`, `add()`应被视为类定义的内容
    let full_name = if let Some(owner_member) = owner_member {
        let mut name = String::new();
        if let LuaMemberOwner::Type(ty) = &owner_member.get_owner() {
            name.push_str(ty.get_simple_name());
            if signature.is_colon_define {
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
            .map(|param| {
                let name = param.0.clone();
                if let Some(ty) = &param.1 {
                    format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
                } else {
                    name
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let rets = get_signature_rets_string(db, signature, is_completion, None);
        let mut result = String::new();
        if type_label.starts_with("function") {
            result.push_str(async_label);
            result.push_str(type_label);
        } else {
            result.push_str(type_label);
            result.push_str(async_label);
        }
        result.push_str(&full_name);
        result.push_str("(");
        result.push_str(params.as_str());
        result.push_str(")");
        result.push_str(rets.as_str());
        result
    };
    // 构建所有重载
    let overloads = {
        let mut overloads = Vec::new();
        for (_, overload) in signature.overloads.iter().enumerate() {
            let async_label = if overload.is_async() { "async " } else { "" };
            let params = overload
                .get_params()
                .iter()
                .map(|param| {
                    let name = param.0.clone();
                    if let Some(ty) = &param.1 {
                        format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
                    } else {
                        name
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let rets = get_signature_rets_string(db, signature, is_completion, Some(overload));
            overloads.push(format!("{}function {}({}){}", async_label, full_name, params, rets));
        }
        overloads
    };
    Some(HoverFunctionTypeResult::Signature(
        signature_info,
        overloads,
    ))
}

fn get_signature_rets_string(
    db: &DbIndex,
    signature: &LuaSignature,
    is_completion: bool,
    overload: Option<&LuaFunctionType>,
) -> String {
    let mut result = String::new();
    // overload 的返回值固定为单行
    let overload_rets_string = if let Some(overload) = overload {
        let rets = overload.get_ret();
        format!(
            " -> {}",
            rets.iter()
                .map(|typ| humanize_type(db, typ, RenderLevel::Simple))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        "".to_string()
    };

    if is_completion {
        let rets = if !overload_rets_string.is_empty() {
            overload_rets_string
        } else {
            let rets = &signature.return_docs;
            format!(
                " -> {}",
                rets.iter()
                    .map(|ret| humanize_type(db, &ret.type_ref, RenderLevel::Simple))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
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
                let mut rets_string_multiline = String::with_capacity(512); // 预分配容量
                rets_string_multiline.push_str("\n");

                for (i, ret) in rets.iter().enumerate() {
                    let type_text = humanize_type(db, &ret.type_ref, RenderLevel::Simple);
                    let prefix = if i == 0 {
                        "->".to_string()
                    } else {
                        format!("{}.", i + 1)
                    };
                    let name = ret.name.clone().unwrap_or_default();
                    let detail = ret
                        .description
                        .as_ref()
                        .map(|desc| format!(" — {}", desc.trim_end()))
                        .unwrap_or_default();

                    rets_string_multiline.push_str(&format!(
                        "  {}{} {}{}\n",
                        prefix,
                        if !name.is_empty() {
                            format!("{}:", name)
                        } else {
                            "".to_string()
                        },
                        type_text,
                        detail
                    ));
                }
                rets_string_multiline
            }
        };
        result.push_str(rets.as_str());
    };
    result
}
