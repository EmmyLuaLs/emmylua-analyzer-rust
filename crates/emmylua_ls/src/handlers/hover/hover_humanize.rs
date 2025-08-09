use super::std_hover::{hover_std_description, is_std};
use emmylua_code_analysis::{
    DbIndex, InFiled, LuaMember, LuaMemberOwner, LuaMultiLineUnion, LuaSemanticDeclId, LuaType,
    LuaUnionType, RenderLevel, SemanticDeclLevel, SemanticModel, format_union_type,
};

use emmylua_code_analysis::humanize_type;
use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexExpr, LuaStat, LuaSyntaxId, LuaSyntaxKind};
use rowan::TextRange;

use super::hover_builder::HoverBuilder;

pub fn hover_const_type(db: &DbIndex, typ: &LuaType, verbose: bool) -> String {
    let render_level = if verbose {
        RenderLevel::Verbose
    } else {
        RenderLevel::Detailed
    };

    let const_value = humanize_type(db, typ, render_level);

    match typ {
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
            format!("integer = {}", const_value)
        }
        LuaType::FloatConst(_) => format!("number = {}", const_value),
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => format!("string = {}", const_value),
        _ => const_value,
    }
}

pub fn hover_humanize_type(
    builder: &mut HoverBuilder,
    ty: &LuaType,
    verbose: bool,
    fallback_level: RenderLevel, // 当有值时, 若获取类型描述为空会回退到使用`humanize_type()`
) -> String {
    let render_level = if verbose {
        RenderLevel::Verbose
    } else {
        RenderLevel::Detailed
    };

    let db = builder.semantic_model.get_db();
    match ty {
        LuaType::Ref(type_decl_id) => {
            if let Some(type_decl) = db.get_type_index().get_type_decl(type_decl_id) {
                if let Some(LuaType::MultiLineUnion(multi_union)) =
                    type_decl.get_alias_origin(db, None)
                {
                    return hover_multi_line_union_type(
                        builder,
                        db,
                        multi_union.as_ref(),
                        Some(type_decl.get_full_name()),
                        verbose,
                    )
                    .unwrap_or_default();
                }
            }
            humanize_type(db, ty, fallback_level)
        }
        LuaType::MultiLineUnion(multi_union) => {
            hover_multi_line_union_type(builder, db, multi_union.as_ref(), None, verbose)
                .unwrap_or_default()
        }
        LuaType::Union(union) => hover_union_type(builder, union, render_level, verbose),
        _ => humanize_type(db, ty, fallback_level),
    }
}

fn hover_union_type(
    builder: &mut HoverBuilder,
    union: &LuaUnionType,
    level: RenderLevel,
    verbose: bool,
) -> String {
    format_union_type(union, level, |ty, level| {
        hover_humanize_type(builder, ty, verbose, level)
    })
}

fn hover_multi_line_union_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    multi_union: &LuaMultiLineUnion,
    ty_name: Option<&str>,
    verbose: bool,
) -> Option<String> {
    let render_level = if verbose {
        RenderLevel::Simple
    } else {
        RenderLevel::Normal
    };

    let members = multi_union.get_unions();
    let type_name = if ty_name.is_none() {
        let members = multi_union.get_unions();
        let type_str = members
            .iter()
            .take(10)
            .map(|(ty, _)| humanize_type(db, ty, render_level))
            .collect::<Vec<_>>()
            .join("|");
        Some(format!("({})", type_str))
    } else {
        ty_name.map(|name| name.to_string())
    };
    let mut text = format!("{}:\n", type_name.clone().unwrap_or_default());
    for (typ, description) in members {
        let type_humanize_text = humanize_type(db, &typ, render_level.next_level().next_level());
        if let Some(description) = description {
            text.push_str(&format!(
                "    | {} -- {}\n",
                type_humanize_text, description
            ));
        } else {
            text.push_str(&format!("    | {}\n", type_humanize_text));
        }
    }
    builder.add_type_expansion(text);
    type_name
}

