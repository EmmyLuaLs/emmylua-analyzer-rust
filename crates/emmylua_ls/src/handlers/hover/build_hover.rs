use emmylua_code_analysis::humanize_type;
use emmylua_code_analysis::{
    DbIndex, LuaDeclExtra, LuaDeclId, LuaDocument, LuaMemberId, LuaMemberKey, LuaSemanticDeclId,
    LuaSignatureId, LuaType, LuaTypeDeclId, RenderLevel, SemanticInfo, SemanticModel,
};
use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaExpr, LuaSyntaxToken};
use lsp_types::{Hover, HoverContents, MarkedString, MarkupContent};
use rowan::TextRange;

use crate::handlers::common::{find_decl_origin_owners, find_member_origin_owners};
use crate::handlers::hover::function::{build_function_hover, has_function_candidate, is_function};
use crate::handlers::hover::humanize_type_decl::build_type_decl_hover;
use crate::handlers::hover::humanize_types::hover_humanize_type;

use super::{
    HoverDeclContext, HoverDeclInfo, hover_builder::HoverBuilder, humanize_types::hover_const_type,
};

pub fn build_semantic_info_hover(
    semantic_model: &SemanticModel,
    db: &DbIndex,
    document: &LuaDocument,
    token: LuaSyntaxToken,
    semantic_info: SemanticInfo,
    range: TextRange,
) -> Option<Hover> {
    let typ = semantic_info.clone().typ;
    if semantic_info.semantic_decl.is_none() {
        return build_hover_without_property(db, document, token, typ);
    }
    let hover_builder = build_hover_content(
        semantic_model,
        db,
        Some(typ),
        semantic_info.semantic_decl.unwrap(),
        false,
        Some(token.clone()),
    );
    if let Some(hover_builder) = hover_builder {
        hover_builder.build_hover_result(document.to_lsp_range(range))
    } else {
        None
    }
}

fn build_hover_without_property(
    db: &DbIndex,
    document: &LuaDocument,
    token: LuaSyntaxToken,
    typ: LuaType,
) -> Option<Hover> {
    let render_level = db
        .get_emmyrc()
        .hover
        .custom_detail
        .map_or(RenderLevel::Detailed, |custom_detail| {
            RenderLevel::CustomDetailed(custom_detail)
        });

    let hover = humanize_type(db, &typ, render_level);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: hover,
        }),
        range: document.to_lsp_range(token.text_range()),
    })
}

pub fn build_hover_content_for_completion<'a>(
    semantic_model: &'a SemanticModel,
    db: &DbIndex,
    property_id: LuaSemanticDeclId,
    token: Option<LuaSyntaxToken>,
) -> Option<HoverBuilder<'a>> {
    let typ = match property_id {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            Some(semantic_model.get_type(decl_id.into()).clone())
        }
        LuaSemanticDeclId::Member(member_id) => {
            Some(semantic_model.get_type(member_id.into()).clone())
        }
        _ => None,
    };
    build_hover_content(semantic_model, db, typ, property_id, true, token)
}

fn build_hover_content<'a>(
    semantic_model: &'a SemanticModel,
    db: &DbIndex,
    typ: Option<LuaType>,
    property_id: LuaSemanticDeclId,
    is_completion: bool,
    token: Option<LuaSyntaxToken>,
) -> Option<HoverBuilder<'a>> {
    let mut builder = HoverBuilder::new(semantic_model, token, is_completion);
    match property_id {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let typ = typ?;
            build_decl_hover(&mut builder, db, typ, decl_id, is_completion)?;
        }
        LuaSemanticDeclId::Member(member_id) => {
            let typ = typ?;
            build_member_hover(&mut builder, db, typ, member_id, is_completion);
        }
        LuaSemanticDeclId::TypeDecl(type_decl_id) => {
            build_type_decl_hover(&mut builder, db, type_decl_id);
        }
        _ => return None,
    }
    Some(builder)
}

