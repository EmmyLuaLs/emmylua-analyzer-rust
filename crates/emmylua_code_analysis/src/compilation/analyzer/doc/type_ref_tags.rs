use emmylua_parser::{
    BinaryOperator, LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaDocDescriptionOwner, LuaDocTagAs,
    LuaDocTagCast, LuaDocTagModule, LuaDocTagOverload, LuaDocTagParam, LuaDocTagReturn,
    LuaDocTagType, LuaExpr, LuaLocalName, LuaNameToken, LuaVarExpr,
};
use smol_str::SmolStr;

use crate::{
    db_index::{
        LuaDeclId, LuaDocParamInfo, LuaDocReturnInfo, LuaMemberId, LuaOperator, LuaPropertyOwnerId,
        LuaSignatureId, LuaType,
    },
    InFiled, LuaFlowId, SignatureReturnStatus, TypeAssertion,
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
                        let decl = analyzer.db.get_decl_index_mut().get_decl_mut(&decl_id)?;

                        decl.set_decl_type(type_ref.clone());
                    }
                    LuaVarExpr::IndexExpr(index_expr) => {
                        let member_id =
                            LuaMemberId::new(index_expr.get_syntax_id(), analyzer.file_id);
                        let member = analyzer
                            .db
                            .get_member_index_mut()
                            .get_member_mut(&member_id)?;
                        member.set_decl_type(type_ref.clone());
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
                let decl = analyzer.db.get_decl_index_mut().get_decl_mut(&decl_id)?;

                decl.set_decl_type(type_ref.clone());
            }
        }
        LuaAst::LuaTableField(table_field) => {
            if let Some(first_type) = type_list.get(0) {
                let member_id = LuaMemberId::new(table_field.get_syntax_id(), analyzer.file_id);
                let member = analyzer
                    .db
                    .get_member_index_mut()
                    .get_member_mut(&member_id)?;
                member.set_decl_type(first_type.clone());
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
        type_ref = LuaType::Nullable(type_ref.into());
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

        let param_list = closure.get_params_list()?;
        for param in param_list.get_params() {
            let param_name = if let Some(name_token) = param.get_name_token() {
                name_token.get_name_text().to_string()
            } else {
                "...".to_string()
            };

            if param_name == name {
                let decl_id = LuaDeclId::new(analyzer.file_id, param.get_position());
                let decl = analyzer.db.get_decl_index_mut().get_decl_mut(&decl_id)?;

                decl.set_decl_type(type_ref);
                break;
            }
        }
    } else if let Some(LuaAst::LuaForRangeStat(for_range)) = analyzer.comment.get_owner() {
        for it_name_token in for_range.get_var_name_list() {
            let it_name = it_name_token.get_name_text();
            if it_name == name {
                let decl_id = LuaDeclId::new(analyzer.file_id, it_name_token.get_position());
                let decl = analyzer.db.get_decl_index_mut().get_decl_mut(&decl_id)?;

                decl.set_decl_type(type_ref);
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
        let operator =
            LuaOperator::new_call(decl_id.clone(), type_ref, analyzer.file_id, tag.get_range());
        analyzer.db.get_operator_index_mut().add_operator(operator);
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
    let decl_type = LuaType::Module(SmolStr::new(module_path).into());
    if let Some(owner) = get_owner_id(analyzer) {
        match owner {
            LuaPropertyOwnerId::LuaDecl(decl_id) => {
                let decl = analyzer.db.get_decl_index_mut().get_decl_mut(&decl_id)?;
                decl.set_decl_type(decl_type);
            }
            _ => {}
        }
    }
    Some(())
}

pub fn analyze_as(analyzer: &mut DocAnalyzer, tag: LuaDocTagAs) -> Option<()> {
    let as_type = tag.get_type()?;
    let type_ref = infer_type(analyzer, as_type);
    let owner = analyzer.comment.get_owner()?;
    let expr = LuaExpr::cast(owner.syntax().clone())?;
    let file_id = analyzer.file_id;
    let in_filed_syntax_id = InFiled::new(file_id, expr.get_syntax_id());
    analyzer
        .db
        .get_type_index_mut()
        .add_as_force_type(in_filed_syntax_id, type_ref);

    Some(())
}

pub fn analyze_cast(analyzer: &mut DocAnalyzer, tag: LuaDocTagCast) -> Option<()> {
    let name_token = tag.get_name_token();
    if let Some(name_token) = name_token {
        analyze_cast_with_name_token(analyzer, name_token, tag);
    }

    Some(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CastAction {
    Force,
    Add,
    Remove,
}

fn analyze_cast_with_name_token(
    analyzer: &mut DocAnalyzer,
    name_token: LuaNameToken,
    tag: LuaDocTagCast,
) -> Option<()> {
    let path = name_token.get_name_text();
    let effect_range = tag.ancestors::<LuaBlock>().next()?.get_range();
    let flow_id = LuaFlowId::from_node(tag.syntax());
    for cast_op_type in tag.get_op_types() {
        let action = match cast_op_type.get_op() {
            Some(op) => {
                if op.get_op() == BinaryOperator::OpAdd {
                    CastAction::Add
                } else {
                    CastAction::Remove
                }
            }
            None => CastAction::Force,
        };

        if cast_op_type.is_nullable() {
            let flow_chain = analyzer
                .db
                .get_flow_index_mut()
                .get_or_create_flow_chain(analyzer.file_id, flow_id);
            match action {
                CastAction::Add => {
                    flow_chain.add_type_assert(
                        path,
                        TypeAssertion::Add(LuaType::Nil),
                        effect_range,
                    );
                }
                CastAction::Remove => {
                    flow_chain.add_type_assert(
                        path,
                        TypeAssertion::Remove(LuaType::Nil),
                        effect_range,
                    );
                }
                _ => {}
            }
        } else if let Some(typ) = cast_op_type.get_type() {
            let typ = infer_type(analyzer, typ);
            let flow_chain = analyzer
                .db
                .get_flow_index_mut()
                .get_or_create_flow_chain(analyzer.file_id, flow_id);

            match action {
                CastAction::Add => {
                    flow_chain.add_type_assert(path, TypeAssertion::Add(typ), effect_range);
                }
                CastAction::Remove => {
                    flow_chain.add_type_assert(path, TypeAssertion::Remove(typ), effect_range);
                }
                CastAction::Force => {
                    flow_chain.add_type_assert(path, TypeAssertion::Narrow(typ), effect_range);
                }
            }
        }
    }

    Some(())
}
