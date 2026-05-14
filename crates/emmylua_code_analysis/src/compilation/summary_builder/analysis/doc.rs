use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaChunk, LuaComment, LuaDocAttributeUse,
    LuaDocDescriptionOwner, LuaDocFieldKey, LuaDocGenericDeclList, LuaDocTag, LuaDocType,
    LuaDocTypeFlag, NumberResult, VisibilityKind,
};
use smol_str::SmolStr;

use super::super::{
    SalsaDocFieldSummary, SalsaDocGenericParamSummary, SalsaDocGenericSummary,
    SalsaDocOperatorSummary, SalsaDocOwnerKindSummary, SalsaDocOwnerSummary, SalsaDocParamSummary,
    SalsaDocReturnItemSummary, SalsaDocReturnSummary, SalsaDocSummary, SalsaDocTagDataSummary,
    SalsaDocTagFieldKeySummary, SalsaDocTagKindSummary, SalsaDocTagSummary,
    SalsaDocTypeDefKindSummary, SalsaDocTypeDefSummary, SalsaDocTypeNodeKey,
    SalsaDocTypeTagSummary, SalsaDocVersionConditionSummary, SalsaDocVisibilityKindSummary,
};

pub fn analyze_doc_summary(chunk: LuaChunk) -> SalsaDocSummary {
    let comments = chunk.descendants::<LuaComment>().collect::<Vec<_>>();
    let mut type_defs = Vec::new();
    let mut type_tags = Vec::new();
    let mut fields = Vec::new();
    let mut generics = Vec::new();
    let mut params = Vec::new();
    let mut returns = Vec::new();
    let mut operators = Vec::new();
    let mut tags = Vec::new();

    for comment in &comments {
        let owner = build_owner_summary(comment.get_owner());
        for tag in comment.get_doc_tags() {
            match tag {
                LuaDocTag::Class(class_tag) => type_defs.push(SalsaDocTypeDefSummary {
                    owner,
                    name: class_tag
                        .get_name_token()
                        .map(|token| token.get_name_text().into())
                        .unwrap_or_default(),
                    kind: SalsaDocTypeDefKindSummary::Class,
                    visibility: type_def_visibility(class_tag.get_type_flag()),
                    syntax_offset: class_tag.get_position(),
                    generic_params: collect_generic_params(class_tag.get_generic_decl()),
                    super_type_offsets: class_tag
                        .get_supers()
                        .into_iter()
                        .flat_map(|supers| supers.get_types())
                        .map(SalsaDocTypeNodeKey::from)
                        .collect(),
                    value_type_offset: None,
                }),
                LuaDocTag::Enum(enum_tag) => type_defs.push(SalsaDocTypeDefSummary {
                    owner,
                    name: enum_tag
                        .get_name_token()
                        .map(|token| token.get_name_text().into())
                        .unwrap_or_default(),
                    kind: SalsaDocTypeDefKindSummary::Enum,
                    visibility: type_def_visibility(enum_tag.get_type_flag()),
                    syntax_offset: enum_tag.get_position(),
                    generic_params: Vec::new(),
                    super_type_offsets: Vec::new(),
                    value_type_offset: enum_tag.get_base_type().map(SalsaDocTypeNodeKey::from),
                }),
                LuaDocTag::Alias(alias_tag) => type_defs.push(SalsaDocTypeDefSummary {
                    owner,
                    name: alias_tag
                        .get_name_token()
                        .map(|token| token.get_name_text().into())
                        .unwrap_or_default(),
                    kind: SalsaDocTypeDefKindSummary::Alias,
                    visibility: type_def_visibility(alias_tag.get_type_flag()),
                    syntax_offset: alias_tag.get_position(),
                    generic_params: collect_generic_params(alias_tag.get_generic_decl_list()),
                    super_type_offsets: Vec::new(),
                    value_type_offset: alias_tag.get_type().map(SalsaDocTypeNodeKey::from),
                }),
                LuaDocTag::Attribute(attribute_tag) => type_defs.push(SalsaDocTypeDefSummary {
                    owner,
                    name: attribute_tag
                        .get_name_token()
                        .map(|token| token.get_name_text().into())
                        .unwrap_or_default(),
                    kind: SalsaDocTypeDefKindSummary::Attribute,
                    visibility: SalsaDocVisibilityKindSummary::Public,
                    syntax_offset: attribute_tag.get_position(),
                    generic_params: Vec::new(),
                    super_type_offsets: Vec::new(),
                    value_type_offset: attribute_tag.get_type().map(SalsaDocTypeNodeKey::from),
                }),
                LuaDocTag::Type(type_tag) => type_tags.push(SalsaDocTypeTagSummary {
                    owner,
                    syntax_offset: type_tag.get_position(),
                    type_offsets: type_tag.get_type_list().map(type_offset).collect(),
                    description: description_text(&type_tag),
                }),
                LuaDocTag::Field(field_tag) => fields.push(SalsaDocFieldSummary {
                    owner,
                    syntax_offset: field_tag.get_position(),
                    key: field_tag.get_field_key().map(field_key_summary),
                    type_offset: field_tag.get_type().map(type_offset),
                    visibility: field_tag
                        .get_visibility_token()
                        .and_then(|token| token.get_visibility())
                        .map(visibility_summary),
                    description: description_text(&field_tag),
                    is_nullable: field_tag.is_nullable(),
                }),
                LuaDocTag::Generic(generic_tag) => generics.push(SalsaDocGenericSummary {
                    owner,
                    syntax_offset: generic_tag.get_position(),
                    params: collect_generic_params(generic_tag.get_generic_decl_list()),
                }),
                LuaDocTag::Param(param_tag) => {
                    let type_info = param_tag.get_type();
                    let is_nullable = param_tag.is_nullable()
                        || matches!(type_info, Some(LuaDocType::Nullable(_)));
                    params.push(SalsaDocParamSummary {
                        owner,
                        name: param_tag
                            .get_name_token()
                            .map(|token| token.get_name_text().into())
                            .unwrap_or_else(|| {
                                if param_tag.is_vararg() {
                                    SmolStr::new("...")
                                } else {
                                    SmolStr::new("")
                                }
                            }),
                        syntax_offset: param_tag.get_position(),
                        type_offset: type_info.map(SalsaDocTypeNodeKey::from),
                        is_nullable,
                        is_vararg: param_tag.is_vararg(),
                    })
                }
                LuaDocTag::Return(return_tag) => returns.push(SalsaDocReturnSummary {
                    owner,
                    syntax_offset: return_tag.get_position(),
                    items: return_tag
                        .get_info_list()
                        .into_iter()
                        .map(|(doc_type, name_token)| SalsaDocReturnItemSummary {
                            name: name_token.map(|token| token.get_name_text().into()),
                            type_offset: SalsaDocTypeNodeKey::from(doc_type),
                        })
                        .collect(),
                }),
                LuaDocTag::Operator(operator_tag) => operators.push(SalsaDocOperatorSummary {
                    owner,
                    name: operator_tag
                        .get_name_token()
                        .map(|token| token.get_name_text().into())
                        .unwrap_or_default(),
                    syntax_offset: operator_tag.get_position(),
                    param_type_offsets: operator_tag
                        .get_param_list()
                        .into_iter()
                        .flat_map(|param_list| param_list.get_types())
                        .map(SalsaDocTypeNodeKey::from)
                        .collect(),
                    return_type_offset: operator_tag
                        .get_return_type()
                        .map(SalsaDocTypeNodeKey::from),
                }),
                LuaDocTag::AttributeUse(attribute_use_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::AttributeUse,
                    syntax_offset: attribute_use_tag.get_position(),
                    data: SalsaDocTagDataSummary::Items(
                        attribute_use_tag
                            .get_attribute_uses()
                            .filter_map(|attribute_use| attribute_use_name(&attribute_use))
                            .collect(),
                    ),
                }),
                LuaDocTag::ReturnOverload(return_overload_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::ReturnOverload,
                    syntax_offset: return_overload_tag.get_position(),
                    data: SalsaDocTagDataSummary::TypeOffsets(
                        return_overload_tag.get_types().map(type_offset).collect(),
                    ),
                }),
                LuaDocTag::Overload(overload_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Overload,
                    syntax_offset: overload_tag.get_position(),
                    data: SalsaDocTagDataSummary::TypeOffsets(
                        overload_tag
                            .get_type()
                            .map(type_offset)
                            .into_iter()
                            .collect(),
                    ),
                }),
                LuaDocTag::Module(module_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Module,
                    syntax_offset: module_tag.get_position(),
                    data: SalsaDocTagDataSummary::Name(
                        module_tag
                            .get_string_token()
                            .map(|token| token.get_value().into()),
                    ),
                }),
                LuaDocTag::See(see_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::See,
                    syntax_offset: see_tag.get_position(),
                    data: SalsaDocTagDataSummary::Content(
                        see_tag
                            .get_see_content()
                            .map(|content| content.syntax().text().into()),
                    ),
                }),
                LuaDocTag::Diagnostic(diagnostic_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Diagnostic,
                    syntax_offset: diagnostic_tag.get_position(),
                    data: SalsaDocTagDataSummary::NameItems {
                        name: diagnostic_tag
                            .get_action_token()
                            .map(|token| token.get_name_text().into()),
                        items: diagnostic_tag
                            .get_code_list()
                            .into_iter()
                            .flat_map(|codes| codes.get_codes())
                            .map(|code| code.get_name_text().into())
                            .collect(),
                    },
                }),
                LuaDocTag::Deprecated(deprecated_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Deprecated,
                    syntax_offset: deprecated_tag.get_position(),
                    data: SalsaDocTagDataSummary::Content(description_text(&deprecated_tag)),
                }),
                LuaDocTag::Version(version_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Version,
                    syntax_offset: version_tag.get_position(),
                    data: SalsaDocTagDataSummary::VersionConds(
                        version_tag
                            .get_version_list()
                            .filter_map(|version| {
                                version.get_version_condition().map(|cond| {
                                    SalsaDocVersionConditionSummary {
                                        text: cond.to_string().into(),
                                    }
                                })
                            })
                            .collect(),
                    ),
                }),
                LuaDocTag::Cast(cast_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Cast,
                    syntax_offset: cast_tag.get_position(),
                    data: SalsaDocTagDataSummary::NameTypeOffsets {
                        name: cast_tag
                            .get_key_expr()
                            .map(|expr| expr.syntax().text().to_string().into()),
                        type_offsets: cast_tag
                            .get_op_types()
                            .filter_map(|op_type| op_type.get_type().map(type_offset))
                            .collect(),
                    },
                }),
                LuaDocTag::Source(source_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Source,
                    syntax_offset: source_tag.get_position(),
                    data: SalsaDocTagDataSummary::Content(
                        source_tag
                            .get_path_token()
                            .map(|path| path.get_path().into()),
                    ),
                }),
                LuaDocTag::Schema(schema_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Schema,
                    syntax_offset: schema_tag.get_position(),
                    data: SalsaDocTagDataSummary::Content(
                        schema_tag
                            .get_path_token()
                            .map(|path| path.get_path().into()),
                    ),
                }),
                LuaDocTag::Other(other_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Other,
                    syntax_offset: other_tag.get_position(),
                    data: SalsaDocTagDataSummary::NameContent {
                        name: other_tag.get_tag_name().map(Into::into),
                        content: description_text(&other_tag),
                    },
                }),
                LuaDocTag::Namespace(namespace_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Namespace,
                    syntax_offset: namespace_tag.get_position(),
                    data: SalsaDocTagDataSummary::Name(
                        namespace_tag
                            .get_name_token()
                            .map(|token| token.get_name_text().into()),
                    ),
                }),
                LuaDocTag::Using(using_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Using,
                    syntax_offset: using_tag.get_position(),
                    data: SalsaDocTagDataSummary::Name(
                        using_tag
                            .get_name_token()
                            .map(|token| token.get_name_text().into()),
                    ),
                }),
                LuaDocTag::Meta(meta_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Meta,
                    syntax_offset: meta_tag.get_position(),
                    data: SalsaDocTagDataSummary::Name(
                        meta_tag
                            .get_name_token()
                            .map(|token| token.get_name_text().into()),
                    ),
                }),
                LuaDocTag::Nodiscard(nodiscard_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Nodiscard,
                    syntax_offset: nodiscard_tag.get_position(),
                    data: SalsaDocTagDataSummary::Content(description_text(&nodiscard_tag)),
                }),
                LuaDocTag::Readonly(readonly_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Readonly,
                    syntax_offset: readonly_tag.get_position(),
                    data: SalsaDocTagDataSummary::Content(description_text(&readonly_tag)),
                }),
                LuaDocTag::Async(async_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Async,
                    syntax_offset: async_tag.get_position(),
                    data: SalsaDocTagDataSummary::None,
                }),
                LuaDocTag::As(as_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::As,
                    syntax_offset: as_tag.get_position(),
                    data: SalsaDocTagDataSummary::TypeOffsets(
                        as_tag.get_type().map(type_offset).into_iter().collect(),
                    ),
                }),
                LuaDocTag::Visibility(visibility_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Visibility,
                    syntax_offset: visibility_tag.get_position(),
                    data: SalsaDocTagDataSummary::Visibility(
                        visibility_tag
                            .get_visibility_token()
                            .and_then(|token| token.get_visibility())
                            .map(visibility_summary),
                    ),
                }),
                LuaDocTag::ReturnCast(return_cast_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::ReturnCast,
                    syntax_offset: return_cast_tag.get_position(),
                    data: SalsaDocTagDataSummary::NameTypeOffsets {
                        name: return_cast_tag
                            .get_name_token()
                            .map(|token| token.get_name_text().into()),
                        type_offsets: return_cast_tag
                            .get_op_types()
                            .filter_map(|op_type| op_type.get_type().map(type_offset))
                            .collect(),
                    },
                }),
                LuaDocTag::Language(language_tag) => tags.push(SalsaDocTagSummary {
                    owner,
                    kind: SalsaDocTagKindSummary::Language,
                    syntax_offset: language_tag.get_position(),
                    data: SalsaDocTagDataSummary::Name(
                        language_tag
                            .get_name_token()
                            .map(|token| token.get_name_text().into()),
                    ),
                }),
            }
        }
    }

    SalsaDocSummary {
        type_defs,
        type_tags,
        fields,
        generics,
        params,
        returns,
        operators,
        tags,
    }
}

