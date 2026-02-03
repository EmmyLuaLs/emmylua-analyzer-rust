use std::collections::HashSet;

use emmylua_code_analysis::humanize_type;
use emmylua_code_analysis::{
    DbIndex, LuaCompilation, LuaDeclExtra, LuaDeclId, LuaDocument, LuaMemberId, LuaMemberKey,
    LuaSemanticDeclId, LuaSignatureId, LuaType, RenderLevel, SemanticInfo, SemanticModel,
};
use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaCallArgList, LuaExpr, LuaSyntaxKind, LuaSyntaxToken,
    LuaTableExpr, LuaTableField,
};
use lsp_types::{Hover, HoverContents, MarkedString, MarkupContent};
use rowan::TextRange;

use crate::handlers::hover::function::{build_function_hover, is_function};
use crate::handlers::hover::humanize_type_decl::build_type_decl_hover;
use crate::handlers::hover::humanize_types::hover_humanize_type;

use super::{
    find_origin::{find_decl_origin_owners, find_member_origin_owners},
    hover_builder::HoverBuilder,
    humanize_types::hover_const_type,
};

pub fn build_semantic_info_hover(
    compilation: &LuaCompilation,
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
        compilation,
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
    compilation: &'a LuaCompilation,
    semantic_model: &'a SemanticModel,
    db: &DbIndex,
    property_id: LuaSemanticDeclId,
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
    build_hover_content(
        compilation,
        semantic_model,
        db,
        typ,
        property_id,
        true,
        None,
    )
}

fn build_hover_content<'a>(
    compilation: &'a LuaCompilation,
    semantic_model: &'a SemanticModel,
    db: &DbIndex,
    typ: Option<LuaType>,
    property_id: LuaSemanticDeclId,
    is_completion: bool,
    token: Option<LuaSyntaxToken>,
) -> Option<HoverBuilder<'a>> {
    let mut builder = HoverBuilder::new(compilation, semantic_model, token, is_completion);
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

    let mut semantic_decls =
        find_decl_origin_owners(builder.compilation, builder.semantic_model, decl_id)
            .get_types(builder.semantic_model);

    // 处理类型签名
    if is_function(&typ) {
        adjust_semantic_decls(
            builder,
            &mut semantic_decls,
            &LuaSemanticDeclId::LuaDecl(decl_id),
            &typ,
        );

        // 处理函数类型
        build_function_hover(builder, db, &semantic_decls);
        // hover_function_type(builder, db, &semantic_decls);

        if let Some((LuaSemanticDeclId::Member(member_id), _)) = semantic_decls
            .iter()
            .find(|(decl, _)| matches!(decl, LuaSemanticDeclId::Member(_)))
        {
            let member = db.get_member_index().get_member(member_id);
            builder.set_location_path(member);
        }

        // `typ`此时可能是泛型实例化后的类型, 所以我们需要从member获取原始类型
        builder
            .add_signature_params_rets_description(builder.semantic_model.get_type(decl_id.into()));
    } else {
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
                get_hover_type(builder, builder.semantic_model).unwrap_or(typ.clone());
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
        let mut semantic_decl_set = HashSet::new();
        let decl_decl = LuaSemanticDeclId::LuaDecl(decl_id);
        semantic_decl_set.insert(&decl_decl);
        if !is_completion {
            semantic_decl_set.extend(semantic_decls.iter().map(|(decl, _)| decl));
        }
        for semantic_decl in semantic_decl_set {
            builder.add_description(semantic_decl);
        }
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
    let mut semantic_decls =
        find_member_origin_owners(builder.compilation, builder.semantic_model, member_id, true)
            .get_types(builder.semantic_model);

    let member_name = match member.get_key() {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(i) => format!("[{}]", i),
        _ => return None,
    };

    if is_function(&typ) {
        adjust_semantic_decls(
            builder,
            &mut semantic_decls,
            &LuaSemanticDeclId::Member(member_id),
            &typ,
        );

        build_function_hover(builder, db, &semantic_decls);

        builder.set_location_path(Some(member));

        // `typ`此时可能是泛型实例化后的类型, 所以我们需要从member获取原始类型
        builder.add_signature_params_rets_description(
            builder.semantic_model.get_type(member.get_id().into()),
        );
    } else {
        if typ.is_const() {
            let const_value = hover_const_type(db, &typ);
            builder.set_type_description(format!("(field) {}: {}", member_name, const_value));
            builder.set_location_path(Some(member));
        } else {
            let member_hover_type =
                get_hover_type(builder, builder.semantic_model).unwrap_or(typ.clone());
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
        let mut semantic_decl_set = HashSet::new();
        let member_decl = LuaSemanticDeclId::Member(member.get_id());
        semantic_decl_set.insert(&member_decl);
        if !is_completion {
            semantic_decl_set.extend(semantic_decls.iter().map(|(decl, _)| decl));
        }
        for semantic_decl in semantic_decl_set {
            builder.add_description(semantic_decl);
        }
    }

    Some(())
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
    if !s.is_empty() {
        marked_strings.push(MarkedString::from_markdown(s));
    }
    Some(())
}

pub fn get_hover_type(builder: &HoverBuilder, semantic_model: &SemanticModel) -> Option<LuaType> {
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
                        return muli_return.get_type(multi_return_index).cloned();
                    }
                    _ => return Some(expr_type),
                },
                Err(_) => return None,
            }
        }
    }

    None
}

