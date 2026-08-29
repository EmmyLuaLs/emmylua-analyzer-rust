use std::fmt::Write;

use crate::{
    DbIndex, LuaMemberKey, LuaType, RenderLevel, TypeMismatch, TypeMismatchKind, TypePathInfo,
    TypePathSegment, humanize_type,
};

use super::humanize_lint_type;

pub fn render_diagnostic_detail(
    db: &DbIndex,
    mismatch: &TypeMismatch,
    root_source: &LuaType,
    root_target: &LuaType,
) -> Option<String> {
    render_type_mismatch_reason(db, mismatch, root_source, root_target)
}

fn render_type_mismatch_reason<'a>(
    db: &DbIndex,
    mismatch: &'a TypeMismatch,
    root_source: &'a LuaType,
    root_target: &'a LuaType,
) -> Option<String> {
    let mut output = String::new();
    let mut depth = 1;
    let mut last_relation = Some((root_source, root_target));

    for step in mismatch.path().rev() {
        if render_path_title(&mut output, &mut depth, db, step.segment()) {
            last_relation = None;
        }
        for info in step.info() {
            match info {
                TypePathInfo::Relation { source, target } => push_relation(
                    &mut output,
                    &mut depth,
                    db,
                    source,
                    target,
                    &mut last_relation,
                ),
            }
        }
    }

    match mismatch.reason() {
        TypeMismatchKind::Incompatible { source, target } => push_relation(
            &mut output,
            &mut depth,
            db,
            source,
            target,
            &mut last_relation,
        ),
        TypeMismatchKind::Message(message) => push_text_line(&mut output, &mut depth, message),
        TypeMismatchKind::MissingMembers { keys } => {
            let (source, target) = last_relation.unwrap_or((root_source, root_target));
            if let Some(text) = format_missing_fields(db, source, target, keys) {
                push_text_line(&mut output, &mut depth, &text);
            }
        }
        TypeMismatchKind::MissingTupleElement { index } => {
            start_line(&mut output, depth);
            let _ = write!(output, "Tuple element {} is missing.", index + 1);
        }
    }

    (!output.is_empty()).then_some(output)
}

pub fn format_missing_fields(
    db: &DbIndex,
    source: &LuaType,
    target: &LuaType,
    keys: &[LuaMemberKey],
) -> Option<String> {
    let mut names = keys
        .iter()
        .filter_map(member_key_to_field_name)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    let first = names.first()?;

    if names.len() == 1 {
        return Some(
            t!(
                "Type `%{source}` is missing the `%{field}` field from type `%{target}`.",
                source = humanize_lint_type(db, source),
                field = first.clone(),
                target = humanize_lint_type(db, target),
            )
            .to_string(),
        );
    }

    const MAX_DISPLAY_FIELDS: usize = 4;

    let source = humanize_lint_type(db, source);
    let target = humanize_lint_type(db, target);
    if names.len() <= MAX_DISPLAY_FIELDS {
        return Some(
            t!(
                "Type `%{source}` is missing the following fields from type `%{target}`: %{fields}",
                source = source,
                target = target,
                fields = names.join(", "),
            )
            .to_string(),
        );
    }

    let fields = names[..MAX_DISPLAY_FIELDS].join(", ");
    Some(
        t!(
            "Type `%{source}` is missing the following fields from type `%{target}`: %{fields}, and %{count} more.",
            source = source,
            target = target,
            fields = fields,
            count = names.len() - MAX_DISPLAY_FIELDS,
        )
        .to_string(),
    )
}

fn member_key_to_field_name(key: &LuaMemberKey) -> Option<String> {
    match key {
        LuaMemberKey::Name(name) => Some(name.to_string()),
        LuaMemberKey::Integer(index) => Some(format!("[{}]", index)),
        LuaMemberKey::None | LuaMemberKey::TypeKey(_) => None,
    }
}

