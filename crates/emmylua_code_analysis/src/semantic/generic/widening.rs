use crate::LuaType;

pub fn widen_literal_type(typ: LuaType) -> LuaType {
    match &typ {
        LuaType::FloatConst(_) => LuaType::Number,
        LuaType::DocIntegerConst(_) | LuaType::IntegerConst(_) => LuaType::Integer,
        LuaType::DocStringConst(_) | LuaType::StringConst(_) => LuaType::String,
        LuaType::DocBooleanConst(_) | LuaType::BooleanConst(_) => LuaType::Boolean,
        _ => typ,
    }
}
