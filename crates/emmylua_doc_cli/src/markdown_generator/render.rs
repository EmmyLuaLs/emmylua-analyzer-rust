use emmylua_code_analysis::{
    humanize_type, DbIndex, LuaFunctionType, LuaPropertyOwnerId, LuaSignatureId, LuaType,
    RenderLevel,
};

pub fn render_const_type(db: &DbIndex, typ: &LuaType) -> String {
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

pub fn render_function_type(
    db: &DbIndex,
    typ: &LuaType,
    func_name: &str,
    is_local: bool,
) -> String {
    match typ {
        LuaType::Function => {
            format!(
                "```lua\n{}function {}()\n```\n",
                if is_local { "local " } else { "" },
                func_name
            )
        }
        LuaType::DocFunction(lua_func) => {
            render_doc_function_type(db, lua_func, func_name, is_local)
        }
        LuaType::Signature(signature_id) => {
            render_signature_type(db, signature_id.clone(), func_name, is_local).unwrap_or(format!(
                "{}function {}",
                if is_local { "local " } else { "" },
                func_name
            ))
        }
        _ => format!(
            "```lua\n{}function {}\n```\n",
            if is_local { "local " } else { "" },
            func_name
        ),
    }
}

fn render_doc_function_type(
    db: &DbIndex,
    lua_func: &LuaFunctionType,
    func_name: &str,
    is_local: bool,
) -> String {
    let async_prev = if lua_func.is_async() { "async " } else { "" };
    let local_prev = if is_local { "local " } else { "" };
    let params = lua_func
        .get_params()
        .iter()
        .map(|param| {
            let name = param.0.clone();
            if let Some(ty) = &param.1 {
                format!("{}: {}", name, render_typ(db, ty))
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>();

    let rets = lua_func.get_ret();

    let ret_strs = rets
        .iter()
        .map(|ty| render_typ(db, ty))
        .collect::<Vec<_>>()
        .join(",");

    let mut result = String::new();
    result.push_str("```lua\n");
    result.push_str(async_prev);
    result.push_str(local_prev);
    result.push_str("function ");
    result.push_str(func_name);
    result.push_str("(");
    if params.len() > 1 {
        result.push_str("\n");
        for param in &params {
            result.push_str("  ");
            result.push_str(param);
            result.push_str(",\n");
        }
        result.pop(); // Remove the last comma
        result.pop(); // Remove the last newline
        result.push_str("\n");
    } else {
        result.push_str(&params.join(", "));
    }
    result.push_str(")");
    if ret_strs.len() > 15 {
        result.push_str("\n");
    }

    if !ret_strs.is_empty() {
        result.push_str("-> ");
        result.push_str(&ret_strs);
    }
    result.push_str("\n```\n");

    result
}

fn render_signature_type(
    db: &DbIndex,
    signature_id: LuaSignatureId,
    func_name: &str,
    is_local: bool,
) -> Option<String> {
    let signature = db.get_signature_index().get(&signature_id)?;
    let mut async_prev = "";
    if let Some(property) = db
        .get_property_index()
        .get_property(LuaPropertyOwnerId::Signature(signature_id))
    {
        async_prev = if property.is_async { "async " } else { "" };
    }

    let local_prev = if is_local { "local " } else { "" };
    let params = signature
        .get_type_params()
        .iter()
        .map(|param| {
            let name = param.0.clone();
            if let Some(ty) = &param.1 {
                format!("{}: {}", name, render_typ(db, ty))
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>();

    let rets = &signature.return_docs;

    let mut result = String::new();
    result.push_str("```lua\n");
    result.push_str(async_prev);
    result.push_str(local_prev);
    result.push_str("function ");
    result.push_str(func_name);
    result.push_str("(");
    if params.len() > 1 {
        result.push_str("\n");
        for param in &params {
            result.push_str("  ");
            result.push_str(param);
            result.push_str(",\n");
        }
        result.pop(); // Remove the last comma
        result.pop(); // Remove the last newline
        result.push_str("\n");
    } else {
        result.push_str(&params.join(", "));
    }
    result.push_str(")");
    match rets.len() {
        0 => {}
        1 => {
            result.push_str(" -> ");
            let type_text = render_typ(db, &rets[0].type_ref);
            let name = rets[0].name.clone().unwrap_or("".to_string());
            result.push_str(format!("{} {}", name, type_text).as_str());
        }
        _ => {
            result.push_str("\n");
            for ret in rets {
                let type_text = render_typ(db, &ret.type_ref);
                let name = ret.name.clone().unwrap_or("".to_string());
                result.push_str(format!(" -> {} {}\n", name, type_text).as_str());
            }
        }
    }

    result.push_str("\n```\n");
    let param_count = signature.params.len();
    for i in 0..param_count {
        let param_info = match signature.get_param_info_by_id(i) {
            Some(info) => info,
            None => continue,
        };

        if let Some(description) = &param_info.description {
            result.push_str(&format!("@param `{}`", param_info.name));
            result.push_str(&format!(" - {}", description));
            result.push_str("\n\n");
        }
    }
    result.push_str("\n");
    for ret in rets {
        if let Some(description) = &ret.description {
            let name = match ret.name {
                Some(ref name) => format!("`{}`", name),
                None => "".to_string(),
            };
            result.push_str(&format!("@return {}", name));
            result.push_str(&format!(" - {}", description));
            result.push_str("\n\n");
        }
    }

    result.push_str("\n");

    Some(result)
}

fn render_typ(db: &DbIndex, typ: &LuaType) -> String {
    match typ {
        LuaType::IntegerConst(_) => "integer".to_string(),
        LuaType::FloatConst(_) => "number".to_string(),
        LuaType::StringConst(_) => "string".to_string(),
        LuaType::BooleanConst(_) => "boolean".to_string(),
        _ => humanize_type(db, typ, RenderLevel::Simple),
    }
}
