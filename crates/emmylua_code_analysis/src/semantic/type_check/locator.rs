use super::mismatch::{TypeMismatch, TypePathSegment};
use crate::{LuaMemberKey, LuaType};
use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexKey, LuaTableExpr};
use rowan::TextRange;

pub fn locate_mismatch_range(source_expr: &LuaExpr, mismatch: &TypeMismatch) -> TextRange {
    let mut current = Some(source_expr.clone());
    let mut range = source_expr.syntax().text_range();

    for step in mismatch.steps().iter().rev() {
        let Some(segment) = step.segment.as_ref() else {
            continue;
        };
        if matches!(
            segment,
            TypePathSegment::SourceUnionMember(_)
                | TypePathSegment::TargetUnionCandidate(_)
                | TypePathSegment::IntersectionMember(_)
                | TypePathSegment::GenericArgument(_)
                | TypePathSegment::FunctionParameter(_)
                | TypePathSegment::FunctionReturn(_)
        ) {
            continue;
        }

        let Some(expr) = current.as_ref() else {
            break;
        };
        let Some(table) = LuaTableExpr::cast(expr.syntax().clone()) else {
            break;
        };
        let key = match segment {
            TypePathSegment::Member(key) => Some(key.clone()),
            TypePathSegment::Index(index) => match index {
                LuaType::StringConst(value) | LuaType::DocStringConst(value) => {
                    Some(LuaMemberKey::Name((**value).clone()))
                }
                LuaType::IntegerConst(value) | LuaType::DocIntegerConst(value) => {
                    Some(LuaMemberKey::Integer(*value))
                }
                _ => break,
            },
            TypePathSegment::TupleElement(index) => Some(LuaMemberKey::Integer(*index as i64 + 1)),
            TypePathSegment::ArrayElement => None,
            _ => continue,
        };
        let found = table.get_fields_with_keys().find(|(_, field_key)| {
            if let Some(expected) = &key {
                key_matches(field_key, expected)
            } else {
                matches!(field_key, LuaIndexKey::Idx(_))
            }
        });
        let Some((field, _)) = found else {
            break;
        };

        current = field.get_value_expr();
        range = current
            .as_ref()
            .map(|value| value.syntax().text_range())
            .unwrap_or_else(|| field.get_range());
    }

    range
}

fn key_matches(key: &LuaIndexKey, expected: &LuaMemberKey) -> bool {
    match (key, expected) {
        (LuaIndexKey::Name(actual), LuaMemberKey::Name(expected)) => {
            actual.get_name_text() == expected.as_str()
        }
        (LuaIndexKey::String(actual), LuaMemberKey::Name(expected)) => {
            actual.get_value() == expected.as_str()
        }
        (LuaIndexKey::Integer(actual), LuaMemberKey::Integer(expected)) => {
            actual.get_number_value().as_integer() == Some(*expected)
        }
        (LuaIndexKey::Idx(actual), LuaMemberKey::Integer(expected)) => *actual as i64 == *expected,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VirtualWorkspace;
    use emmylua_parser::{LuaAstNode, LuaLocalStat, LuaTableExpr};
    use smol_str::SmolStr;

    #[test]
    fn literal_index_frames_follow_table_fields() {
        let mut workspace = VirtualWorkspace::new();
        let file_id = workspace.def("local value = { foo = 'oops' }");
        let stat = workspace.get_node::<LuaLocalStat>(file_id);
        let source_expr = stat.get_value_exprs().next().expect("initializer");
        let table = LuaTableExpr::cast(source_expr.syntax().clone()).expect("table expression");
        let (field, _) = table.get_fields_with_keys().next().expect("field");
        let expected_range = field
            .get_value_expr()
            .expect("field value")
            .syntax()
            .text_range();
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Integer).at(
            TypePathSegment::Index(LuaType::StringConst(SmolStr::new("foo").into())),
            &LuaType::Table,
            &LuaType::Table,
        );

        assert_eq!(
            locate_mismatch_range(&source_expr, &mismatch),
            expected_range
        );
    }

    #[test]
    fn integer_index_frames_follow_table_fields() {
        let mut workspace = VirtualWorkspace::new();
        let file_id = workspace.def("local value = { [1] = 'oops' }");
        let stat = workspace.get_node::<LuaLocalStat>(file_id);
        let source_expr = stat.get_value_exprs().next().expect("initializer");
        let table = LuaTableExpr::cast(source_expr.syntax().clone()).expect("table expression");
        let (field, _) = table.get_fields_with_keys().next().expect("field");
        let expected_range = field
            .get_value_expr()
            .expect("field value")
            .syntax()
            .text_range();
        let mismatch = TypeMismatch::incompatible(&LuaType::String, &LuaType::Integer).at(
            TypePathSegment::Index(LuaType::IntegerConst(1)),
            &LuaType::Table,
            &LuaType::Table,
        );

        assert_eq!(
            locate_mismatch_range(&source_expr, &mismatch),
            expected_range
        );
    }

    #[test]
    fn broad_index_frames_use_source_expr_fallback() {
        let mut workspace = VirtualWorkspace::new();
        let file_id = workspace.def("local value = { foo = 'oops' }");
        let stat = workspace.get_node::<LuaLocalStat>(file_id);
        let source_expr = stat.get_value_exprs().next().expect("initializer");
        let broad_index = TypeMismatch::incompatible(&LuaType::String, &LuaType::Integer).at(
            TypePathSegment::Index(LuaType::String),
            &LuaType::Table,
            &LuaType::Table,
        );
        assert_eq!(
            locate_mismatch_range(&source_expr, &broad_index),
            source_expr.syntax().text_range()
        );
    }
}
