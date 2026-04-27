use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaCommentOwner, LuaDocDescription,
    LuaDocDescriptionOwner, LuaDocGenericDeclList, LuaDocTagAlias, LuaDocTagAttribute,
    LuaDocTagClass, LuaDocTagEnum, LuaDocTagGeneric, LuaFuncStat, LuaLocalName, LuaLocalStat,
    LuaNameExpr, LuaSyntaxId, LuaSyntaxKind, LuaTokenKind, LuaVarExpr,
};
use rowan::TextRange;
use smol_str::SmolStr;

use super::{
    DocAnalyzer, infer_type::infer_type, preprocess_description, tags::find_owner_closure,
};
use crate::GenericParam;
use crate::compilation::analyzer::doc::tags::report_orphan_tag;
use crate::{
    LuaTypeCache, LuaTypeDeclId,
    compilation::analyzer::common::bind_type,
    db_index::{
        LuaDeclId, LuaGenericParamInfo, LuaMemberId, LuaSemanticDeclId, LuaSignatureId, LuaType,
    },
};
use std::sync::Arc;
use std::vec;

pub fn analyze_class(analyzer: &mut DocAnalyzer, tag: LuaDocTagClass) -> Option<()> {
    let file_id = analyzer.file_id;
    let name = tag.get_name_token()?.get_name_text().to_string();

    let class_decl =
        analyzer
            .db
            .get_type_index()
            .find_type_decl(file_id, &name, Some(analyzer.workspace_id))?;

    let class_decl_id = class_decl.get_id();
    analyzer.current_type_id = Some(class_decl_id.clone());
    if let Some(generic_params) = tag.get_generic_decl() {
        let generic_params = get_generic_params(analyzer, generic_params);

        if generic_params
            .iter()
            .any(|param| param.type_constraint.is_some())
        {
            analyzer
                .db
                .get_type_index_mut()
                .add_generic_params(class_decl_id.clone(), generic_params.clone());
        }

        add_generic_index(analyzer, generic_params, &tag);
    }

    if let Some(supers) = tag.get_supers() {
        for super_doc_type in supers.get_types() {
            let super_type = infer_type(analyzer, super_doc_type);
            if super_type.is_unknown() {
                continue;
            }

            analyzer.db.get_type_index_mut().add_super_type(
                class_decl_id.clone(),
                file_id,
                super_type,
            );
        }
    }

    add_description_for_type_decl(analyzer, &class_decl_id, tag.get_descriptions());

    bind_def_type(analyzer, LuaType::Def(class_decl_id.clone()));
    Some(())
}

fn add_description_for_type_decl(
    analyzer: &mut DocAnalyzer,
    type_decl_id: &LuaTypeDeclId,
    descriptions: Vec<LuaDocDescription>,
) {
    let mut description_text = String::new();
    for description in descriptions {
        let description = preprocess_description(&description.get_description_text(), None);
        if !description.is_empty() {
            if !description_text.is_empty() {
                description_text.push_str("\n\n");
            }

            description_text.push_str(&description);
        }
    }

    analyzer.db.get_property_index_mut().add_description(
        analyzer.file_id,
        LuaSemanticDeclId::TypeDecl(type_decl_id.clone()),
        description_text,
    );
}

pub fn analyze_enum(analyzer: &mut DocAnalyzer, tag: LuaDocTagEnum) -> Option<()> {
    let file_id = analyzer.file_id;
    let name = tag.get_name_token()?.get_name_text().to_string();

    let enum_decl_id = {
        let enum_decl = analyzer.db.get_type_index().find_type_decl(
            file_id,
            &name,
            Some(analyzer.workspace_id),
        )?;
        if !enum_decl.is_enum() {
            return None;
        }
        enum_decl.get_id()
    };

    analyzer.current_type_id = Some(enum_decl_id.clone());

    if let Some(base_type) = tag.get_base_type() {
        let base_type = infer_type(analyzer, base_type);
        if base_type.is_unknown() {
            return None;
        }

        let enum_decl = analyzer
            .db
            .get_type_index_mut()
            .get_type_decl_mut(&enum_decl_id)?;
        enum_decl.add_enum_base(base_type);
    }

    add_description_for_type_decl(analyzer, &enum_decl_id, tag.get_descriptions());

    bind_def_type(analyzer, LuaType::Def(enum_decl_id.clone()));

    Some(())
}

