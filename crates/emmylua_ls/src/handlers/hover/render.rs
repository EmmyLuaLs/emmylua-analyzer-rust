use emmylua_code_analysis::{LuaType, SemanticModel, humanize_semantic_type};

pub fn humanize(model: &SemanticModel<'_>, ty: &LuaType) -> String {
    humanize_semantic_type(model, ty)
}
