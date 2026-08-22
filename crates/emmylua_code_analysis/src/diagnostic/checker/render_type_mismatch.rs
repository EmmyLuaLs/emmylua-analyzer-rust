use crate::{
    DbIndex, LuaArrayLen, LuaArrayType, LuaType, RenderLevel, TypeMismatch, TypeMismatchKind,
    TypePathSegment, humanize_type,
};

pub fn render_diagnostic_detail(
    db: &DbIndex,
    mismatch: &TypeMismatch,
    root_source: &LuaType,
    root_target: &LuaType,
) -> Option<String> {
    if mismatch.path().is_empty()
        && matches!(mismatch.reason(), TypeMismatchKind::Incompatible { source, target } if source == root_source && target == root_target)
    {
        return None;
    }
    Some(render_type_mismatch_reason(db, mismatch))
}

fn render_type_mismatch_reason(db: &DbIndex, mismatch: &TypeMismatch) -> String {
    let mut lines = Vec::new();
    let mut depth = 1;
    let path_rev: Vec<&TypePathSegment> = mismatch.path().iter().rev().collect();

    for (i, segment) in path_rev.iter().enumerate() {
        let remaining_segments = &path_rev[i + 1..];
        let line = match segment {
            TypePathSegment::Member(key) => Some(format!(
                "The types of property '{}' are incompatible.",
                key.to_path()
            )),
            TypePathSegment::Index(index) => Some(format!(
                "Index type '{}' is incompatible.",
                humanize_type(db, index, RenderLevel::Simple)
            )),
            TypePathSegment::TupleElement(index) => Some(format!(
                "Type at position {} in source is not compatible with type at position {} in target.",
                index + 1,
                index + 1
            )),
            TypePathSegment::ArrayElement => {
                if let TypeMismatchKind::Incompatible { source, target } = mismatch.reason()
                    && remaining_segments
                        .iter()
                        .all(|s| matches!(s, TypePathSegment::ArrayElement))
                {
                    (!remaining_segments.is_empty()).then(|| {
                        let count = remaining_segments.len();
                        let sub_source = wrap_array(source.clone(), count);
                        let sub_target = wrap_array(target.clone(), count);
                        render_relation(db, &sub_source, &sub_target)
                    })
                } else {
                    Some("Array element is incompatible.".to_string())
                }
            }
            TypePathSegment::FunctionParameter(index) => {
                Some(format!("Function parameter {} is incompatible.", index + 1))
            }
            TypePathSegment::FunctionReturn(index) => {
                Some(format!("Function return {} is incompatible.", index + 1))
            }
            TypePathSegment::GenericArgument(index) => {
                Some(format!("Generic argument {} is incompatible.", index + 1))
            }
        };
        if let Some(line) = line {
            lines.push(format!("{}{}", "  ".repeat(depth), line));
            depth += 1;
        }
    }

    let reason = match mismatch.reason() {
        TypeMismatchKind::Incompatible { source, target } => render_relation(db, source, target),
        TypeMismatchKind::Message(message) => message.clone(),
        TypeMismatchKind::MissingMember { key } => {
            format!("Property '{}' is missing.", key.to_path())
        }
        TypeMismatchKind::MissingTupleElement { index } => {
            format!("Tuple element {} is missing.", index + 1)
        }
    };
    lines.push(format!("{}{}", "  ".repeat(depth), reason));

    lines.join("\n")
}

fn wrap_array(mut typ: LuaType, count: usize) -> LuaType {
    for _ in 0..count {
        typ = LuaType::Array(LuaArrayType::new(typ, LuaArrayLen::None).into());
    }
    typ
}

