use crate::common::render_typ;
use crate::doc_model::DocModel;
use emmylua_code_analysis::{LuaType, RenderLevel};

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
    func_name: &str,
    is_local: bool,
) -> String {
    let local_prev = if is_local { "local " } else { "" };
    let Some(func) = model.function_info(typ, None) else {
        return format!("```lua\n{}function {}\n```\n", local_prev, func_name);
    };

    let async_prev = if func.is_async { "async " } else { "" };
    let params = func
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
        .collect::<Vec<_>>();

    let ret_strs = func
        .returns
        .iter()
        .map(|ty| render_typ(model, ty, RenderLevel::Documentation))
        .collect::<Vec<_>>()
        .join(", ");

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

    // `---@overload` is emitted as a separate signature block.
    for overload in &func.overloads {
        if let LuaType::DocFunction(_) = overload {
            result.push_str(&render_function_type(model, overload, func_name, is_local));
        }
    }

    result
}