fn type_def_visibility(flag: Option<LuaDocTypeFlag>) -> SalsaDocVisibilityKindSummary {
    let Some(flag) = flag else {
        return SalsaDocVisibilityKindSummary::Public;
    };

    for token in flag.get_attrib_tokens() {
        match token.get_name_text() {
            "private" => return SalsaDocVisibilityKindSummary::Private,
            "internal" => return SalsaDocVisibilityKindSummary::Internal,
            "public" => return SalsaDocVisibilityKindSummary::Public,
            _ => {}
        }
    }

    SalsaDocVisibilityKindSummary::Public
}

fn type_offset(doc_type: LuaDocType) -> SalsaDocTypeNodeKey {
    doc_type.into()
}

fn attribute_use_name(attribute_use: &LuaDocAttributeUse) -> Option<SmolStr> {
    attribute_use
        .get_type()
        .and_then(|typ| typ.get_name_text().map(Into::into))
}

fn field_key_summary(field_key: LuaDocFieldKey) -> SalsaDocTagFieldKeySummary {
    match field_key {
        LuaDocFieldKey::Name(name) => SalsaDocTagFieldKeySummary::Name(name.get_name_text().into()),
        LuaDocFieldKey::String(string) => {
            SalsaDocTagFieldKeySummary::String(string.get_value().into())
        }
        LuaDocFieldKey::Integer(num) => match num.get_number_value() {
            NumberResult::Int(i) => SalsaDocTagFieldKeySummary::Integer(i),
            _ => SalsaDocTagFieldKeySummary::String(num.get_text().into()),
        },
        LuaDocFieldKey::Type(doc_type) => SalsaDocTagFieldKeySummary::Type(type_offset(doc_type)),
    }
}

