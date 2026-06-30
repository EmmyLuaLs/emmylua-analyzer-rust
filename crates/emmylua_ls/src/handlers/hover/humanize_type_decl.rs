use emmylua_code_analysis::{
    DbIndex, LuaSemanticDeclId, LuaType, LuaTypeDeclId, RenderLevel,
    get_attribute_constructor_params, humanize_type, is_attribute_class,
};
use emmylua_parser::{LuaAstNode, LuaDocAttributeUse, LuaExpr};

use crate::handlers::hover::HoverBuilder;

pub fn build_type_decl_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    type_decl_id: LuaTypeDeclId,
) -> Option<()> {
    let type_decl = db.get_type_index().get_type_decl(&type_decl_id)?;
    let type_description = if type_decl.is_alias() {
        if let Some(origin) = type_decl.get_alias_origin(db, None) {
            let type_name =
                humanize_type(db, &LuaType::Def(type_decl_id.clone()), RenderLevel::Normal);
            let origin_type = humanize_type(db, &origin, builder.detail_render_level);
            format!("(alias) {} = {}", type_name, origin_type)
        } else {
            "".to_string()
        }
    } else if type_decl.is_enum() {
        format!("(enum) {}", type_decl.get_name())
    } else if is_attribute_class(db, &type_decl_id) {
        build_attribute(builder, db, type_decl.get_name(), &type_decl_id)
    } else {
        let humanize_text = humanize_type(
            db,
            &LuaType::Def(type_decl_id.clone()),
            builder.detail_render_level,
        );
        format!("(class) {}", humanize_text)
    };

    builder.set_type_description(type_description);
    builder.add_description(&LuaSemanticDeclId::TypeDecl(type_decl_id));
    Some(())
}

fn build_attribute(
    builder: &HoverBuilder,
    db: &DbIndex,
    attribute_name: &str,
    type_decl_id: &LuaTypeDeclId,
) -> String {
    let arg_types = get_hover_attribute_arg_types(builder);
    let params = get_attribute_constructor_params(db, type_decl_id, &arg_types)
        .into_iter()
        .map(|(name, typ)| match typ {
            Some(typ) => {
                let type_name = humanize_type(db, &typ, RenderLevel::Normal);
                format!("{}: {}", name, type_name)
            }
            None => name.to_string(),
        })
        .collect::<Vec<_>>();

    if params.is_empty() {
        format!("(class) {}", attribute_name)
    } else {
        format!("(class) {}({})", attribute_name, params.join(", "))
    }
}

fn get_hover_attribute_arg_types(builder: &HoverBuilder) -> Vec<LuaType> {
    let Some(token) = builder.get_trigger_token() else {
        return Vec::new();
    };

    let mut node = token.parent();
    while let Some(current) = node {
        if let Some(attribute_use) = LuaDocAttributeUse::cast(current.clone()) {
            return attribute_use
                .get_arg_list()
                .map(|arg_list| {
                    arg_list
                        .get_args()
                        .map(|arg| {
                            builder
                                .semantic_model
                                .infer_expr(LuaExpr::LiteralExpr(arg))
                                .unwrap_or(LuaType::Unknown)
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        node = current.parent();
    }

    Vec::new()
}
