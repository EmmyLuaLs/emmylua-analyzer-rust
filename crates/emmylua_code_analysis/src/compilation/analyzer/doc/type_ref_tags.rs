use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaDocDescriptionOwner, LuaDocTagAs, LuaDocTagCast,
    LuaDocTagModule, LuaDocTagOther, LuaDocTagOverload, LuaDocTagParam, LuaDocTagReturn,
    LuaDocTagSee, LuaDocTagType, LuaExpr, LuaLocalName, LuaTokenKind, LuaVarExpr,
};

use crate::{
    compilation::analyzer::{bind_type::bind_type, unresolve::UnResolveModuleRef},
    db_index::{
        LuaDeclId, LuaDocParamInfo, LuaDocReturnInfo, LuaMemberId, LuaOperator, LuaSemanticDeclId,
        LuaSignatureId, LuaType,
    },
    InFiled, InferFailReason, LuaOperatorMetaMethod, LuaTypeCache, OperatorFunction,
    SignatureReturnStatus, TypeOps,
};

use super::{
    infer_type::infer_type,
    preprocess_description,
    tags::{find_owner_closure, get_owner_id},
    DocAnalyzer,
};

pub fn analyze_type(analyzer: &mut DocAnalyzer, tag: LuaDocTagType) -> Option<()> {
    let mut type_list = Vec::new();
    for lua_doc_type in tag.get_type_list() {
        let type_ref = infer_type(analyzer, lua_doc_type);
        type_list.push(type_ref);
    }

    // bind ref type
    let owner = analyzer.comment.get_owner()?;
    match owner {
        LuaAst::LuaAssignStat(assign_stat) => {
            let (vars, _) = assign_stat.get_var_and_expr_list();
            let min_len = vars.len().min(type_list.len());
            for i in 0..min_len {
                let var_expr = vars.get(i)?;
                let type_ref = type_list.get(i)?;
                match var_expr {
                    LuaVarExpr::NameExpr(name_expr) => {
                        let name_token = name_expr.get_name_token()?;
                        let position = name_token.get_position();
                        let file_id = analyzer.file_id;
                        let decl_id = LuaDeclId::new(file_id, position);
                        analyzer
                            .db
                            .get_type_index_mut()
                            .bind_type(decl_id.into(), LuaTypeCache::DocType(type_ref.clone()));
                    }
                    LuaVarExpr::IndexExpr(index_expr) => {
                        let member_id =
                            LuaMemberId::new(index_expr.get_syntax_id(), analyzer.file_id);
                        analyzer
                            .db
                            .get_type_index_mut()
                            .bind_type(member_id.into(), LuaTypeCache::DocType(type_ref.clone()));
                    }
                }
            }
        }
        LuaAst::LuaLocalStat(local_assign_stat) => {
            let local_list: Vec<LuaLocalName> = local_assign_stat.get_local_name_list().collect();
            let min_len = local_list.len().min(type_list.len());
            for i in 0..min_len {
                let local_name = local_list.get(i)?;
                let type_ref = type_list.get(i)?;
                let name_token = local_name.get_name_token()?;
                let position = name_token.get_position();
                let file_id = analyzer.file_id;
                let decl_id = LuaDeclId::new(file_id, position);

                analyzer
                    .db
                    .get_type_index_mut()
                    .bind_type(decl_id.into(), LuaTypeCache::DocType(type_ref.clone()));
            }
        }
        LuaAst::LuaTableField(table_field) => {
            if let Some(first_type) = type_list.get(0) {
                let member_id = LuaMemberId::new(table_field.get_syntax_id(), analyzer.file_id);

                analyzer
                    .db
                    .get_type_index_mut()
                    .bind_type(member_id.into(), LuaTypeCache::DocType(first_type.clone()));
            }
        }
        _ => {}
    }

    Some(())
}

pub fn analyze_param(analyzer: &mut DocAnalyzer, tag: LuaDocTagParam) -> Option<()> {
    let name = if let Some(name) = tag.get_name_token() {
        name.get_name_text().to_string()
    } else if tag.is_vararg() {
        "...".to_string()
    } else {
        return None;
    };

    let nullable = tag.is_nullable();
    let mut type_ref = if let Some(lua_doc_type) = tag.get_type() {
        infer_type(analyzer, lua_doc_type)
    } else {
        return None;
    };

    if nullable && !type_ref.is_nullable() {
        type_ref = TypeOps::Union.apply(analyzer.db, &type_ref, &LuaType::Nil);
    }

    let description = if let Some(des) = tag.get_description() {
        Some(preprocess_description(&des.get_description_text()))
    } else {
        None
    };

    // bind type ref to signature and param
    if let Some(closure) = find_owner_closure(analyzer) {
        let id = LuaSignatureId::from_closure(analyzer.file_id, &closure);
        let signature = analyzer.db.get_signature_index_mut().get_or_create(id);
        let param_info = LuaDocParamInfo {
            name: name.clone(),
            type_ref: type_ref.clone(),
            nullable,
            description,
        };

        let idx = signature.find_param_idx(&name)?;

        signature.param_docs.insert(idx, param_info);
    } else if let Some(LuaAst::LuaForRangeStat(for_range)) = analyzer.comment.get_owner() {
        for it_name_token in for_range.get_var_name_list() {
            let it_name = it_name_token.get_name_text();
            if it_name == name {
                let decl_id = LuaDeclId::new(analyzer.file_id, it_name_token.get_position());

                analyzer
                    .db
                    .get_type_index_mut()
                    .bind_type(decl_id.into(), LuaTypeCache::DocType(type_ref));
                break;
            }
        }
    }

    Some(())
}

