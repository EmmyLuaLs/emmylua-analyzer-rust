use super::mismatch::TypePathSegment;
use crate::{LuaMemberKey, LuaType};
use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexKey, LuaTableExpr};
use rowan::TextRange;

pub fn locate_mismatch_range(source_expr: &LuaExpr, frames: &[TypePathSegment]) -> TextRange {
    let mut current = Some(source_expr.clone());
    let mut range = source_expr.syntax().text_range();
    for frame in frames.iter().rev() {
        if matches!(
            frame,
            TypePathSegment::SourceUnionMember(_)
                | TypePathSegment::TargetUnionCandidate(_)
                | TypePathSegment::IntersectionMember(_)
                | TypePathSegment::GenericArgument(_)
        ) {
            continue;
        }
        let Some(expr) = current.as_ref() else {
            break;
        };
        let Some(table) = LuaTableExpr::cast(expr.syntax().clone()) else {
            break;
        };
        let key = match frame {
            TypePathSegment::Member(k) => Some(k.clone()),
            TypePathSegment::Index(index) => match index {
                LuaType::StringConst(value) | LuaType::DocStringConst(value) => {
                    Some(LuaMemberKey::Name((**value).clone()))
                }
                LuaType::IntegerConst(value) | LuaType::DocIntegerConst(value) => {
                    Some(LuaMemberKey::Integer(*value))
                }
                _ => break,
            },
            TypePathSegment::TupleElement(i) => Some(LuaMemberKey::Integer(*i as i64 + 1)),
            TypePathSegment::ArrayElement => None,
            _ => break,
        };
        let found = table.get_fields_with_keys().find(|(field, k)| {
            if let Some(expected) = &key {
                key_matches(k, expected)
            } else {
                matches!(k, LuaIndexKey::Idx(_)) && field.get_value_expr().is_some()
            }
        });
        let Some((field, _)) = found else {
            break;
        };
        current = field.get_value_expr();
        range = current
            .as_ref()
            .map(|v| v.syntax().text_range())
            .unwrap_or_else(|| field.get_range());
    }
    range
}

fn key_matches(key: &LuaIndexKey, expected: &LuaMemberKey) -> bool {
    match (key, expected) {
        (LuaIndexKey::Name(a), LuaMemberKey::Name(b)) => a.get_name_text() == b.as_str(),
        (LuaIndexKey::String(a), LuaMemberKey::Name(b)) => a.get_value() == b.as_str(),
        (LuaIndexKey::Integer(a), LuaMemberKey::Integer(b)) => {
            a.get_number_value().as_integer() == Some(*b)
        }
        (LuaIndexKey::Idx(a), LuaMemberKey::Integer(b)) => *a as i64 == *b,
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
        let mismatch_range = locate_mismatch_range(
            &source_expr,
            &[TypePathSegment::Index(LuaType::StringConst(
                SmolStr::new("foo").into(),
            ))],
        );
        assert_eq!(mismatch_range, expected_range);
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
        let mismatch_range = locate_mismatch_range(
            &source_expr,
            &[TypePathSegment::Index(LuaType::IntegerConst(1))],
        );
        assert_eq!(mismatch_range, expected_range);
    }

    #[test]
    fn broad_index_frames_use_source_expr_fallback() {
        let mut workspace = VirtualWorkspace::new();
        let file_id = workspace.def("local value = { foo = 'oops' }");
        let stat = workspace.get_node::<LuaLocalStat>(file_id);
        let source_expr = stat.get_value_exprs().next().expect("initializer");
        let mismatch_range =
            locate_mismatch_range(&source_expr, &[TypePathSegment::Index(LuaType::String)]);
        assert_eq!(mismatch_range, source_expr.syntax().text_range());
    }
}