fn render_relation(db: &DbIndex, source: &LuaType, target: &LuaType) -> String {
    format!(
        "Type '{}' is not assignable to type '{}'.",
        humanize_type(db, source, RenderLevel::Simple),
        humanize_type(db, target, RenderLevel::Simple)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssignabilityResult, LuaMemberKey, VirtualWorkspace, check_assignable};
    use smol_str::SmolStr;

    #[test]
    fn test_render_nested_property_mismatch_ts_style() {
        let db = DbIndex::new();
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Number)
            .at(TypePathSegment::Member(LuaMemberKey::Name(SmolStr::new(
                "b",
            ))))
            .at(TypePathSegment::Member(LuaMemberKey::Name(SmolStr::new(
                "a",
            ))));

        let rendered = render_type_mismatch_reason(&db, &mismatch);
        assert_eq!(
            rendered,
            "  The types of property 'a' are incompatible.\n    The types of property 'b' are incompatible.\n      Type 'string' is not assignable to type 'number'."
        );
    }

    #[test]
    fn test_render_nested_array_elements_ts_style() {
        let db = DbIndex::new();
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Integer)
            .at(TypePathSegment::ArrayElement)
            .at(TypePathSegment::ArrayElement)
            .at(TypePathSegment::Member(LuaMemberKey::Name(SmolStr::new(
                "data",
            ))));

        let rendered = render_type_mismatch_reason(&db, &mismatch);
        assert_eq!(
            rendered,
            "  The types of property 'data' are incompatible.\n    Type 'string[]' is not assignable to type 'integer[]'.\n      Type 'string' is not assignable to type 'integer'."
        );
    }

    #[test]
    fn test_render_3_level_array_elements_ts_style() {
        let db = DbIndex::new();
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Number)
            .at(TypePathSegment::ArrayElement)
            .at(TypePathSegment::ArrayElement)
            .at(TypePathSegment::ArrayElement);

        let rendered = render_type_mismatch_reason(&db, &mismatch);
        assert_eq!(
            rendered,
            "  Type 'string[][]' is not assignable to type 'number[][]'.\n    Type 'string[]' is not assignable to type 'number[]'.\n      Type 'string' is not assignable to type 'number'."
        );
    }

    #[test]
    fn test_render_single_array_element_ts_style() {
        let db = DbIndex::new();
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Integer)
            .at(TypePathSegment::ArrayElement);

        let rendered = render_type_mismatch_reason(&db, &mismatch);
        assert_eq!(
            rendered,
            "  Type 'string' is not assignable to type 'integer'."
        );
    }

    #[test]
    fn test_render_tuple_element_ts_style() {
        let db = DbIndex::new();
        let mismatch = TypeMismatch::incompatible(&LuaType::Boolean, &LuaType::String)
            .at(TypePathSegment::TupleElement(1));

        let rendered = render_type_mismatch_reason(&db, &mismatch);
        assert_eq!(
            rendered,
            "  Type at position 2 in source is not compatible with type at position 2 in target.\n    Type 'boolean' is not assignable to type 'string'."
        );
    }

    #[test]
    fn test_render_array_to_object_alias() {
        let mut ws: VirtualWorkspace = VirtualWorkspace::new();
        ws.def("---@alias Item { foo: number }");

        let a = ws.ty("Item");
        let b = ws.ty("string[][]");
        let mismatch = check_assignable(ws.get_db_mut(), &b, &a);

        let AssignabilityResult::NotAssignable(m) = mismatch else {
            panic!("expected not assignable");
        };
        assert!(m.path().is_empty());

        assert_eq!(
            render_diagnostic_detail(ws.get_db_mut(), &m, &b, &a),
            Some("  Type 'string[][]' is not assignable to type '{ foo: number }'.".to_string())
        );
    }

    #[test]
    fn test_render_nested_array_to_object_alias() {
        let mut ws: VirtualWorkspace = VirtualWorkspace::new();
        ws.def("---@alias Item { foo: number }");

        let a = ws.ty("Item[]");
        let b = ws.ty("string[][][]");
        let mismatch = check_assignable(ws.get_db_mut(), &b, &a);

        let AssignabilityResult::NotAssignable(m) = mismatch else {
            panic!("expected not assignable");
        };
        assert_eq!(m.path(), &[TypePathSegment::ArrayElement]);
        assert_eq!(
            render_diagnostic_detail(ws.get_db_mut(), &m, &b, &a),
            Some("  Type 'string[][]' is not assignable to type 'Item?'.".to_string())
        );
    }
}