fn build_decl_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: LuaType,
    decl_id: LuaDeclId,
    is_completion: bool,
) -> Option<()> {
    let decl = db.get_decl_index().get_decl(&decl_id)?;

    let semantic_decls =
        find_decl_origin_owners(builder.semantic_model, decl_id).get_types(builder.semantic_model);

    // 处理类型签名
    if is_function(&typ) {
        let origin_decls = into_hover_decl_infos(semantic_decls);
        let hover_decl_context = HoverDeclContext::new(
            HoverDeclInfo::new(LuaSemanticDeclId::LuaDecl(decl_id), typ.clone()),
            origin_decls,
        );

        // 处理函数类型
        build_function_hover(builder, db, &hover_decl_context);

        if let Some(decl_info) = hover_decl_context
            .origin_decls()
            .iter()
            .find(|decl_info| matches!(decl_info.id(), LuaSemanticDeclId::Member(_)))
            && let LuaSemanticDeclId::Member(member_id) = decl_info.id()
        {
            let member = db.get_member_index().get_member(member_id);
            builder.set_location_path(member);
        }

        // `typ`此时可能是泛型实例化后的类型, 所以我们需要从member获取原始类型
        builder
            .add_signature_params_rets_description(builder.semantic_model.get_type(decl_id.into()));
    } else {
        let target_type = builder.semantic_model.get_type(decl_id.into()).clone();
        if typ.is_const() {
            let const_value = hover_const_type(db, &typ);
            let prefix = if decl.is_local() {
                "local "
            } else {
                "(global) "
            };
            builder.set_type_description(format!("{}{}: {}", prefix, decl.get_name(), const_value));
        } else {
            let decl_hover_type =
                get_assignment_hover_type(builder, builder.semantic_model, &target_type, &typ)
                    .unwrap_or(typ.clone());
            let type_humanize_text =
                hover_humanize_type(builder, &decl_hover_type, Some(builder.detail_render_level));
            let prefix = if decl.is_local() {
                "local "
            } else {
                "(global) "
            };
            builder.set_type_description(format!(
                "{}{}: {}",
                prefix,
                decl.get_name(),
                type_humanize_text
            ));
        }

        // 添加注释文本
        add_hover_descriptions(
            builder,
            LuaSemanticDeclId::LuaDecl(decl_id),
            &target_type,
            semantic_decls.iter().map(|(decl, typ)| (decl, typ)),
            is_completion,
        );
    }

    if let LuaDeclExtra::Param {
        idx, signature_id, ..
    } = &decl.extra
    {
        if let Some(signature) = db.get_signature_index().get(signature_id)
            && let Some(param_info) = signature.get_param_info_by_id(*idx)
            && let Some(description) = &param_info.description
        {
            builder.add_annotation_description(description.clone());
        }
    }

    Some(())
}