pub fn analyze_alias(analyzer: &mut DocAnalyzer, tag: LuaDocTagAlias) -> Option<()> {
    let file_id = analyzer.file_id;
    let name = tag.get_name_token()?.get_name_text().to_string();

    let alias_decl_id = {
        let alias_decl = analyzer.db.get_type_index().find_type_decl(
            file_id,
            &name,
            Some(analyzer.workspace_id),
        )?;
        if !alias_decl.is_alias() {
            return None;
        }

        alias_decl.get_id()
    };

    if let Some(generic_params) = tag.get_generic_decl_list() {
        let generic_params = get_generic_params(analyzer, generic_params);

        if generic_params
            .iter()
            .any(|param| param.type_constraint.is_some())
        {
            analyzer
                .db
                .get_type_index_mut()
                .add_generic_params(alias_decl_id.clone(), generic_params.clone());
        }
        let range = analyzer.comment.get_range();
        let scope_id = analyzer.generic_index.add_generic_scope(vec![range], false);
        analyzer
            .generic_index
            .append_generic_params(scope_id, generic_params);
    }

    let origin_type = infer_type(analyzer, tag.get_type()?);

    let alias = analyzer
        .db
        .get_type_index_mut()
        .get_type_decl_mut(&alias_decl_id)?;

    alias.add_alias_origin(origin_type);

    add_description_for_type_decl(analyzer, &alias_decl_id, tag.get_descriptions());

    Some(())
}

/// 分析属性定义
pub fn analyze_attribute(analyzer: &mut DocAnalyzer, tag: LuaDocTagAttribute) -> Option<()> {
    let file_id = analyzer.file_id;
    let name = tag.get_name_token()?.get_name_text().to_string();

    let decl_id = {
        let decl = analyzer.db.get_type_index().find_type_decl(
            file_id,
            &name,
            Some(analyzer.workspace_id),
        )?;
        if !decl.is_attribute() {
            return None;
        }
        decl.get_id()
    };
    let attribute_type = infer_type(analyzer, tag.get_type()?);
    let attribute_decl = analyzer
        .db
        .get_type_index_mut()
        .get_type_decl_mut(&decl_id)?;
    attribute_decl.add_attribute_type(attribute_type);

    add_description_for_type_decl(analyzer, &decl_id, tag.get_descriptions());
    Some(())
}

fn get_generic_params(
    analyzer: &mut DocAnalyzer,
    params: LuaDocGenericDeclList,
) -> Vec<GenericParam> {
    analyzer.generic_index.clear_pending_type_params();
    let mut params_result = Vec::new();
    for param in params.get_generic_decl() {
        let name = if let Some(param) = param.get_name_token() {
            SmolStr::new(param.get_name_text())
        } else {
            continue;
        };

        let type_constraint = param
            .get_constraint_type()
            .map(|type_ref| infer_type(analyzer, type_ref));

        let param = GenericParam::new(name, type_constraint, None);
        analyzer
            .generic_index
            .append_pending_type_param(param.clone());

        params_result.push(param);
    }
    analyzer.generic_index.clear_pending_type_params();

    params_result
}

fn add_generic_index(
    analyzer: &mut DocAnalyzer,
    generic_params: Vec<GenericParam>,
    tag: &LuaDocTagClass,
) {
    let mut ranges = Vec::new();
    ranges.push(tag.get_effective_range());
    if let Some(comment_owner) = analyzer.comment.get_owner() {
        let range = comment_owner.get_range();
        ranges.push(range);
        match comment_owner {
            LuaAst::LuaLocalStat(local_stat) => {
                if let Some(result) = get_local_stat_reference_ranges(analyzer, local_stat) {
                    ranges.extend(result);
                }
            }
            LuaAst::LuaAssignStat(assign_stat) => {
                if let Some(result) = get_global_reference_ranges(analyzer, assign_stat) {
                    ranges.extend(result);
                }
            }
            _ => {}
        }
    }

    let scope_id = analyzer.generic_index.add_generic_scope(ranges, false);
    analyzer
        .generic_index
        .append_generic_params(scope_id, generic_params);
}

fn get_local_stat_reference_ranges(
    analyzer: &mut DocAnalyzer,
    local_stat: LuaLocalStat,
) -> Option<Vec<TextRange>> {
    let file_id = analyzer.file_id;
    let first_local = local_stat.child::<LuaLocalName>()?;
    let decl_id = LuaDeclId::new(file_id, first_local.get_position());
    let mut ranges = Vec::new();
    let decl_ref = analyzer
        .db
        .get_reference_index_mut()
        .get_decl_references(&file_id, &decl_id)?;
    for decl_ref in &decl_ref.cells {
        let syntax_id = LuaSyntaxId::new(LuaSyntaxKind::NameExpr.into(), decl_ref.range);
        let name_node = syntax_id.to_node_from_root(&analyzer.root)?;
        if let Some(parent1) = name_node.parent()
            && parent1.kind() == LuaSyntaxKind::IndexExpr.into()
            && let Some(parent2) = parent1.parent()
        {
            if parent2.kind() == LuaSyntaxKind::FuncStat.into() {
                ranges.push(parent2.text_range());
                let stat = LuaFuncStat::cast(parent2)?;
                for comment in stat.get_comments() {
                    ranges.push(comment.get_range());
                }
            } else if parent2.kind() == LuaSyntaxKind::AssignStat.into() {
                let stat = LuaAssignStat::cast(parent2)?;
                if let Some(assign_token) = stat.get_assign_op()
                    && assign_token.get_position() > decl_ref.range.start()
                {
                    ranges.push(stat.get_range());
                    for comment in stat.get_comments() {
                        ranges.push(comment.get_range());
                    }
                }
            }
        }
    }

    Some(ranges)
}

