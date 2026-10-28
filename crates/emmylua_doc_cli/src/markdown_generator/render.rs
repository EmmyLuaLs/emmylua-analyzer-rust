use crate::common::render_typ;
use crate::doc_model::{DocModel, DocSignature};
use crate::markdown_generator::markdown_types::MemberParam;
use emmylua_code_analysis::{LuaType, RenderLevel};

fn render_param_type(model: &DocModel, ty: Option<&LuaType>) -> Option<String> {
    let text = render_typ(model, ty?, RenderLevel::Documentation);
    if text.is_empty() { None } else { Some(text) }
}

/// A function's parameter and return-value rows.
pub type FunctionDetails = (Vec<MemberParam>, Vec<MemberParam>);

/// Extracts parameter and return-value rows for a function member.
///
/// Signature-backed functions use their explicit `---@param` / `---@return`
/// annotations, so type-only and described entries stay visible while inferred
/// returns do not leak into the detail table. `DocFunction` members (for example
/// `---@field add fun(a: number): number`) fall back to the function type itself.
pub fn function_details_md(
    model: &DocModel,
    typ: &LuaType,
    signature: Option<&DocSignature>,
) -> Option<FunctionDetails> {
    let (params, returns) = if let Some(signature) = signature {
        let params = signature
            .params
            .iter()
            .map(|param| MemberParam {
                name: param.name.clone(),
                type_text: render_param_type(model, param.ty.as_ref()),
                description: param.description.clone(),
            })
            .collect::<Vec<_>>();
        let returns = signature
            .returns
            .iter()
            .map(|ret| MemberParam {
                name: ret.name.clone().unwrap_or_default(),
                type_text: render_param_type(model, Some(&ret.ty)),
                description: ret.description.clone(),
            })
            .collect::<Vec<_>>();
        (params, returns)
    } else if let LuaType::DocFunction(func) = typ {
        let params = func
            .get_params()
            .iter()
            .map(|(name, ty)| MemberParam {
                name: name.clone(),
                type_text: render_param_type(model, ty.as_ref()),
                description: None,
            })
            .collect::<Vec<_>>();
        let returns = vec![MemberParam {
            name: String::new(),
            type_text: render_param_type(model, Some(func.get_ret())),
            description: None,
        }];
        (params, returns)
    } else {
        return None;
    };

    let has_details = params
        .iter()
        .any(|p| p.type_text.is_some() || p.description.is_some())
        || returns
            .iter()
            .any(|r| r.type_text.is_some() || r.description.is_some());
    has_details.then_some((params, returns))
}

/// Renders the `---@overload` signatures of a signature-backed function as
/// markdown code blocks. Plain `DocFunction` types carry no overloads.
pub fn function_overloads_md(
    model: &DocModel,
    signature: Option<&DocSignature>,
    func_name: &str,
) -> Vec<String> {
    signature
        .map(|signature| {
            signature
                .overloads
                .iter()
                .map(|overload| render_function_type(model, overload, None, func_name, false))
                .collect()
        })
        .unwrap_or_default()
}

pub fn render_const_type(model: &DocModel, typ: &LuaType) -> String {
    let const_value = model.render_type(typ, RenderLevel::Documentation);

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
    model: &DocModel,
    typ: &LuaType,
    signature: Option<&DocSignature>,
    func_name: &str,
    is_local: bool,
) -> String {
    let local_prev = if is_local { "local " } else { "" };
    let Some(func) = model.function_info(typ, signature) else {
        return format!("```lua\n{}function {}\n```\n", local_prev, func_name);
    };

    let async_prev = if func.is_async { "async " } else { "" };
    let params = if !func.params.is_empty() {
        func.params
            .iter()
            .map(|param| match &param.ty {
                Some(ty) => format!(
                    "{}: {}",
                    param.name,
                    render_typ(model, ty, RenderLevel::Documentation)
                ),
                None => param.name.clone(),
            })
            .collect::<Vec<_>>()
    } else if let Some(signature) = signature {
        signature
            .params
            .iter()
            .map(|param| match &param.ty {
                Some(ty) => format!(
                    "{}: {}",
                    param.name,
                    render_typ(model, ty, RenderLevel::Documentation)
                ),
                None => param.name.clone(),
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let ret_strs = if !func.returns.is_empty() {
        func.returns
            .iter()
            .map(|ty| render_typ(model, ty, RenderLevel::Documentation))
            .collect::<Vec<_>>()
            .join(", ")
    } else if let Some(signature) = signature {
        signature
            .returns
            .iter()
            .map(|ret| render_typ(model, &ret.ty, RenderLevel::Documentation))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        String::new()
    };

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

    if !ret_strs.is_empty() && ret_strs != "unknown" {
        result.push_str("-> ");
        result.push_str(&ret_strs);
    }
    result.push_str("\n```\n");

    result
}