/// 推断前缀是否为全局定义, 如果是, 则返回全局名称, 否则返回 None
pub fn infer_prefix_global_name<'a>(
    semantic_model: &'a SemanticModel,
    member: &LuaMember,
) -> Option<&'a str> {
    let root = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&member.get_file_id())?
        .get_red_root();
    let cur_node = member.get_syntax_id().to_node_from_root(&root)?;

    match cur_node.kind().into() {
        LuaSyntaxKind::IndexExpr => {
            let index_expr = LuaIndexExpr::cast(cur_node)?;
            let semantic_decl = semantic_model.find_decl(
                index_expr
                    .get_prefix_expr()?
                    .get_syntax_id()
                    .to_node_from_root(&root)
                    .unwrap()
                    .into(),
                SemanticDeclLevel::default(),
            );
            if let Some(property_owner) = semantic_decl {
                if let LuaSemanticDeclId::LuaDecl(id) = property_owner {
                    if let Some(decl) = semantic_model.get_db().get_decl_index().get_decl(&id) {
                        if decl.is_global() {
                            return Some(decl.get_name());
                        }
                    }
                }
            }
        }
        _ => {}
    }
    None
}

/// 描述信息结构体
#[derive(Debug, Clone)]
pub struct DescriptionInfo {
    pub description: Option<String>,
    pub tag_content: Option<Vec<(String, String)>>,
}

impl DescriptionInfo {
    pub fn new() -> Self {
        Self {
            description: None,
            tag_content: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.description.is_none() && self.tag_content.is_none()
    }
}

/// 从属性所有者获取描述信息
pub fn extract_description_from_property_owner(
    semantic_model: &SemanticModel,
    property_owner: &LuaSemanticDeclId,
) -> Option<DescriptionInfo> {
    let property = semantic_model
        .get_db()
        .get_property_index()
        .get_property(&property_owner)?;

    let mut result = DescriptionInfo::new();

    if let Some(detail) = &property.description {
        let mut description = detail.to_string();

        match property_owner {
            LuaSemanticDeclId::Member(id) => {
                if let Some(member) = semantic_model.get_db().get_member_index().get_member(&id) {
                    if let Some(LuaMemberOwner::Type(ty)) = semantic_model
                        .get_db()
                        .get_member_index()
                        .get_current_owner(&id)
                    {
                        if is_std(semantic_model.get_db(), member.get_file_id()) {
                            let std_desc =
                                hover_std_description(ty.get_name(), member.get_key().get_name());
                            if !std_desc.is_empty() {
                                description = std_desc;
                            }
                        }
                    }
                }
            }
            LuaSemanticDeclId::LuaDecl(id) => {
                if let Some(decl) = semantic_model.get_db().get_decl_index().get_decl(&id) {
                    if is_std(semantic_model.get_db(), decl.get_file_id()) {
                        let std_desc = hover_std_description(decl.get_name(), None);
                        if !std_desc.is_empty() {
                            description = std_desc;
                        }
                    }
                }
            }
            _ => {}
        }

        result.description = Some(description);
    }

    if let Some(tag_content) = &property.tag_content {
        for (tag_name, description) in tag_content.get_all_tags() {
            if result.tag_content.is_none() {
                result.tag_content = Some(Vec::new());
            }
            if let Some(tag_content) = &mut result.tag_content {
                tag_content.push((tag_name.clone(), description.clone()));
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// 从 element_id 中提取所有者名称
pub fn extract_owner_name_from_element(
    semantic_model: &SemanticModel,
    element_id: &InFiled<TextRange>,
) -> Option<String> {
    let root = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&element_id.file_id)?
        .get_red_root();

    // 通过 TextRange 找到对应的 AST 节点
    let node = LuaSyntaxId::to_node_at_range(&root, element_id.value)?;
    let stat = LuaStat::cast(node.clone().parent()?)?;
    match stat {
        LuaStat::LocalStat(local_stat) => {
            let value = LuaExpr::cast(node)?;
            let local_name = local_stat.get_local_name_by_value(value);
            if let Some(local_name) = local_name {
                return Some(local_name.get_name_token()?.get_name_text().to_string());
            }
        }
        _ => {}
    }

    None
}