fn build_member_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: LuaType,
    member_id: LuaMemberId,
    is_completion: bool,
) -> Option<()> {
    let member = db.get_member_index().get_member(&member_id)?;
    let mut semantic_decls = find_member_origin_owners(builder.semantic_model, member_id, true)
        .get_types(builder.semantic_model);

    if let Some(token) = builder.get_trigger_token() {
        semantic_decls.retain(|(semantic_decl, _)| {
            builder
                .semantic_model
                .is_semantic_visible(token.clone(), semantic_decl.clone())
        });
    }

    let member_name = match member.get_key() {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(i) => format!("[{}]", i),
        _ => return None,
    };

    let origin_decls = into_hover_decl_infos(semantic_decls);
    let hover_decl_context = HoverDeclContext::new(
        HoverDeclInfo::new(LuaSemanticDeclId::Member(member_id), typ.clone()),
        origin_decls,
    );

    // 当为表字段时, 如果能够追溯到该成员的定义为 function, 那么我们也需要显示方法的签名而不是当前字段的真实类型
    if has_function_candidate(&hover_decl_context) {
        build_function_hover(builder, db, &hover_decl_context);

        builder.set_location_path(Some(member));

        // `typ`此时可能是泛型实例化后的类型, 所以我们需要从member获取原始类型
        builder.add_signature_params_rets_description(
            builder.semantic_model.get_type(member.get_id().into()),
        );
    } else {
        let target_type = builder
            .semantic_model
            .get_type(member.get_id().into())
            .clone();
        if typ.is_const() {
            let const_value = hover_const_type(db, &typ);
            builder.set_type_description(format!("(field) {}: {}", member_name, const_value));
            builder.set_location_path(Some(member));
        } else {
            let member_hover_type =
                get_assignment_hover_type(builder, builder.semantic_model, &target_type, &typ)
                    .unwrap_or(typ.clone());
            let level = if member_hover_type.is_module_ref() {
                builder.detail_render_level
            } else {
                RenderLevel::Simple
            };
            let type_humanize_text = hover_humanize_type(builder, &member_hover_type, Some(level));
            builder
                .set_type_description(format!("(field) {}: {}", member_name, type_humanize_text));
            builder.set_location_path(Some(member));
        }

        // 添加注释文本
        add_hover_descriptions(
            builder,
            LuaSemanticDeclId::Member(member.get_id()),
            &target_type,
            hover_decl_context
                .origin_decls()
                .iter()
                .map(|decl_info| (decl_info.id(), decl_info.typ())),
            is_completion,
        );
    }

    Some(())
}

fn add_hover_descriptions<'a, I>(
    builder: &mut HoverBuilder,
    primary_owner: LuaSemanticDeclId,
    target_type: &LuaType,
    origin_decls: I,
    is_completion: bool,
) where
    I: IntoIterator<Item = (&'a LuaSemanticDeclId, &'a LuaType)>,
{
    let mut description_owners = Vec::new();
    description_owners.push(primary_owner);
    collect_type_decl_description_owners(target_type, &mut description_owners);

    if !is_completion {
        for (origin_owner, origin_type) in origin_decls {
            if !description_owners.contains(origin_owner) {
                description_owners.push(origin_owner.clone());
            }
            collect_type_decl_description_owners(origin_type, &mut description_owners);
        }
    }

    for owner in &description_owners {
        builder.add_description(owner);
    }
}

fn collect_type_decl_description_owners(
    typ: &LuaType,
    description_owners: &mut Vec<LuaSemanticDeclId>,
) {
    match typ {
        LuaType::Def(type_decl_id) | LuaType::Ref(type_decl_id) => {
            push_type_decl_description_owner(description_owners, type_decl_id.clone());
        }
        LuaType::Generic(generic) => {
            push_type_decl_description_owner(description_owners, generic.get_base_type_id());
        }
        LuaType::Instance(instance) => {
            collect_type_decl_description_owners(instance.get_base(), description_owners);
        }
        LuaType::Union(union) => {
            for typ in union.into_vec() {
                collect_type_decl_description_owners(&typ, description_owners);
            }
        }
        LuaType::Intersection(intersection) => {
            for typ in intersection.get_types() {
                collect_type_decl_description_owners(typ, description_owners);
            }
        }
        _ => {}
    }
}

fn push_type_decl_description_owner(
    description_owners: &mut Vec<LuaSemanticDeclId>,
    type_decl_id: LuaTypeDeclId,
) {
    let owner = LuaSemanticDeclId::TypeDecl(type_decl_id);
    if !description_owners.contains(&owner) {
        description_owners.push(owner);
    }
}

fn into_hover_decl_infos(semantic_decls: Vec<(LuaSemanticDeclId, LuaType)>) -> Vec<HoverDeclInfo> {
    semantic_decls
        .into_iter()
        .map(|(semantic_decl_id, typ)| HoverDeclInfo::new(semantic_decl_id, typ))
        .collect()
}