pub fn analyze_return(analyzer: &mut DocAnalyzer, tag: LuaDocTagReturn) -> Option<()> {
    let description = if let Some(des) = tag.get_description() {
        Some(preprocess_description(&des.get_description_text()))
    } else {
        None
    };

    if let Some(closure) = find_owner_closure(analyzer) {
        let signature_id = LuaSignatureId::from_closure(analyzer.file_id, &closure);
        let returns = tag.get_type_and_name_list();
        for (doc_type, name_token) in returns {
            let name = if let Some(name) = name_token {
                Some(name.get_name_text().to_string())
            } else {
                None
            };

            let type_ref = infer_type(analyzer, doc_type);
            let return_info = LuaDocReturnInfo {
                name,
                type_ref,
                description: description.clone(),
            };

            let signature = analyzer
                .db
                .get_signature_index_mut()
                .get_or_create(signature_id);
            signature.return_docs.push(return_info);
            signature.resolve_return = SignatureReturnStatus::DocResolve;
        }
    }
    Some(())
}

pub fn analyze_overload(analyzer: &mut DocAnalyzer, tag: LuaDocTagOverload) -> Option<()> {
    if let Some(decl_id) = analyzer.current_type_id.clone() {
        let type_ref = infer_type(analyzer, tag.get_type()?);
        match type_ref {
            LuaType::DocFunction(func) => {
                let operator = LuaOperator::new(
                    decl_id.clone().into(),
                    LuaOperatorMetaMethod::Call,
                    analyzer.file_id,
                    tag.get_range(),
                    OperatorFunction::Func(func.clone()),
                );
                analyzer.db.get_operator_index_mut().add_operator(operator);
            }
            _ => {}
        }
    } else if let Some(closure) = find_owner_closure(analyzer) {
        let type_ref = infer_type(analyzer, tag.get_type()?);
        match type_ref {
            LuaType::DocFunction(func) => {
                let id = LuaSignatureId::from_closure(analyzer.file_id, &closure);
                let signature = analyzer.db.get_signature_index_mut().get_or_create(id);
                signature.overloads.push(func);
            }
            _ => {}
        }
    }
    Some(())
}

pub fn analyze_module(analyzer: &mut DocAnalyzer, tag: LuaDocTagModule) -> Option<()> {
    let module_path = tag.get_string_token()?.get_value();
    let module_info = analyzer.db.get_module_index().find_module(&module_path)?;
    let export_type = module_info.export_type.clone();
    let module_file_id = module_info.file_id;
    let owner_id = get_owner_id(analyzer)?;
    if let Some(export_type) = export_type {
        match &owner_id {
            LuaSemanticDeclId::LuaDecl(decl_id) => {
                analyzer.db.get_type_index_mut().bind_type(
                    decl_id.clone().into(),
                    LuaTypeCache::DocType(export_type.clone()),
                );
            }
            LuaSemanticDeclId::Member(member_id) => {
                analyzer.db.get_type_index_mut().bind_type(
                    member_id.clone().into(),
                    LuaTypeCache::DocType(export_type.clone()),
                );
            }
            _ => {}
        }
    } else {
        let unresolve = UnResolveModuleRef {
            module_file_id,
            owner_id,
            reason: InferFailReason::None,
        };

        analyzer.context.add_unresolve(unresolve.into());
    }

    Some(())
}

pub fn analyze_as(analyzer: &mut DocAnalyzer, tag: LuaDocTagAs) -> Option<()> {
    let as_type = tag.get_type()?;
    let type_ref = infer_type(analyzer, as_type);
    let comment = analyzer.comment.clone();
    let mut left_token = comment.syntax().first_token()?.prev_token()?;
    if left_token.kind() == LuaTokenKind::TkWhitespace.into() {
        left_token = left_token.prev_token()?;
    }

    let mut ast_node = left_token.parent()?;
    loop {
        if LuaExpr::can_cast(ast_node.kind().into()) {
            break;
        } else if LuaBlock::can_cast(ast_node.kind().into()) {
            return None;
        }
        ast_node = ast_node.parent()?;
    }
    let expr = LuaExpr::cast(ast_node)?;

    let file_id = analyzer.file_id;
    let in_filed_syntax_id = InFiled::new(file_id, expr.get_syntax_id());
    bind_type(
        analyzer.db,
        in_filed_syntax_id.into(),
        LuaTypeCache::DocType(type_ref),
    );

    Some(())
}

pub fn analyze_cast(analyzer: &mut DocAnalyzer, tag: LuaDocTagCast) -> Option<()> {
    for op in tag.get_op_types() {
        if let Some(doc_type) = op.get_type() {
            let typ = infer_type(analyzer, doc_type.clone());
            analyzer.context.cast_flow.insert(
                InFiled {
                    file_id: analyzer.file_id,
                    value: doc_type.get_syntax_id(),
                },
                typ,
            );
        }
    }
    Some(())
}

pub fn analyze_see(analyzer: &mut DocAnalyzer, tag: LuaDocTagSee) -> Option<()> {
    let owner = get_owner_id(analyzer)?;
    let content = tag.get_see_content()?;
    let text = content.get_text();

    analyzer
        .db
        .get_property_index_mut()
        .add_see(analyzer.file_id, owner, text.to_string());

    Some(())
}

pub fn analyze_other(analyzer: &mut DocAnalyzer, other: LuaDocTagOther) -> Option<()> {
    let owner = get_owner_id(analyzer)?;
    let tag_name = other.get_tag_name()?;
    let description = if let Some(des) = other.get_description() {
        let description = preprocess_description(&des.get_description_text());
        format!("@*{}* {}", tag_name, description)
    } else {
        format!("@*{}*", tag_name)
    };

    analyzer
        .db
        .get_property_index_mut()
        .add_other(analyzer.file_id, owner, description);

    Some(())
}
