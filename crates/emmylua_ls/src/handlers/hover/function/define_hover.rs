use emmylua_code_analysis::{DbIndex, LuaSemanticDeclId, LuaType};

use crate::handlers::hover::{HoverBuilder, humanize_types::DescriptionInfo};

use super::{
    extract_function_member, get_function_description, hover_instantiate_function_type,
    render::process_function_type,
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
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
) -> Option<()> {
    let mut function_infos = Vec::new();
    for (semantic_decl_id, typ) in semantic_decls {
        let mut typ = typ.clone();
        let function_member = extract_function_member(db, semantic_decl_id);

        if let Some(substitutor) = &builder.substitutor {
            if let Some(lua_func) = hover_instantiate_function_type(db, &typ, substitutor) {
                typ = LuaType::DocFunction(lua_func);
            }
        }

        let Some(contents) =
            process_function_type(builder, db, &typ, semantic_decl_id, function_member)
        else {
            continue;
        };
        if contents.is_empty() {
            continue;
        }
        let description = get_function_description(builder, db, &semantic_decl_id);
        if let Some(info) = HoverFunctionInfo::from_contents(contents, description) {
            function_infos.push(info);
        }
    }

    set_builder_contents(builder, &mut function_infos)
}

/// 统一处理文本设置
pub(super) fn set_builder_contents(
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
