use super::*;
use crate::*;
use rowan::TextRange;

#[test]
fn test_summary_builder_doc_structures() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box<T>: Base
---@operator add(Box<T>): Box<T>
local Box = {}

---@alias Identifier string

---@enum Mode: integer
local Mode = { Fast = 1 }

---@alias Mapper fun(value: integer): boolean, nil
---@alias Mixed integer | string
---@alias Negative -integer

---@generic T: integer, U
---@param value T?
---@param ... U
---@return T result, U
---@return_overload true, T
---@overload fun(value: integer): boolean
---@nodiscard use the boolean result
---@async
---@deprecated prefer map2
---@version > 5.1, JIT
---@see Box#add
---@source https://example.com/doc
---@diagnostic disable-next-line: missing-return, unused
---@other custom payload
---@language lua
---@module "socket.core"
---@cast value string
---@return_cast value integer else string
---@readonly
---@private
---@namespace Demo
---@using Demo
local function map(value, ...)
  return value, ...
end

local result = map(1) ---@as string
---@meta socket.io"#;
    set_test_file(&mut compilation, 8, "C:/ws/doc_summary.lua", source);

    let doc_queries = compilation.doc();
    let doc = doc_queries.summary(FileId::new(8)).expect("doc summary");
    let doc_types = doc_queries.types(FileId::new(8)).expect("doc type summary");
    let signatures = doc_queries
        .signatures(FileId::new(8))
        .expect("signature summary");

    assert!(doc.type_defs.iter().any(|type_def| matches!(
        type_def,
        SalsaDocTypeDefSummary {
            name,
            kind: SalsaDocTypeDefKindSummary::Class,
            generic_params,
            super_type_offsets,
            ..
        } if name == "Box" && generic_params.len() == 1 && super_type_offsets.len() == 1
    )));
    assert!(doc.type_defs.iter().any(|type_def| matches!(
        type_def,
        SalsaDocTypeDefSummary {
            name,
            kind: SalsaDocTypeDefKindSummary::Alias,
            value_type_offset: Some(_),
            ..
        } if name == "Identifier"
    )));
    assert!(doc.type_defs.iter().any(|type_def| matches!(
        type_def,
        SalsaDocTypeDefSummary {
            name,
            kind: SalsaDocTypeDefKindSummary::Enum,
            value_type_offset: Some(_),
            ..
        } if name == "Mode"
    )));
    assert!(doc.operators.iter().any(|operator| matches!(
        operator,
        SalsaDocOperatorSummary {
            owner: SalsaDocOwnerSummary {
                kind: SalsaDocOwnerKindSummary::LocalStat,
                syntax_offset: Some(_),
            },
            name,
            param_type_offsets,
            return_type_offset: Some(_),
            ..
        } if name == "add" && param_type_offsets.len() == 1
    )));
    assert!(doc.generics.iter().any(|generic| matches!(
        generic,
        SalsaDocGenericSummary {
            owner: SalsaDocOwnerSummary {
                kind: SalsaDocOwnerKindSummary::LocalFuncStat,
                syntax_offset: Some(_),
            },
            params,
            ..
        } if params.len() == 2 && params[0].name == "T" && params[0].type_offset.is_some() && params[1].name == "U"
    )));
    assert!(doc.params.iter().any(|param| matches!(
        param,
        SalsaDocParamSummary {
            owner: SalsaDocOwnerSummary {
                kind: SalsaDocOwnerKindSummary::LocalFuncStat,
                syntax_offset: Some(_),
            },
            name,
            type_offset: Some(_),
            is_nullable: true,
            is_vararg: false,
            ..
        } if name == "value"
    )));
    assert!(doc.params.iter().any(|param| matches!(
        param,
        SalsaDocParamSummary {
            name,
            type_offset: Some(_),
            is_vararg: true,
            ..
        } if name == "..."
    )));
    assert!(doc.returns.iter().any(|return_info| matches!(
        return_info,
        SalsaDocReturnSummary {
            owner: SalsaDocOwnerSummary {
                kind: SalsaDocOwnerKindSummary::LocalFuncStat,
                syntax_offset: Some(_),
            },
            items,
            ..
        } if items.len() == 2 && items[0].name.as_deref() == Some("result")
    )));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::ReturnOverload && !tag.type_offsets().is_empty()
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Overload && tag.type_offsets().len() == 1
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Nodiscard
            && tag.content().map(|value| value.as_str()) == Some("use the boolean result")
    }));
    assert!(
        doc.tags
            .iter()
            .any(|tag| tag.kind == SalsaDocTagKindSummary::Async)
    );
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Deprecated
            && tag.content().map(|value| value.as_str()) == Some("prefer map2")
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Version && tag.version_conds().len() == 2
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::See
            && tag.content().map(|value| value.as_str()) == Some("Box#add")
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Source
            && tag.content().map(|value| value.as_str()) == Some("https://example.com/doc")
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Diagnostic
            && tag.name().map(|value| value.as_str()) == Some("disable-next-line")
            && tag.items().len() == 2
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Other
            && tag.name().map(|value| value.as_str()) == Some("other")
            && tag.content().map(|value| value.as_str()) == Some("custom payload")
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Language
            && tag.name().map(|value| value.as_str()) == Some("lua")
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Module
            && tag.name().map(|value| value.as_str()) == Some("socket.core")
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Cast
            && tag.name().map(|value| value.as_str()) == Some("value")
            && !tag.type_offsets().is_empty()
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::ReturnCast
            && tag.name().map(|value| value.as_str()) == Some("value")
            && tag.type_offsets().len() == 2
    }));
    assert!(
        doc.tags
            .iter()
            .any(|tag| tag.kind == SalsaDocTagKindSummary::Readonly)
    );
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Visibility
            && tag.visibility() == Some(&SalsaDocVisibilityKindSummary::Private)
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Namespace
            && tag.name().map(|value| value.as_str()) == Some("Demo")
    }));
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Using
            && tag.name().map(|value| value.as_str()) == Some("Demo")
    }));
    assert!(
        doc.tags
            .iter()
            .any(|tag| { tag.kind == SalsaDocTagKindSummary::As && tag.type_offsets().len() == 1 })
    );
    assert!(doc.tags.iter().any(|tag| {
        tag.kind == SalsaDocTagKindSummary::Meta
            && tag.name().map(|value| value.as_str()) == Some("socket.io")
    }));

    assert_eq!(doc_queries.tags(FileId::new(8)), Some(doc.tags.clone()));

    let source_tag = doc
        .tags
        .iter()
        .find(|tag| {
            tag.kind == SalsaDocTagKindSummary::Source
                && tag.content().map(|value| value.as_str()) == Some("https://example.com/doc")
                && matches!(
                    tag.owner,
                    SalsaDocOwnerSummary {
                        kind: SalsaDocOwnerKindSummary::LocalFuncStat,
                        syntax_offset: Some(_),
                    }
                )
        })
        .cloned()
        .expect("source tag");
    let map_owner = source_tag.owner.clone();
    let source_offset = source_tag.syntax_offset;

    assert_eq!(
        doc_queries.tag_at(FileId::new(8), source_offset),
        Some(source_tag.clone())
    );

    let source_tags = doc_queries
        .tags_for_kind(FileId::new(8), SalsaDocTagKindSummary::Source)
        .expect("source tags");
    assert_eq!(source_tags, vec![source_tag.clone()]);

    let owner_tags = doc_queries
        .tags_for_owner(FileId::new(8), map_owner.clone())
        .expect("owner tags");
    assert_eq!(
        owner_tags,
        doc.tags
            .iter()
            .filter(|tag| tag.owner == map_owner)
            .cloned()
            .collect::<Vec<_>>()
    );

    let properties = doc_queries
        .tag_properties(FileId::new(8))
        .expect("doc tag properties");
    let map_property = properties
        .iter()
        .find(|property| property.owner == map_owner)
        .cloned()
        .expect("map property");

    assert_eq!(
        doc_queries.tag_property(FileId::new(8), map_owner.clone()),
        Some(map_property.clone())
    );
    assert_eq!(
        map_property.visibility(),
        Some(&SalsaDocVisibilityKindSummary::Private)
    );
    assert_eq!(
        map_property.source().map(|value| value.as_str()),
        Some("https://example.com/doc")
    );
    assert_eq!(
        map_property.module().map(|value| value.as_str()),
        Some("socket.core")
    );
    assert_eq!(
        map_property.namespace().map(|value| value.as_str()),
        Some("Demo")
    );
    assert_eq!(
        map_property.language().map(|value| value.as_str()),
        Some("lua")
    );
    assert_eq!(
        map_property
            .deprecated_message()
            .flatten()
            .map(|value| value.as_str()),
        Some("prefer map2")
    );
    assert_eq!(
        map_property
            .nodiscard_message()
            .flatten()
            .map(|value| value.as_str()),
        Some("use the boolean result")
    );
    assert_eq!(
        map_property
            .using_names()
            .map(|name| name.as_str())
            .collect::<Vec<_>>(),
        vec!["Demo"]
    );
    assert_eq!(
        map_property
            .see()
            .map(|content| content.as_str())
            .collect::<Vec<_>>(),
        vec!["Box#add"]
    );
    assert!(map_property.is_deprecated());
    assert!(map_property.is_nodiscard());
    assert!(map_property.is_async());
    assert!(map_property.is_readonly());
    assert_eq!(map_property.version_conds().len(), 2);
    assert_eq!(map_property.diagnostics().count(), 1);
    let diagnostic_actions = map_property.diagnostics().next().expect("diagnostic entry");
    assert_eq!(diagnostic_actions.len(), 2);
    assert!(diagnostic_actions.iter().all(|action| matches!(
        action.kind,
        SalsaDocDiagnosticActionKindSummary::DisableNextLine
    )));
    assert_eq!(
        diagnostic_actions
            .iter()
            .filter_map(|action| action.code_name.as_deref())
            .collect::<Vec<_>>(),
        vec!["missing-return", "unused"]
    );

    let resolved_diagnostics = compilation
        .doc()
        .resolved_tag_diagnostics(FileId::new(8), map_property.owner.clone())
        .expect("resolved diagnostic actions");
    assert_eq!(resolved_diagnostics.len(), 2);
    assert!(resolved_diagnostics.iter().all(|diagnostic| matches!(
        diagnostic.kind,
        SalsaDocDiagnosticActionKindSummary::DisableNextLine
    )));
    assert!(
        resolved_diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.range.is_empty())
    );
    assert_eq!(
        resolved_diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![DiagnosticCode::MissingReturn, DiagnosticCode::Unused]
    );
    assert_eq!(
        map_property.custom_tags().cloned().collect::<Vec<_>>(),
        vec![SalsaDocTagNameContentSummary {
            name: "other".into(),
            content: Some("custom payload".into()),
        }]
    );

    let meta_property = properties
        .iter()
        .find(|property| property.meta().map(|value| value.as_str()) == Some("socket.io"))
        .cloned()
        .expect("meta property");
    assert_eq!(
        meta_property.meta().map(|value| value.as_str()),
        Some("socket.io")
    );

    assert!(size_of::<SalsaDocTagSummary>() <= 128);
    assert!(size_of::<SalsaDocTagPropertySummary>() <= 64);

    assert!(doc_types.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeNodeSummary {
            kind: SalsaDocTypeKindSummary::Nullable {
                inner_type_offset: Some(_)
            },
            ..
        }
    )));
    assert!(doc_types.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeNodeSummary {
            kind: SalsaDocTypeKindSummary::Function { params, returns, .. },
            ..
        } if !params.is_empty() && returns.len() == 2
    )));
    assert!(doc_types.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeNodeSummary {
            kind: SalsaDocTypeKindSummary::Binary {
                op: SalsaDocTypeBinaryOperatorSummary::Union,
                ..
            },
            ..
        }
    )));
    assert!(doc_types.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeNodeSummary {
            kind: SalsaDocTypeKindSummary::Unary {
                op: SalsaDocTypeUnaryOperatorSummary::Neg,
                ..
            },
            ..
        }
    )));

    let lowered_types = doc_queries
        .lowered_types(FileId::new(8))
        .expect("lowered doc type index");
    let resolved_types = doc_queries
        .resolved_types(FileId::new(8))
        .expect("resolved doc type index");

    assert_eq!(resolved_types.types.len(), doc_types.types.len());
    assert_eq!(resolved_types.types.len(), lowered_types.types.len());

    assert!(lowered_types.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Nullable {
                inner_type: SalsaDocTypeRef::Node(_),
            },
            ..
        }
    )));
    assert!(lowered_types.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Function {
                params,
                returns,
                ..
            },
            ..
        } if !params.is_empty()
            && returns.len() == 2
            && matches!(params[0].doc_type, SalsaDocTypeRef::Node(_))
    )));
    assert!(lowered_types.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Union { item_types },
            ..
        } if item_types.len() == 2
    )));
    assert!(lowered_types.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Unary {
                op: SalsaDocTypeUnaryOperatorSummary::Neg,
                inner_type: SalsaDocTypeRef::Node(_),
            },
            ..
        }
    )));

    let lowered_union_offset = doc_types
        .types
        .iter()
        .find_map(|doc_type| match &doc_type.kind {
            SalsaDocTypeKindSummary::Binary {
                op: SalsaDocTypeBinaryOperatorSummary::Union,
                ..
            } => Some(doc_type.syntax_offset),
            _ => None,
        })
        .expect("union syntax offset");
    assert!(matches!(
        doc_queries.lowered_type_at(FileId::new(8), lowered_union_offset),
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Union { item_types },
            ..
        }) if item_types.len() == 2
    ));
    assert!(matches!(
        doc_queries.resolved_type_at(FileId::new(8), lowered_union_offset),
        Some(SalsaDocTypeResolvedSummary {
            doc_type: SalsaDocTypeNodeSummary {
                kind: SalsaDocTypeKindSummary::Binary {
                    op: SalsaDocTypeBinaryOperatorSummary::Union,
                    ..
                },
                ..
            },
            lowered: SalsaDocTypeLoweredNode {
                kind: SalsaDocTypeLoweredKind::Union { item_types },
                ..
            },
            ..
        }) if item_types.len() == 2
    ));

    let lowered_union_key = doc_types
        .types
        .iter()
        .find_map(|doc_type| match &doc_type.kind {
            SalsaDocTypeKindSummary::Binary {
                op: SalsaDocTypeBinaryOperatorSummary::Union,
                ..
            } => Some(doc_type.node_key()),
            _ => None,
        })
        .expect("union type key");
    assert!(matches!(
        doc_queries.resolved_type_by_key(FileId::new(8), lowered_union_key),
        Some(SalsaDocTypeResolvedSummary {
            type_key,
            syntax_offset,
            lowered: SalsaDocTypeLoweredNode { syntax_offset: lowered_offset, .. },
            ..
        }) if type_key == lowered_union_key && syntax_offset == lowered_union_offset && lowered_offset == lowered_union_offset
    ));

    assert!(signatures.signatures.iter().any(|signature| matches!(
        signature,
        SalsaSignatureSummary {
            source: SalsaSignatureSourceSummary::LocalFuncStat,
            name: Some(name),
            params,
            doc_generic_offsets,
            doc_param_offsets,
            doc_return_offsets,
            ..
        } if name == "map"
            && params.len() == 2
            && doc_generic_offsets.len() == 1
            && doc_param_offsets.len() == 2
            && doc_return_offsets.len() == 1
    )));

    set_test_file(
        &mut compilation,
        9,
        "C:/ws/signature_call.lua",
        r#"local obj = {}
function obj:run(x, ...)
  return x
end
local fn = function(y) return y end
local value = obj:run(1, 2)
local wrapped = fn(3)"#,
    );

    let signature_summary = compilation
        .doc()
        .signatures(FileId::new(9))
        .expect("signature and call summary");

    assert!(
        signature_summary
            .signatures
            .iter()
            .any(|signature| matches!(
                signature,
                SalsaSignatureSummary {
                    source: SalsaSignatureSourceSummary::FuncStat,
                    name: Some(name),
                    is_method: true,
                    params,
                    return_expr_offsets,
                    ..
                } if name == "obj.run" && params.len() == 2 && !return_expr_offsets.is_empty()
            ))
    );
    assert!(
        signature_summary
            .signatures
            .iter()
            .any(|signature| matches!(
                signature,
                SalsaSignatureSummary {
                    source: SalsaSignatureSourceSummary::ClosureExpr,
                    name: None,
                    params,
                    ..
                } if params.len() == 1
            ))
    );
    assert!(signature_summary.calls.iter().any(|call| matches!(
        call,
        SalsaCallSummary {
            kind: SalsaCallKindSummary::Normal,
            is_colon_call: true,
            arg_expr_offsets,
            ..
        } if arg_expr_offsets.len() == 2
    )));

    let mut incomplete_compilation = setup_compilation();
    set_test_file(
        &mut incomplete_compilation,
        13,
        "C:/ws/incomplete_doc_type.lua",
        r#"---@alias Broken fun(x)"#,
    );

    let incomplete_lowered = incomplete_compilation
        .doc()
        .lowered_types(FileId::new(13))
        .expect("incomplete lowered doc type index");
    let incomplete_resolved = incomplete_compilation
        .doc()
        .resolved_types(FileId::new(13))
        .expect("incomplete resolved doc type index");
    assert!(incomplete_lowered.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Function { params, .. },
            ..
        } if params.len() == 1
            && matches!(params[0].doc_type, SalsaDocTypeRef::Incomplete)
    )));
    assert!(incomplete_resolved.types.iter().any(|doc_type| matches!(
        doc_type,
        SalsaDocTypeResolvedSummary {
            lowered: SalsaDocTypeLoweredNode {
                kind: SalsaDocTypeLoweredKind::Function { params, .. },
                ..
            },
            ..
        } if params.len() == 1
            && matches!(params[0].doc_type, SalsaDocTypeRef::Incomplete)
    )));
}

