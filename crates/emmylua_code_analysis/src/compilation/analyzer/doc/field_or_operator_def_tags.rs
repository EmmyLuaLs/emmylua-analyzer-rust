use std::ops::Deref;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocDescriptionOwner, LuaDocFieldKey, LuaDocTagField,
    LuaDocTagOperator, LuaDocType,
};

use crate::{
    db_index::{
        LuaMember, LuaMemberKey, LuaMemberOwner, LuaOperator, LuaOperatorMetaMethod,
        LuaPropertyOwnerId, LuaType,
    },
    AnalyzeError, DiagnosticCode, LuaDocParamInfo, LuaDocReturnInfo, LuaMemberId, LuaSignatureId,
    SignatureReturnStatus,
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

    let member_owner = LuaMemberOwner::Type(current_type_id.clone());
    let visibility_kind = if let Some(visibility_token) = tag.get_visibility_token() {
        Some(visibility_token.get_visibility())
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
            (typ, LuaPropertyOwnerId::Signature(signature_id))
        }
        _ => (
            infer_type(analyzer, type_node),
            LuaPropertyOwnerId::Member(member_id),
        ),
    };

    if nullable && !field_type.is_nullable() {
        field_type = LuaType::Nullable(field_type.into());
    }

    let description = if let Some(description) = tag.get_description() {
        Some(description.get_description_text().to_string())
    } else {
        None
    };

    let field_key = tag.get_field_key()?;
    let (key, member) = match field_key {
        LuaDocFieldKey::Name(name_token) => {
            let key = LuaMemberKey::Name(name_token.get_name_text().to_string().into());
            let member = LuaMember::new(
                member_owner,
                key.clone(),
                analyzer.file_id,
                tag.get_syntax_id(),
                Some(field_type),
            );

            (key, member)
        }
        LuaDocFieldKey::String(string_token) => {
            let key = LuaMemberKey::Name(string_token.get_value().into());
            let member = LuaMember::new(
                member_owner,
                key.clone(),
                analyzer.file_id,
                tag.get_syntax_id(),
                Some(field_type),
            );

            (key, member)
        }
        LuaDocFieldKey::Integer(int_token) => {
            let key = LuaMemberKey::Integer(int_token.get_int_value());
            let member = LuaMember::new(
                member_owner,
                key.clone(),
                analyzer.file_id,
                tag.get_syntax_id(),
                Some(field_type),
            );

            (key, member)
        }
        LuaDocFieldKey::Type(doc_type) => {
            let range = doc_type.get_range();
            let key_type_ref = infer_type(analyzer, doc_type);
            if key_type_ref.is_unknown() {
                return None;
            }

            let operator = LuaOperator::new(
                current_type_id.clone(),
                LuaOperatorMetaMethod::Index,
                vec![key_type_ref],
                field_type,
                analyzer.file_id,
                range,
            );
            analyzer.db.get_operator_index_mut().add_operator(operator);
            return Some(());
        }
    };

    analyzer.db.get_reference_index_mut().add_index_reference(
        key,
        analyzer.file_id,
        tag.get_syntax_id(),
    );

    match &property_owner {
        LuaPropertyOwnerId::Signature(signature_id) => {
            merge_signature_member(analyzer, member, signature_id.clone());
        }
        LuaPropertyOwnerId::Member(_) => {
            analyzer.db.get_member_index_mut().add_member(member);
        }
        _ => {}
    }

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

fn merge_signature_member(
    analyzer: &mut DocAnalyzer,
    mut member: LuaMember,
    signature_id: LuaSignatureId,
) -> Option<()> {
    let key = member.get_key();
    let owner = member.get_owner();

    if let Some(old_member) = analyzer
        .db
        .get_member_index()
        .get_member_from_owner(&owner, &key)
    {
        let old_type = old_member.get_decl_type().clone();
        match old_type {
            LuaType::Signature(old_signature_id) => {
                let signatrue = analyzer
                    .db
                    .get_signature_index_mut()
                    .get_mut(&old_signature_id)?;
                if let LuaType::DocFunction(f) = member.get_decl_type() {
                    signatrue.overloads.push(f.clone());
                }

                return Some(());
            }
            _ => {
                analyzer.db.get_diagnostic_index_mut().add_diagnostic(
                    analyzer.file_id,
                    AnalyzeError {
                        kind: DiagnosticCode::AnnotationUsageError,
                        message: t!("`@field` with signature type can't be used with other types")
                            .to_string(),
                        range: member.get_range(),
                    },
                );
            }
        }
    }

    // create signatrue from doc func type
    let signature = analyzer
        .db
        .get_signature_index_mut()
        .get_or_create(signature_id.clone());

    let decl_type = member.get_decl_type();
    let (f, nullable) = match decl_type {
        LuaType::DocFunction(f) => (f, false),
        LuaType::Nullable(base) => {
            if let LuaType::DocFunction(f) = base.deref() {
                (f.clone(), true)
            } else {
                return None;
            }
        }
        _ => {
            return None;
        }
    };

    signature.is_colon_define = f.is_colon_define();
    for (i, (name, typ)) in f.get_params().iter().enumerate() {
        signature.params.push(name.clone());
        signature.param_docs.insert(
            i,
            LuaDocParamInfo {
                name: name.clone(),
                type_ref: typ.clone().unwrap_or(LuaType::Any),
                nullable: false,
                description: None,
            },
        );
    }

    for typ in f.get_ret() {
        signature.return_docs.push(LuaDocReturnInfo {
            name: None,
            type_ref: typ.clone(),
            description: None,
        });
        signature.resolve_return = SignatureReturnStatus::DocResolve;
    }

    if f.is_async() {
        let property_owner = LuaPropertyOwnerId::Signature(signature_id.clone());
        analyzer
            .db
            .get_property_index_mut()
            .add_async(analyzer.file_id, property_owner);
    }

    let mut member_type = LuaType::Signature(signature_id);
    if nullable {
        member_type = LuaType::Nullable(member_type.into());
    }

    member.set_decl_type(member_type);
    analyzer.db.get_member_index_mut().add_member(member);
    Some(())
}

pub fn analyze_operator(analyzer: &mut DocAnalyzer, tag: LuaDocTagOperator) -> Option<()> {
    let current_type_id = analyzer.current_type_id.clone()?;
    let name_token = tag.get_name_token()?;
    let op_kind = LuaOperatorMetaMethod::from_str(name_token.get_name_text())?;
    let operands: Vec<LuaType> = tag
        .get_param_list()?
        .get_types()
        .map(|operand| infer_type(analyzer, operand))
        .collect();

    let return_type = if let Some(return_type) = tag.get_return_type() {
        infer_type(analyzer, return_type)
    } else {
        LuaType::Unknown
    };

    let operator = LuaOperator::new(
        current_type_id,
        op_kind,
        operands,
        return_type,
        analyzer.file_id,
        name_token.get_range(),
    );

    analyzer.db.get_operator_index_mut().add_operator(operator);

    Some(())
}
