use emmylua_code_analysis::{
    DbIndex, LuaDeclId, LuaDocument, LuaMember, LuaMemberId, LuaMemberKey, LuaSemanticDeclId,
    LuaSignatureId, LuaType, LuaTypeDeclId, RenderLevel, SemanticDeclLevel, SemanticInfo,
    SemanticModel,
};
use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaExpr, LuaSyntaxKind, LuaSyntaxToken, LuaTableExpr, LuaTableField,
};
use lsp_types::{Hover, HoverContents, MarkedString, MarkupContent};
use std::collections::HashSet;

use emmylua_code_analysis::humanize_type;

use crate::handlers::hover::{
    function_humanize::{is_function, try_extract_signature_id_from_field},
    hover_humanize::hover_humanize_type,
};

use super::{
    hover_builder::HoverBuilder,
    hover_humanize::{hover_const_type, hover_function_type},
};

pub fn build_semantic_info_hover(
    semantic_model: &SemanticModel,
    db: &DbIndex,
    document: &LuaDocument,
    token: LuaSyntaxToken,
    semantic_info: SemanticInfo,
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
        hover_builder.build_hover_result(document.to_lsp_range(token.text_range()))
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
    let hover = humanize_type(db, &typ, RenderLevel::Detailed);
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
) -> Option<HoverBuilder<'a>> {
    let typ = match property_id {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            // let decl = db.get_decl_index().get_decl(&decl_id)?;
            Some(semantic_model.get_type(decl_id.into()).clone())
        }
        LuaSemanticDeclId::Member(member_id) => {
            // let member = db.get_member_index().get_member(&member_id)?;
            Some(semantic_model.get_type(member_id.into()).clone())
            // Some(member.get_decl_type())
        }
        _ => None,
    };
    build_hover_content(semantic_model, db, typ, property_id, true, None)
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
            build_decl_hover(&mut builder, db, typ, decl_id);
        }
        LuaSemanticDeclId::Member(member_id) => {
            let typ = typ?;
            build_member_hover(&mut builder, db, typ, member_id);
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
) -> Option<()> {
    let decl = db.get_decl_index().get_decl(&decl_id)?;

    let mut origin_member = None;
    let mut origin_decl = None;

    // 处理类型签名
    if is_function(&typ) {
        let origin_decl_id = find_decl_origin_owner(builder.semantic_model, decl_id.clone());
        match origin_decl_id {
            Some(LuaSemanticDeclId::Member(member_id)) => {
                origin_member = Some(db.get_member_index().get_member(&member_id).unwrap());
            }
            Some(LuaSemanticDeclId::LuaDecl(decl_id)) => {
                origin_decl = Some(db.get_decl_index().get_decl(&decl_id).unwrap());
            }
            _ => {}
        }
        hover_function_type(
            builder,
            db,
            &typ,
            origin_member,
            if let Some(owner_decl) = origin_decl {
                owner_decl.get_name()
            } else {
                decl.get_name()
            },
            if let Some(owner_decl) = origin_decl {
                owner_decl.is_local()
            } else {
                decl.is_local()
            },
        );

        builder.set_location_path(origin_member);
    } else if typ.is_const() {
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
            hover_humanize_type(builder, &decl_hover_type, Some(RenderLevel::Detailed));
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

    builder.add_annotation_description("---".to_string());

    // 如果`decl`没有描述, 则尝试从`owner_member/owner_decl`获取描述
    builder
        .add_description(LuaSemanticDeclId::LuaDecl(decl_id))
        .or_else(|| {
            origin_member.and_then(|m: &LuaMember| {
                builder.add_description(LuaSemanticDeclId::Member(m.get_id()))
            })
        })
        .or_else(|| {
            origin_decl
                .and_then(|d| builder.add_description(LuaSemanticDeclId::LuaDecl(d.get_id())))
        });

    builder.add_signature_params_rets_description(typ);
    Some(())
}

fn build_member_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: LuaType,
    member_id: LuaMemberId,
) -> Option<()> {
    let mut member = db.get_member_index().get_member(&member_id)?;

    let mut origin_decl = None;
    match find_member_origin_owner(&builder.semantic_model, member_id) {
        Some(LuaSemanticDeclId::Member(member_id)) => {
            member = db.get_member_index().get_member(&member_id)?;
        }
        Some(LuaSemanticDeclId::LuaDecl(decl_id)) => {
            origin_decl = Some(db.get_decl_index().get_decl(&decl_id)?);
        }
        _ => {}
    }

    let member_name = match member.get_key() {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(i) => format!("[{}]", i),
        _ => return None,
    };

    if is_function(&typ) {
        hover_function_type(
            builder,
            db,
            &typ,
            if origin_decl.is_none() {
                Some(&member)
            } else {
                None
            },
            if let Some(owner_decl) = origin_decl {
                owner_decl.get_name()
            } else {
                &member_name
            },
            if let Some(owner_decl) = origin_decl {
                owner_decl.is_local()
            } else {
                false
            },
        );

        builder.set_location_path(Some(&member));
    } else if typ.is_const() {
        let const_value = hover_const_type(db, &typ);
        builder.set_type_description(format!("(field) {}: {}", member_name, const_value));
        builder.set_location_path(Some(&member));
    } else {
        let member_hover_type =
            get_hover_type(builder, builder.semantic_model).unwrap_or(typ.clone());
        let type_humanize_text =
            hover_humanize_type(builder, &member_hover_type, Some(RenderLevel::Simple));
        builder.set_type_description(format!("(field) {}: {}", member_name, type_humanize_text));
        builder.set_location_path(Some(&member));
    }

    builder.add_annotation_description("---".to_string());

    // 添加注释文本
    origin_decl.and_then(|d| builder.add_description(LuaSemanticDeclId::LuaDecl(d.get_id())));
    builder.add_description(LuaSemanticDeclId::Member(member.get_id()));

    if let Some(signature_id) = try_extract_signature_id_from_field(builder.semantic_model, &member)
    {
        builder.add_description(LuaSemanticDeclId::Signature(signature_id));
    }

    builder.add_signature_params_rets_description(typ);
    Some(())
}