#[test]
fn test_summary_builder_ownerless_diagnostic_ranges() {
    let mut compilation = setup_compilation();
    let source = r#"---@diagnostic disable: syntax-error
---@diagnostic enable: syntax-error

---@param
local function f() end
"#;
    set_test_file(
        &mut compilation,
        10,
        "C:/ws/doc_ownerless_diagnostic.lua",
        source,
    );

    let properties = compilation
        .doc()
        .tag_properties(FileId::new(10))
        .expect("doc tag properties");
    let ownerless_property = properties
        .iter()
        .find(|property| property.owner.syntax_offset.is_none())
        .cloned()
        .expect("ownerless property");

    let resolved_diagnostics = compilation
        .doc()
        .resolved_tag_diagnostics(FileId::new(10), ownerless_property.owner.clone())
        .expect("resolved ownerless diagnostics");

    assert_eq!(resolved_diagnostics.len(), 2);
    assert!(resolved_diagnostics.iter().all(|diagnostic| {
        diagnostic.range == TextRange::new(TextSize::new(0), TextSize::new(source.len() as u32))
    }));
    assert!(resolved_diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.kind,
        SalsaDocDiagnosticActionKindSummary::Disable
    ) && diagnostic.code
        == Some(DiagnosticCode::SyntaxError)));
    assert!(resolved_diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.kind,
        SalsaDocDiagnosticActionKindSummary::Enable
    ) && diagnostic.code
        == Some(DiagnosticCode::SyntaxError)));
}
