use crate::doc_model::DocModel;
use emmylua_code_analysis::{LuaType, RenderLevel};

pub fn render_typ(model: &DocModel, typ: &LuaType, level: RenderLevel) -> String {
    match typ {
        LuaType::IntegerConst(_) => "integer".to_string(),
        LuaType::FloatConst(_) => "number".to_string(),
        LuaType::StringConst(_) => "string".to_string(),
        LuaType::BooleanConst(_) => "boolean".to_string(),
        _ => model.render_type(typ, level),
    }
}

pub fn render_const(typ: &LuaType) -> Option<String> {
    match typ {
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => Some(i.to_string()),
        LuaType::FloatConst(f) => Some(f.to_string()),
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
            Some(format!("{:?}", s.to_string()))
        }
        LuaType::BooleanConst(b) => Some(b.to_string()),
        _ => None,
    }
}