fn build_type_decl_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    type_decl_id: LuaTypeDeclId,
) -> Option<()> {
    let type_decl = db.get_type_index().get_type_decl(&type_decl_id)?;
    let type_description = if type_decl.is_alias() {
        if let Some(origin) = type_decl.get_alias_origin(db, None) {
            let origin_type = humanize_type(db, &origin, RenderLevel::Detailed);
            format!("(alias) {} = {}", type_decl.get_name(), origin_type)
        } else {
            "".to_string()
        }
    } else if type_decl.is_enum() {
        format!("(enum) {}", type_decl.get_name())
    } else {
        let humanize_text = humanize_type(
            db,
            &LuaType::Def(type_decl_id.clone()),
            RenderLevel::Detailed,
        );
        format!("(class) {}", humanize_text)
    };

    builder.set_type_description(type_description);
    builder.add_description(LuaSemanticDeclId::TypeDecl(type_decl_id));
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
    marked_strings.push(MarkedString::from_markdown(s));
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
                        return muli_return.get_type(multi_return_index).map(|t| t.clone());
                    }
                    _ => return Some(expr_type),
                },
                Err(_) => return None,
            }
        }
    }

    None
}

pub fn find_decl_origin_owner(
    semantic_model: &SemanticModel,
    decl_id: LuaDeclId,
) -> Option<LuaSemanticDeclId> {
    let root = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&decl_id.file_id)?
        .get_red_root();
    let node = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?
        .get_value_syntax_id()?
        .to_node_from_root(&root)?;
    let semantic_decl = semantic_model.find_decl(node.into(), SemanticDeclLevel::default());
    match semantic_decl {
        Some(LuaSemanticDeclId::Member(member_id)) => {
            find_member_origin_owner(semantic_model, member_id).or(semantic_decl)
        }
        Some(LuaSemanticDeclId::LuaDecl(_)) => semantic_decl,
        _ => None,
    }
}
pub fn find_member_origin_owner(
    semantic_model: &SemanticModel,
    member_id: LuaMemberId,
) -> Option<LuaSemanticDeclId> {
    const MAX_ITERATIONS: usize = 50;
    let mut visited_members = HashSet::new();

    let mut current_owner = resolve_member_owner(semantic_model, &member_id);
    let mut final_owner = current_owner.clone();
    let mut iteration_count = 0;

    while let Some(LuaSemanticDeclId::Member(current_member_id)) = &current_owner {
        if visited_members.contains(current_member_id) || iteration_count >= MAX_ITERATIONS {
            break;
        }

        visited_members.insert(current_member_id.clone());
        iteration_count += 1;

        match resolve_member_owner(semantic_model, current_member_id) {
            Some(next_owner) => {
                final_owner = Some(next_owner.clone());
                current_owner = Some(next_owner);
            }
            None => break,
        }
    }

    final_owner
}

fn resolve_member_owner(
    semantic_model: &SemanticModel,
    member_id: &LuaMemberId,
) -> Option<LuaSemanticDeclId> {
    let root = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&member_id.file_id)?
        .get_red_root();
    let current_node = member_id.get_syntax_id().to_node_from_root(&root)?;
    match member_id.get_syntax_id().get_kind() {
        LuaSyntaxKind::TableFieldAssign => {
            if LuaTableField::can_cast(current_node.kind().into()) {
                let table_field = LuaTableField::cast(current_node.clone())?;
                // 如果表是类, 那么通过类型推断获取 owner
                if let Some(owner_id) =
                    resolve_table_field_through_type_inference(semantic_model, &table_field)
                {
                    return Some(owner_id);
                }
                // 非类, 那么通过右值推断
                let value_expr = table_field.get_value_expr()?;
                let value_node = value_expr.get_syntax_id().to_node_from_root(&root)?;
                semantic_model.find_decl(value_node.into(), SemanticDeclLevel::default())
            } else {
                None
            }
        }
        LuaSyntaxKind::IndexExpr => {
            let assign_node = current_node.parent()?;
            let assign_stat = LuaAssignStat::cast(assign_node)?;
            let (vars, exprs) = assign_stat.get_var_and_expr_list();

            for (var, expr) in vars.iter().zip(exprs.iter()) {
                if var.syntax().text_range() == current_node.text_range() {
                    let expr_node = expr.get_syntax_id().to_node_from_root(&root)?;
                    return semantic_model
                        .find_decl(expr_node.into(), SemanticDeclLevel::default());
                }
            }
            None
        }
        _ => None,
    }
}

fn resolve_table_field_through_type_inference(
    semantic_model: &SemanticModel,
    table_field: &LuaTableField,
) -> Option<LuaSemanticDeclId> {
    let parent = table_field.syntax().parent()?;
    let table_expr = LuaTableExpr::cast(parent)?;
    let table_type = semantic_model.infer_table_should_be(table_expr)?;

    if !matches!(table_type, LuaType::Ref(_) | LuaType::Def(_)) {
        return None;
    }

    let field_key = table_field.get_field_key()?;
    let key = semantic_model.get_member_key(&field_key)?;
    let member_infos = semantic_model.get_member_infos(&table_type)?;

    member_infos
        .iter()
        .find(|m| m.key == key)?
        .property_owner_id
        .clone()
}