fn visibility_summary(visibility: VisibilityKind) -> SalsaDocVisibilityKindSummary {
    match visibility {
        VisibilityKind::Public => SalsaDocVisibilityKindSummary::Public,
        VisibilityKind::Protected => SalsaDocVisibilityKindSummary::Protected,
        VisibilityKind::Private => SalsaDocVisibilityKindSummary::Private,
        VisibilityKind::Internal => SalsaDocVisibilityKindSummary::Internal,
        VisibilityKind::Package => SalsaDocVisibilityKindSummary::Package,
    }
}

fn description_text<T>(tag: &T) -> Option<SmolStr>
where
    T: LuaDocDescriptionOwner,
{
    tag.get_description().and_then(|description| {
        let text = description.get_description_text();
        if text.is_empty() {
            None
        } else {
            Some(preprocess_description(&text).into())
        }
    })
}

fn preprocess_description(mut description: &str) -> String {
    if description.starts_with(['#', '@']) {
        description = description.trim_start_matches(['#', '@']);
    }

    let mut result = String::new();
    let lines = description.lines();
    let mut start_with_one_space = None;
    for mut line in lines {
        let indent_count = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent_count == line.len() {
            // empty line
            result.push('\n');
            continue;
        }

        if start_with_one_space.is_none() {
            start_with_one_space = Some(indent_count == 1);
        }

        if let Some(true) = start_with_one_space {
            let mut chars = line.chars();
            let first_char = chars.next();
            if let Some(c) = first_char
                && c.is_whitespace()
            {
                line = chars.as_str();
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    // trim end
    result.trim_end().to_string()
}

fn collect_generic_params(
    generic_decl_list: Option<LuaDocGenericDeclList>,
) -> Vec<SalsaDocGenericParamSummary> {
    generic_decl_list
        .into_iter()
        .flat_map(|generic_decl_list| generic_decl_list.get_generic_decl())
        .filter_map(|generic_decl| {
            Some(SalsaDocGenericParamSummary {
                name: generic_decl.get_name_token()?.get_name_text().into(),
                type_offset: generic_decl
                    .get_constraint_type()
                    .map(SalsaDocTypeNodeKey::from),
                default_type_offset: generic_decl
                    .get_default_type()
                    .map(SalsaDocTypeNodeKey::from),
            })
        })
        .collect()
}

fn build_owner_summary(owner: Option<LuaAst>) -> SalsaDocOwnerSummary {
    let Some(owner) = owner else {
        return SalsaDocOwnerSummary {
            kind: SalsaDocOwnerKindSummary::None,
            syntax_offset: None,
        };
    };

    let kind = match owner {
        LuaAst::LuaAssignStat(_) => SalsaDocOwnerKindSummary::AssignStat,
        LuaAst::LuaLocalStat(_) => SalsaDocOwnerKindSummary::LocalStat,
        LuaAst::LuaFuncStat(_) => SalsaDocOwnerKindSummary::FuncStat,
        LuaAst::LuaLocalFuncStat(_) => SalsaDocOwnerKindSummary::LocalFuncStat,
        LuaAst::LuaTableField(_) => SalsaDocOwnerKindSummary::TableField,
        LuaAst::LuaClosureExpr(_) => SalsaDocOwnerKindSummary::Closure,
        LuaAst::LuaCallExprStat(_) => SalsaDocOwnerKindSummary::CallExprStat,
        LuaAst::LuaReturnStat(_) => SalsaDocOwnerKindSummary::ReturnStat,
        _ => SalsaDocOwnerKindSummary::Other,
    };

    SalsaDocOwnerSummary {
        kind,
        syntax_offset: Some(owner.get_position()),
    }
}
