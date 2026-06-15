use emmylua_code_analysis::{DbIndex, TypeSubstitutor};
use emmylua_parser::{LuaAstNode, LuaLocalName, LuaLocalStat};

use crate::handlers::hover::{HoverBuilder, HoverDeclContext, humanize_types::DescriptionInfo};

use super::{
    extract_function_member, generic::index_prefix_substitutor,
    generic::instantiate_type_if_needed, get_function_description, render::process_function_type,
};

/// Hover 函数信息聚合
#[derive(Debug, Clone)]
pub(super) struct HoverFunctionInfo {
    pub primary: String,
    pub overloads: Option<Vec<String>>,
    pub description: Option<DescriptionInfo>,
}

impl HoverFunctionInfo {
    /// 从渲染结果构造 HoverFunctionInfo，消除重复的构造模式
    pub fn from_contents(
        contents: Vec<String>,
        description: Option<DescriptionInfo>,
    ) -> Option<Self> {
        let mut contents = contents.into_iter();
        let primary = contents.next()?;
        let overloads = {
            let overloads = contents.collect::<Vec<_>>();
            (!overloads.is_empty()).then_some(overloads)
        };
        Some(Self {
            primary,
            overloads,
            description,
        })
    }
}

pub(super) fn build_function_define_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    decl_context: &HoverDeclContext,
) -> Option<()> {
    let mut function_infos = Vec::new();
    let ordered_decls = decl_context.ordered_decl_refs();
    let substitutor = ordered_decls
        .iter()
        .any(|decl_info| decl_info.typ().contain_tpl())
        .then(|| infer_define_substitutor(builder))
        .flatten();

    for decl_info in ordered_decls {
        let semantic_decl_id = decl_info.id();
        let function_member = extract_function_member(db, semantic_decl_id);
        let instantiated_type = substitutor
            .as_ref()
            .and_then(|substitutor| instantiate_type_if_needed(db, decl_info.typ(), substitutor));
        let typ = instantiated_type
            .as_ref()
            .unwrap_or_else(|| decl_info.typ());

        let Some(contents) =
            process_function_type(builder, db, typ, semantic_decl_id, function_member, None)
        else {
            continue;
        };
        if contents.is_empty() {
            continue;
        }
        let description = get_function_description(builder, db, semantic_decl_id);
        if let Some(info) = HoverFunctionInfo::from_contents(contents, description) {
            function_infos.push(info);
        }
    }

    set_function_info_to_builder(builder, &mut function_infos)
}

fn infer_define_substitutor(builder: &HoverBuilder) -> Option<TypeSubstitutor> {
    let token = builder.get_trigger_token()?;
    let target_local_name = LuaLocalName::cast(token.parent()?)?;
    let local_stat = LuaLocalStat::cast(target_local_name.syntax().parent()?)?;

    for (index, name) in local_stat.get_local_name_list().enumerate() {
        if target_local_name == name {
            let value_expr = local_stat.get_value_exprs().nth(index)?;
            return index_prefix_substitutor(builder, &value_expr);
        }
    }

    None
}

/// 统一处理文本设置
pub(super) fn set_function_info_to_builder(
    builder: &mut HoverBuilder,
    function_infos: &mut Vec<HoverFunctionInfo>,
) -> Option<()> {
    // 去重
    function_infos.dedup_by(|a, b| a.primary == b.primary);
    if function_infos.is_empty() {
        return None;
    }

    let main = function_infos.remove(0);

    // 计算 overload 的总数
    let overload_count = main.overloads.as_ref().map_or(0, |o| o.len())
        + function_infos
            .iter()
            .map(|info| 1 + info.overloads.as_ref().map_or(0, |o| o.len()))
            .sum::<usize>();

    let main_primary = if overload_count > 0 {
        format!("{} (+{} overloads)", main.primary, overload_count)
    } else {
        main.primary
    };

    builder.set_type_description(main_primary);
    builder.add_description_from_info(main.description);

    // 添加 main 的 overloads
    if let Some(overloads) = main.overloads {
        for overload in overloads {
            builder.add_signature_overload(overload, None);
        }
    }

    // 添加其余条目
    for type_desc in function_infos.drain(..) {
        let comment = type_desc
            .description
            .and_then(|description| description.description);
        builder.add_signature_overload(type_desc.primary, comment);
        if let Some(overloads) = type_desc.overloads {
            for overload in overloads {
                builder.add_signature_overload(overload, None);
            }
        }
    }

    Some(())
}