pub fn add_signature_param_description(
    db: &DbIndex,
    marked_strings: &mut Vec<MarkedString>,
    signature_id: LuaSignatureId,
) -> Option<()> {
    let signature = db.get_signature_index().get(&signature_id)?;
    let param_count = signature.params.len();
    let mut s = String::new();
    for i in 0..param_count {
        let param_info = match signature.get_param_info_by_id(i) {
            Some(info) => info,
            None => continue,
        };

        if let Some(description) = &param_info.description {
            s.push_str(&format!(
                "@*param* `{}` — {}\n\n",
                param_info.name, description
            ));
        }
    }

    if !s.is_empty() {
        marked_strings.push(MarkedString::from_markdown(s));
    }
    Some(())
}

pub fn add_signature_ret_description(
    db: &DbIndex,
    marked_strings: &mut Vec<MarkedString>,
    signature_id: LuaSignatureId,
) -> Option<()> {
    let signature = db.get_signature_index().get(&signature_id)?;
    let mut s = String::new();
    for i in 0..signature.return_docs.len() {
        let ret_info = &signature.return_docs[i];
        if let Some(description) = ret_info.description.clone() {
            s.push_str(&format!(
                "@*return* {} — {}\n\n",
                match &ret_info.name {
                    Some(name) if !name.is_empty() => format!("`{}` ", name),
                    _ => "".to_string(),
                },
                description
            ));
        }
    }
    for ret_overload in &signature.return_overloads {
        let return_overload_types = ret_overload
            .type_refs
            .iter()
            .map(|ty| humanize_type(db, ty, RenderLevel::Simple))
            .collect::<Vec<_>>()
            .join(", ");
        let description = ret_overload.description.as_deref().unwrap_or_default();
        if description.is_empty() {
            s.push_str(&format!(
                "@*return_overload* `{}`\n\n",
                return_overload_types
            ));
        } else {
            s.push_str(&format!(
                "@*return_overload* `{}` — {}\n\n",
                return_overload_types, description
            ));
        }
    }
    if !s.is_empty() {
        marked_strings.push(MarkedString::from_markdown(s));
    }
    Some(())
}

fn get_assignment_hover_type(
    builder: &HoverBuilder,
    semantic_model: &SemanticModel,
    target_type: &LuaType,
    fallback_type: &LuaType,
) -> Option<LuaType> {
    let assign_stat = LuaAssignStat::cast(builder.get_trigger_token()?.parent()?.parent()?)?;
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    for (i, var) in vars.iter().enumerate() {
        if var
            .syntax()
            .text_range()
            .contains(builder.get_trigger_token()?.text_range().start())
        {
            let mut expr: Option<&LuaExpr> = exprs.get(i);
            let multi_return_index = if expr.is_none() {
                expr = Some(exprs.last()?);
                i + 1 - exprs.len()
            } else {
                0
            };

            let expr_type = semantic_model.infer_expr(expr.unwrap().clone());
            match expr_type {
                Ok(expr_type) => match expr_type {
                    LuaType::Variadic(muli_return) => {
                        let expr_type = muli_return.get_type(multi_return_index).cloned()?;
                        return select_assignment_hover_type(
                            semantic_model,
                            target_type,
                            fallback_type,
                            expr_type,
                        );
                    }
                    _ => {
                        return select_assignment_hover_type(
                            semantic_model,
                            target_type,
                            fallback_type,
                            expr_type,
                        );
                    }
                },
                Err(_) => return None,
            }
        }
    }

    None
}

fn select_assignment_hover_type(
    semantic_model: &SemanticModel,
    target_type: &LuaType,
    fallback_type: &LuaType,
    expr_type: LuaType,
) -> Option<LuaType> {
    let mut should_keep = false;
    if matches!(expr_type, LuaType::Table | LuaType::TableConst(_)) {
        let mut type_decl_description_owners = Vec::new();
        collect_type_decl_description_owners(target_type, &mut type_decl_description_owners);
        should_keep = !type_decl_description_owners.is_empty();
    }

    if should_keep {
        return Some(target_type.clone());
    }

    if semantic_model.type_check(target_type, &expr_type).is_ok() {
        return Some(expr_type);
    }

    Some(fallback_type.clone())
}