fn render_path_title(
    output: &mut String,
    depth: &mut usize,
    db: &DbIndex,
    segment: &TypePathSegment,
) -> bool {
    match segment {
        TypePathSegment::Member(key) => {
            start_line(output, *depth);
            let _ = write!(
                output,
                "The types of property '{}' are incompatible.",
                key.to_path()
            );
        }
        TypePathSegment::Index(index) => {
            start_line(output, *depth);
            let _ = write!(
                output,
                "Index type '{}' is incompatible.",
                humanize_type(db, index, RenderLevel::Simple)
            );
        }
        TypePathSegment::TupleElement(index) => {
            start_line(output, *depth);
            let _ = write!(
                output,
                "Type at position {} in source is not compatible with type at position {} in target.",
                index + 1,
                index + 1
            );
        }
        TypePathSegment::ArrayElement => return false,
        TypePathSegment::FunctionParameter(index) => {
            start_line(output, *depth);
            let _ = write!(output, "Function parameter {} is incompatible.", index + 1);
        }
        TypePathSegment::FunctionReturn(index) => {
            start_line(output, *depth);
            let _ = write!(output, "Function return {} is incompatible.", index + 1);
        }
        TypePathSegment::GenericArgument(index) => {
            start_line(output, *depth);
            let _ = write!(output, "Generic argument {} is incompatible.", index + 1);
        }
    }
    *depth += 1;
    true
}

fn push_relation<'a>(
    output: &mut String,
    depth: &mut usize,
    db: &DbIndex,
    source: &'a LuaType,
    target: &'a LuaType,
    last_relation: &mut Option<(&'a LuaType, &'a LuaType)>,
) {
    if last_relation
        .is_some_and(|(last_source, last_target)| last_source == source && last_target == target)
    {
        return;
    }

    start_line(output, *depth);
    let _ = write!(
        output,
        "Type '{}' is not assignable to type '{}'.",
        humanize_type(db, source, RenderLevel::Simple),
        humanize_type(db, target, RenderLevel::Simple)
    );
    *depth += 1;
    *last_relation = Some((source, target));
}

fn push_text_line(output: &mut String, depth: &mut usize, text: &str) {
    start_line(output, *depth);
    output.push_str(text);
    *depth += 1;
}

