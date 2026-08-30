use emmylua_code_analysis::{LuaType, SalsaSemanticModel, humanize_semantic_type};

pub fn humanize(model: &SalsaSemanticModel<'_>, ty: &LuaType) -> String {
    humanize_semantic_type(model, ty)
}
