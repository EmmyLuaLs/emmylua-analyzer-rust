use std::sync::Arc;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocDescriptionOwner, LuaDocFieldKey, LuaDocTagField,
    LuaDocTagOperator, LuaDocType, VisibilityKind,
};

use crate::{
    db_index::{
        LuaMember, LuaMemberKey, LuaMemberOwner, LuaOperator, LuaOperatorMetaMethod,
        LuaSemanticDeclId, LuaType,
    },
    AnalyzeError, DiagnosticCode, LuaFunctionType, LuaMemberFeature, LuaMemberId, LuaSignatureId,
    LuaTypeCache, OperatorFunction, TypeOps,
};

use super::{infer_type::infer_type, DocAnalyzer};

pub fn analyze_field(analyzer: &mut DocAnalyzer, tag: LuaDocTagField) -> Option<()> {
    let current_type_id = match &analyzer.current_type_id {
        Some(id) => id.clone(),
        None => {
            analyzer.db.get_diagnostic_index_mut().add_diagnostic(
                analyzer.file_id,
                AnalyzeError {
                    kind: DiagnosticCode::AnnotationUsageError,
                    message: t!("`@field` must be used under a `@class`").to_string(),
                    range: tag.get_range(),
                },
            );
            return None;
        }
    };

    let owner_id = LuaMemberOwner::Type(current_type_id.clone());
    let visibility_kind = if let Some(visibility_token) = tag.get_visibility_token() {
        Some(visibility_token.get_visibility())
    } else if let Some(visibility) = get_visibility_from_field_attrib(&tag) {
        Some(visibility)
    } else {
        None
    };

    let member_id = LuaMemberId::new(tag.get_syntax_id(), analyzer.file_id);

    let nullable = tag.is_nullable();
    let type_node = tag.get_type()?;
    let (mut field_type, property_owner) = match &type_node {
        LuaDocType::Func(doc_func) => {
            let typ = infer_type(analyzer, type_node.clone());
            let signature_id = LuaSignatureId::from_doc_func(analyzer.file_id, &doc_func);
            (typ, LuaSemanticDeclId::Signature(signature_id))
        }
        _ => (
            infer_type(analyzer, type_node),
            LuaSemanticDeclId::Member(member_id),
        ),
    };
    if nullable && !field_type.is_nullable() {
        field_type = TypeOps::Union.apply(analyzer.db, &field_type, &LuaType::Nil);
    }

    let description = if let Some(description) = tag.get_description() {
        Some(description.get_description_text().to_string())
    } else {
        None
    };

    let field_key = tag.get_field_key()?;
    let key = match field_key {
        LuaDocFieldKey::Name(name_token) => {
            LuaMemberKey::Name(name_token.get_name_text().to_string().into())
        }
        LuaDocFieldKey::String(string_token) => LuaMemberKey::Name(string_token.get_value().into()),
        LuaDocFieldKey::Integer(int_token) => LuaMemberKey::Integer(int_token.get_int_value()),
        LuaDocFieldKey::Type(doc_type) => {
            let range = doc_type.get_range();
            let key_type_ref = infer_type(analyzer, doc_type);
            if key_type_ref.is_unknown() {
                return None;
            }

            let operator = LuaOperator::new(
                current_type_id.clone().into(),
                LuaOperatorMetaMethod::Index,
                analyzer.file_id,
                range,
                OperatorFunction::Func(Arc::new(LuaFunctionType::new(
                    false,
                    false,
                    vec![
                        (
                            "self".to_string(),
                            Some(LuaType::Ref(current_type_id.clone())),
                        ),
                        ("key".to_string(), Some(key_type_ref)),
                    ],
                    field_type,
                ))),
            );
            analyzer.db.get_operator_index_mut().add_operator(operator);
            return Some(());
        }
    };

    let decl_feature = if analyzer.is_meta {
        LuaMemberFeature::MetaFieldDecl
    } else {
        LuaMemberFeature::FileFieldDecl
    };

    let member = LuaMember::new(member_id, key.clone(), decl_feature, None);
    analyzer.db.get_reference_index_mut().add_index_reference(
        key,
        analyzer.file_id,
        tag.get_syntax_id(),
    );

    analyzer
        .db
        .get_member_index_mut()
        .add_member(owner_id, member);

    analyzer.db.get_type_index_mut().bind_type(
        member_id.clone().into(),
        LuaTypeCache::DocType(field_type.clone()),
    );

    if let Some(visibility_kind) = visibility_kind {
        analyzer.db.get_property_index_mut().add_visibility(
            analyzer.file_id,
            property_owner.clone(),
            visibility_kind,
        );
    }

    if let Some(description) = description {
        analyzer.db.get_property_index_mut().add_description(
            analyzer.file_id,
            property_owner.clone(),
            description,
        );
    }

    Some(())
}

pub fn analyze_operator(analyzer: &mut DocAnalyzer, tag: LuaDocTagOperator) -> Option<()> {
    let current_type_id = analyzer.current_type_id.clone()?;
    let name_token = tag.get_name_token()?;
    let op_kind = LuaOperatorMetaMethod::from_operator_name(name_token.get_name_text())?;
    let mut operands: Vec<(String, Option<LuaType>)> = tag
        .get_param_list()?
        .get_types()
        .enumerate()
        .map(|(i, doc_type)| (format!("arg{}", i), Some(infer_type(analyzer, doc_type))))
        .collect();

    operands.insert(
        0,
        (
            "self".to_string(),
            Some(LuaType::Ref(current_type_id.clone())),
        ),
    );

    let return_type = if let Some(return_type) = tag.get_return_type() {
        infer_type(analyzer, return_type)
    } else {
        LuaType::Unknown
    };

    let operator = LuaOperator::new(
        current_type_id.into(),
        op_kind,
        analyzer.file_id,
        name_token.get_range(),
        OperatorFunction::Func(Arc::new(LuaFunctionType::new(
            false,
            false,
            operands,
            return_type,
        ))),
    );

    analyzer.db.get_operator_index_mut().add_operator(operator);

    Some(())
}

fn get_visibility_from_field_attrib(tag: &LuaDocTagField) -> Option<VisibilityKind> {
    if let Some(attrib) = tag.get_attrib() {
        for token in attrib.get_attrib_tokens() {
            let visibility = VisibilityKind::to_visibility_kind(token.get_name_text());
            if visibility != VisibilityKind::None {
                return Some(visibility);
            }
        }
    }

    None
}