fn get_global_reference_ranges(
    analyzer: &mut DocAnalyzer,
    assign_stat: LuaAssignStat,
) -> Option<Vec<TextRange>> {
    let file_id = analyzer.file_id;
    let name_token = assign_stat.child::<LuaNameExpr>()?.get_name_token()?;
    let name = name_token.get_name_text().to_string();
    let mut ranges = Vec::new();

    let ref_syntax_ids = analyzer
        .db
        .get_reference_index_mut()
        .get_global_file_references(&name, file_id)?;
    for syntax_id in ref_syntax_ids {
        let name_node = syntax_id.to_node_from_root(&analyzer.root)?;
        if let Some(parent1) = name_node.parent()
            && parent1.kind() == LuaSyntaxKind::IndexExpr.into()
            && let Some(parent2) = parent1.parent()
        {
            if parent2.kind() == LuaSyntaxKind::FuncStat.into() {
                ranges.push(parent2.text_range());
                let stat = LuaFuncStat::cast(parent2)?;
                for comment in stat.get_comments() {
                    ranges.push(comment.get_range());
                }
            } else if parent2.kind() == LuaSyntaxKind::AssignStat.into() {
                let stat = LuaAssignStat::cast(parent2)?;
                if let Some(assign_token) = stat.token_by_kind(LuaTokenKind::TkAssign)
                    && assign_token.get_position() > syntax_id.get_range().start()
                {
                    ranges.push(stat.get_range());
                    for comment in stat.get_comments() {
                        ranges.push(comment.get_range());
                    }
                }
            }
        }
    }

    Some(ranges)
}

pub fn analyze_func_generic(analyzer: &mut DocAnalyzer, tag: LuaDocTagGeneric) -> Option<()> {
    let Some(comment_owner) = analyzer.comment.get_owner() else {
        report_orphan_tag(analyzer, &tag);
        return None;
    };

    let scope_id = analyzer.generic_index.add_generic_scope(
        vec![analyzer.comment.get_range(), comment_owner.get_range()],
        true,
    );

    let mut param_info = Vec::new();
    if let Some(params_list) = tag.get_generic_decl_list() {
        for param in params_list.get_generic_decl() {
            let Some(name_token) = param.get_name_token() else {
                continue;
            };
            let name_text = name_token.get_name_text().to_string();
            let smol_name = SmolStr::new(name_text.as_str());
            analyzer
                .generic_index
                .append_generic_param(scope_id, GenericParam::new(smol_name.clone(), None, None));

            let type_ref = param
                .get_constraint_type()
                .map(|type_ref| infer_type(analyzer, type_ref));

            analyzer.generic_index.set_param_constraint(
                scope_id,
                name_text.as_str(),
                type_ref.clone(),
            );

            param_info.push(Arc::new(LuaGenericParamInfo::new(
                name_text, type_ref, None,
            )));
        }
    }

    let closure = find_owner_closure(analyzer)?;
    let signature_id = LuaSignatureId::from_closure(analyzer.file_id, &closure);
    let signature = analyzer
        .db
        .get_signature_index_mut()
        .get_or_create(signature_id);
    signature.generic_params = param_info;

    Some(())
}

fn bind_def_type(analyzer: &mut DocAnalyzer, type_def: LuaType) -> Option<()> {
    let owner = analyzer.comment.get_owner()?;
    match owner {
        LuaAst::LuaLocalStat(local_stat) => {
            let local_name = local_stat.child::<LuaLocalName>()?;
            let position = local_name.get_position();
            let file_id = analyzer.file_id;
            let decl_id = LuaDeclId::new(file_id, position);

            bind_type(analyzer.db, decl_id.into(), LuaTypeCache::DocType(type_def));
        }
        LuaAst::LuaAssignStat(assign_stat) => {
            if let LuaVarExpr::NameExpr(name_expr) = assign_stat.child::<LuaVarExpr>()? {
                let position = name_expr.get_position();
                let file_id = analyzer.file_id;
                let decl_id = LuaDeclId::new(file_id, position);
                bind_type(analyzer.db, decl_id.into(), LuaTypeCache::DocType(type_def));
            } else if let LuaVarExpr::IndexExpr(index_expr) = assign_stat.child::<LuaVarExpr>()? {
                let member_id = LuaMemberId::new(index_expr.get_syntax_id(), analyzer.file_id);
                bind_type(
                    analyzer.db,
                    member_id.into(),
                    LuaTypeCache::DocType(type_def),
                );
            }
        }
        LuaAst::LuaTableField(field) => {
            let member_id = LuaMemberId::new(field.get_syntax_id(), analyzer.file_id);
            bind_type(
                analyzer.db,
                member_id.into(),
                LuaTypeCache::DocType(type_def),
            );
        }
        _ => {}
    }
    Some(())
}