fn start_line(output: &mut String, depth: usize) {
    if !output.is_empty() {
        output.push('\n');
    }
    for _ in 0..depth {
        output.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AssignabilityResult, LuaArrayLen, LuaArrayType, LuaMemberKey, TypePathInfo,
        VirtualWorkspace, check_assignable,
    };
    use smol_str::SmolStr;

    fn wrap_array(mut typ: LuaType, count: usize) -> LuaType {
        for _ in 0..count {
            typ = LuaType::Array(LuaArrayType::new(typ, LuaArrayLen::None).into());
        }
        typ
    }

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

        assert_eq!(
            render_type_mismatch_reason(&db, &mismatch, &LuaType::String, &LuaType::Number),
            Some(
                "  The types of property 'a' are incompatible.\n    The types of property 'b' are incompatible.\n      Type 'string' is not assignable to type 'number'."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_render_nested_array_elements_ts_style() {
        let db = DbIndex::new();
        let source_array = wrap_array(LuaType::String, 1);
        let target_array = wrap_array(LuaType::Integer, 1);
        let source_nested_array = wrap_array(LuaType::String, 2);
        let target_nested_array = wrap_array(LuaType::Integer, 2);
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Integer)
            .at_with_info(
                TypePathSegment::ArrayElement,
                [TypePathInfo::relation(&source_array, &target_array)],
            )
            .at_with_info(
                TypePathSegment::ArrayElement,
                [
                    TypePathInfo::relation(&source_nested_array, &target_nested_array),
                    TypePathInfo::relation(&source_array, &target_array),
                ],
            )
            .at(TypePathSegment::Member(LuaMemberKey::Name(SmolStr::new(
                "data",
            ))));

        assert_eq!(
            render_type_mismatch_reason(
                &db,
                &mismatch,
                &source_nested_array,
                &target_nested_array,
            ),
            Some(
                "  The types of property 'data' are incompatible.\n    Type 'string[][]' is not assignable to type 'integer[][]'.\n      Type 'string[]' is not assignable to type 'integer[]'.\n        Type 'string' is not assignable to type 'integer'."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_render_3_level_array_elements_ts_style() {
        let db = DbIndex::new();
        let source_array = wrap_array(LuaType::String, 1);
        let target_array = wrap_array(LuaType::Number, 1);
        let source_nested_array = wrap_array(LuaType::String, 2);
        let target_nested_array = wrap_array(LuaType::Number, 2);
        let source_root = wrap_array(LuaType::String, 3);
        let target_root = wrap_array(LuaType::Number, 3);
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Number)
            .at_with_info(
                TypePathSegment::ArrayElement,
                [TypePathInfo::relation(&source_array, &target_array)],
            )
            .at_with_info(
                TypePathSegment::ArrayElement,
                [
                    TypePathInfo::relation(&source_nested_array, &target_nested_array),
                    TypePathInfo::relation(&source_array, &target_array),
                ],
            )
            .at_with_info(
                TypePathSegment::ArrayElement,
                [
                    TypePathInfo::relation(&source_root, &target_root),
                    TypePathInfo::relation(&source_nested_array, &target_nested_array),
                ],
            );

        assert_eq!(
            render_type_mismatch_reason(&db, &mismatch, &source_root, &target_root),
            Some(
                "  Type 'string[][]' is not assignable to type 'number[][]'.\n    Type 'string[]' is not assignable to type 'number[]'.\n      Type 'string' is not assignable to type 'number'."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_render_single_array_element_ts_style() {
        let db = DbIndex::new();
        let source = wrap_array(LuaType::String, 1);
        let target = wrap_array(LuaType::Integer, 1);
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Integer)
            .at_with_info(
                TypePathSegment::ArrayElement,
                [TypePathInfo::relation(&source, &target)],
            );

        assert_eq!(
            render_type_mismatch_reason(&db, &mismatch, &source, &target),
            Some("  Type 'string' is not assignable to type 'integer'.".to_string())
        );
    }

    #[test]
    fn test_render_array_member_mismatch_without_generic_array_message() {
        let db = DbIndex::new();
        let source = wrap_array(LuaType::String, 1);
        let target = wrap_array(LuaType::Integer, 1);
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Integer)
            .at(TypePathSegment::Member(LuaMemberKey::Name(SmolStr::new(
                "value",
            ))))
            .at_with_info(
                TypePathSegment::ArrayElement,
                [TypePathInfo::relation(&source, &target)],
            );

        assert_eq!(
            render_type_mismatch_reason(&db, &mismatch, &source, &target),
            Some(
                "  The types of property 'value' are incompatible.\n    Type 'string' is not assignable to type 'integer'."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_render_tuple_element_ts_style() {
        let db = DbIndex::new();
        let mismatch = TypeMismatch::incompatible(&LuaType::Boolean, &LuaType::String)
            .at(TypePathSegment::TupleElement(1));

        assert_eq!(
            render_type_mismatch_reason(&db, &mismatch, &LuaType::Boolean, &LuaType::String),
            Some(
                "  Type at position 2 in source is not compatible with type at position 2 in target.\n    Type 'boolean' is not assignable to type 'string'."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_render_same_family_generic_alias_argument_mismatch() {
        let mut ws: VirtualWorkspace = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Box<T> { value: T }
            ---@alias DeepBox<T> Box<Box<Box<T>>>
            "#,
        );

        let a = ws.ty("DeepBox<number>");
        let b = ws.ty("DeepBox<string>");
        let mismatch = check_assignable(ws.get_db_mut(), &b, &a);

        let AssignabilityResult::NotAssignable(m) = mismatch else {
            panic!("expected not assignable");
        };
        assert!(!m.has_path());
        assert_eq!(
            render_diagnostic_detail(ws.get_db_mut(), &m, &b, &a),
            Some("  Type 'string' is not assignable to type 'number'.".to_string())
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
        assert!(!m.has_path());

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
        assert!(
            m.path()
                .map(|step| step.segment())
                .eq([&TypePathSegment::ArrayElement])
        );
        assert_eq!(
            render_diagnostic_detail(ws.get_db_mut(), &m, &b, &a),
            Some("  Type 'string[][]' is not assignable to type 'Item?'.".to_string())
        );
    }
}
