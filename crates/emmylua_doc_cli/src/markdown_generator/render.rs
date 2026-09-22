use crate::common::render_typ;
use crate::markdown_generator::markdown_types::MemberParam;
use emmylua_code_analysis::{
    AsyncState, DbIndex, LuaFunctionType, LuaSignatureId, LuaType, RenderLevel, humanize_type,
};

fn render_param_type(db: &DbIndex, ty: Option<&LuaType>) -> Option<String> {
    let text = render_typ(db, ty?, RenderLevel::Documentation);
    if text.is_empty() { None } else { Some(text) }
}

/// A function's parameter and return-value rows.
pub type FunctionDetails = (Vec<MemberParam>, Vec<MemberParam>);

/// Extracts parameter and return-value rows (with descriptions) for a function
/// type, mirroring the HTML generator's `function_details_html`.
///
/// Returns `None` for non-function types. Rows are only returned when at least
/// one parameter or return value carries a description, so that empty tables
/// are not emitted (a `DocFunction` never carries descriptions).
pub fn function_details_md(db: &DbIndex, typ: &LuaType) -> Option<FunctionDetails> {
    let (params, returns) = match typ {
        LuaType::Signature(signature_id) => {
            let signature = db.get_signature_index().get(signature_id)?;
            let params = signature
                .get_type_params()
                .iter()
                .enumerate()
                .map(|(idx, (name, ty))| MemberParam {
                    name: name.clone(),
                    type_text: render_param_type(db, ty.as_ref()),
                    description: signature
                        .get_param_info_by_id(idx)
                        .and_then(|info| info.description.clone()),
                })
                .collect::<Vec<_>>();
            let returns = signature
                .return_docs
                .iter()
                .map(|ret| MemberParam {
                    name: ret.name.clone().unwrap_or_default(),
                    type_text: Some(render_typ(db, &ret.type_ref, RenderLevel::Documentation)),
                    description: ret.description.clone(),
                })
                .collect::<Vec<_>>();
            (params, returns)
        }
        LuaType::DocFunction(func) => {
            let params = func
                .get_params()
                .iter()
                .map(|(name, ty)| MemberParam {
                    name: name.clone(),
                    type_text: render_param_type(db, ty.as_ref()),
                    description: None,
                })
                .collect::<Vec<_>>();
            let returns = vec![MemberParam {
                name: String::new(),
                type_text: Some(render_typ(db, func.get_ret(), RenderLevel::Documentation)),
                description: None,
            }];
            (params, returns)
        }
        _ => return None,
    };

    let has_description = params.iter().any(|p| p.description.is_some())
        || returns.iter().any(|r| r.description.is_some());
    has_description.then_some((params, returns))
}

/// Renders the `---@overload` signatures of a function type as markdown code
/// blocks, mirroring the HTML generator's `signature_overloads_html`.
pub fn function_overloads_md(db: &DbIndex, typ: &LuaType, func_name: &str) -> Vec<String> {
    let LuaType::Signature(signature_id) = typ else {
        return Vec::new();
    };
    let Some(signature) = db.get_signature_index().get(signature_id) else {
        return Vec::new();
    };
    signature
        .overloads
        .iter()
        .map(|overload| {
            render_function_type(
                db,
                &LuaType::DocFunction(overload.clone()),
                func_name,
                false,
            )
        })
        .collect()
}

pub fn render_const_type(db: &DbIndex, typ: &LuaType) -> String {
    let const_value = humanize_type(db, typ, RenderLevel::Documentation);

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
            render_signature_type(db, *signature_id, func_name, is_local).unwrap_or(format!(
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
    let async_prev = if lua_func.get_async_state() == AsyncState::Async {
        "async "
    } else {
        ""
    };
    let local_prev = if is_local { "local " } else { "" };
    let params = lua_func
        .get_params()
        .iter()
        .map(|param| {
            let name = param.0.clone();
            if let Some(ty) = &param.1 {
                format!(
                    "{}: {}",
                    name,
                    render_typ(db, ty, RenderLevel::Documentation)
                )
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>();

    let ret_type = lua_func.get_ret();

    let ret_strs = render_typ(db, ret_type, RenderLevel::Documentation);

    let mut result = String::new();
    result.push_str("```lua\n");
    result.push_str(async_prev);
    result.push_str(local_prev);
    result.push_str("function ");
    result.push_str(func_name);
    result.push('(');
    if params.len() > 1 {
        result.push('\n');
        for param in &params {
            result.push_str("  ");
            result.push_str(param);
            result.push_str(",\n");
        }
        result.pop(); // Remove the last comma
        result.pop(); // Remove the last newline
        result.push('\n');
    } else {
        result.push_str(&params.join(", "));
    }
    result.push(')');
    if ret_strs.len() > 15 {
        result.push('\n');
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
    if let Some(signature) = db.get_signature_index().get(&signature_id) {
        async_prev = match signature.async_state {
            AsyncState::Async => "async ",
            AsyncState::Sync => "sync ",
            _ => "",
        };
    }

    let local_prev = if is_local { "local " } else { "" };
    let params = signature
        .get_type_params()
        .iter()
        .map(|param| {
            let name = param.0.clone();
            if let Some(ty) = &param.1 {
                format!(
                    "{}: {}",
                    name,
                    render_typ(db, ty, RenderLevel::Documentation)
                )
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
    result.push('(');
    if params.len() > 1 {
        result.push('\n');
        for param in &params {
            result.push_str("  ");
            result.push_str(param);
            result.push_str(",\n");
        }
        result.pop(); // Remove the last comma
        result.pop(); // Remove the last newline
        result.push('\n');
    } else {
        result.push_str(&params.join(", "));
    }
    result.push(')');
    match rets.len() {
        0 => {}
        1 => {
            result.push_str(" -> ");
            let type_text = render_typ(db, &rets[0].type_ref, RenderLevel::Documentation);
            let name = rets[0].name.clone().unwrap_or("".to_string());
            result.push_str(format!("{} {}", name, type_text).as_str());
        }
        _ => {
            result.push('\n');
            for ret in rets {
                let type_text = render_typ(db, &ret.type_ref, RenderLevel::Documentation);
                let name = ret.name.clone().unwrap_or("".to_string());
                result.push_str(format!(" -> {} {}\n", name, type_text).as_str());
            }
        }
    }

    result.push_str("\n```\n");

    Some(result)
}