#[allow(unused)]
fn adjust_semantic_decls(
    builder: &mut HoverBuilder,
    semantic_decls: &mut Vec<(LuaSemanticDeclId, LuaType)>,
    current_semantic_decl_id: &LuaSemanticDeclId,
    current_type: &LuaType,
) -> Option<()> {
    if let Some(pos) = semantic_decls
        .iter()
        .position(|(_, typ)| current_type == typ)
    {
        let item = semantic_decls.remove(pos);
        semantic_decls.push(item);
        return Some(());
    }
    // semantic_decls 是追溯最初定义的结果, 不包含当前内容
    let current_len = semantic_decls.len();
    if current_len == 0 {
        // 没有最初定义, 直接添加原始内容
        semantic_decls.push((current_semantic_decl_id.clone(), current_type.clone()));
        return Some(());
    }
    // 此时有最初定义, 证明当前内容的是派生的或者全部项实例化后联合的结果, 非常难以区分
    // 如果当前定义是 LuaDecl 且追溯到了最初定义, 那么我们不需要添加
    if let LuaSemanticDeclId::LuaDecl(_) = current_semantic_decl_id {
        return Some(());
    }

    // 如果当前定义在最初定义组中存在, 那么我们也不需要添加.
    // 具有一个难以解决的问题, 返回的`current_semantic_decl_id`为 member 时, 不一定是当前 token 指向的内容, 因此我们还需要再做一层判断,
    // 如果是具有实际定义的, 我们仍然需要添加, 例如 signature.
    if semantic_decls
        .iter()
        .any(|(decl, typ)| decl == current_semantic_decl_id && !typ.is_signature())
    {
        return Some(());
    }

    if has_add_to_semantic_decls(builder, current_semantic_decl_id).unwrap_or(true) {
        semantic_decls.push((current_semantic_decl_id.clone(), current_type.clone()));
    };

    Some(())
}

fn has_add_to_semantic_decls(
    builder: &mut HoverBuilder,
    semantic_decl_id: &LuaSemanticDeclId,
) -> Option<bool> {
    if let LuaSemanticDeclId::Member(member_id) = semantic_decl_id {
        let semantic_model = if member_id.file_id == builder.semantic_model.get_file_id() {
            builder.semantic_model
        } else {
            &builder.compilation.get_semantic_model(member_id.file_id)?
        };

        let root = semantic_model.get_root().syntax();
        let current_node = member_id.get_syntax_id().to_node_from_root(root)?;
        if member_id.get_syntax_id().get_kind() == LuaSyntaxKind::TableFieldAssign {
            if LuaTableField::can_cast(current_node.kind().into()) {
                let table_field = LuaTableField::cast(current_node.clone())?;
                let parent = table_field.syntax().parent()?;
                let table_expr = LuaTableExpr::cast(parent)?;
                let table_type = semantic_model.infer_table_should_be(table_expr.clone())?;
                if matches!(table_type, LuaType::Ref(_) | LuaType::Generic(_)) {
                    // 如果位于函数调用中, 则不添加
                    let is_in_call = table_expr.ancestors::<LuaCallArgList>().next().is_some();
                    return Some(!is_in_call);
                }
            }
        };
    }

    Some(true)
}
