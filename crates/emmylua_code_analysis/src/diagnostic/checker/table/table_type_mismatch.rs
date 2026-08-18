use emmylua_parser::{LuaAstNode, LuaTableExpr};
use rowan::TextRange;

use crate::{
    AssignabilityResult, DiagnosticCode, LuaMemberKey, LuaType, LuaUnionType, SemanticModel,
    VariadicType, get_real_type, render_type_mismatch,
};

use super::{
    super::{DiagnosticContext, humanize_lint_type},
    TableAssignmentOutcome,
};

struct TableCheckState {
    remaining_fields: usize,
    budget_exhausted: bool,
}

impl TableCheckState {
    const MAX_FIELD_CHECK_COUNT: usize = 500;

    fn new() -> Self {
        Self {
            remaining_fields: TableCheckState::MAX_FIELD_CHECK_COUNT,
            budget_exhausted: false,
        }
    }

    fn enter_field(&mut self) -> bool {
        if self.check_field_budget() {
            return false;
        }
        self.remaining_fields -= 1;
        true
    }

    fn check_field_budget(&mut self) -> bool {
        if self.remaining_fields == 0 {
            self.budget_exhausted = true;
        }
        self.budget_exhausted
    }
}

pub(super) fn check_table_type_mismatch(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    table_expr: &LuaTableExpr,
    actual_type: &LuaType,
    expected_type: &LuaType,
) -> TableAssignmentOutcome {
    // 整表兼容时不访问任何字段 AST. 无法完成的类型关系同样按保守兼容处理.
    if semantic_model.is_assignable(actual_type, expected_type) {
        return TableAssignmentOutcome::Assignable;
    }

    let mut state = TableCheckState::new();
    if check_table_fields(
        context,
        semantic_model,
        expected_type,
        table_expr,
        &mut state,
    ) {
        return TableAssignmentOutcome::Reported;
    }

    if context.has_diagnostic_codes_in_range(
        table_expr.get_range(),
        &[DiagnosticCode::AssignTypeMismatch],
    ) {
        TableAssignmentOutcome::Reported
    } else {
        TableAssignmentOutcome::NoDiagnostic
    }
}

fn check_table_fields(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    expected_type: &LuaType,
    table_expr: &LuaTableExpr,
    state: &mut TableCheckState,
) -> bool {
    let mut has_diagnostic = false;
    let mut fields = table_expr.get_fields_with_keys().peekable();

    while let Some((field, field_key)) = fields.next() {
        if !state.enter_field() {
            break;
        }

        let Some(value_expr) = field.get_value_expr() else {
            continue;
        };
        let actual_type = semantic_model
            .infer_expr(value_expr.clone())
            .unwrap_or(LuaType::Any);

        let Some(member_key) = semantic_model.get_member_key(&field_key) else {
            continue;
        };

        let Ok(field_expected_type) = semantic_model.infer_member_type(expected_type, &member_key)
        else {
            continue;
        };

        // 最后的顺序字段可以展开函数调用的多返回值.
        if field.is_value_field()
            && fields.peek().is_none()
            && let LuaMemberKey::Integer(start_index) = &member_key
            && let LuaType::Variadic(variadic) = &actual_type
        {
            has_diagnostic |= check_table_last_variadic_type(
                context,
                semantic_model,
                expected_type,
                *start_index,
                variadic,
                field.get_range(),
            );
            continue;
        }

        if semantic_model.is_assignable(&actual_type, &field_expected_type) {
            continue;
        }

        // 此时不匹配, 如果右侧仍然是表字面量, 则需要递归检查其字段.
        let real_expected_type = get_real_type(semantic_model.get_db(), &field_expected_type)
            .unwrap_or(&field_expected_type);

        // 期望类型可能是可空 union(如 `Foo?`), 此时若真实值是表字面量仍需递归检查字段.
        let nested_expected_type = match real_expected_type {
            LuaType::Union(union) => match union.as_ref() {
                LuaUnionType::Nullable(inner) if inner.is_table() || inner.is_custom_type() => {
                    Some(inner)
                }
                _ => None,
            },
            _ if real_expected_type.is_table() || real_expected_type.is_custom_type() => {
                Some(&field_expected_type)
            }
            _ => None,
        };

        if let Some(nested_expected_type) = nested_expected_type
            && let Some(child_table) = LuaTableExpr::cast(value_expr.syntax().clone())
        {
            let field_has_diagnostic = if state.check_field_budget() {
                add_table_type_mismatch(
                    context,
                    semantic_model,
                    field.get_range(),
                    &field_expected_type,
                    &actual_type,
                )
            } else {
                let child_has_diagnostic = check_table_fields(
                    context,
                    semantic_model,
                    nested_expected_type,
                    &child_table,
                    state,
                );
                child_has_diagnostic
                    || state.check_field_budget()
                        && add_table_type_mismatch(
                            context,
                            semantic_model,
                            field.get_range(),
                            &field_expected_type,
                            &actual_type,
                        )
            };
            has_diagnostic |= field_has_diagnostic;
            continue;
        }

        state.check_field_budget();

        has_diagnostic |= add_table_type_mismatch(
            context,
            semantic_model,
            field.get_range(),
            &field_expected_type,
            &actual_type,
        );
    }

    has_diagnostic
}

fn add_table_type_mismatch(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    expected_type: &LuaType,
    actual_type: &LuaType,
) -> bool {
    let AssignabilityResult::NotAssignable(mismatch) =
        semantic_model.check_assignable(actual_type, expected_type)
    else {
        return false;
    };
    let db = semantic_model.get_db();
    context.add_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        range,
        t!(
            "Cannot assign `%{value}` to `%{source}`. %{reason}",
            value = humanize_lint_type(db, actual_type),
            source = humanize_lint_type(db, expected_type),
            reason = render_type_mismatch(db, &mismatch)
        )
        .to_string(),
        None,
    )
}

fn check_table_last_variadic_type(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    expected_type: &LuaType,
    start_index: i64,
    actual_variadic: &VariadicType,
    range: TextRange,
) -> bool {
    let db = semantic_model.get_db();
    for offset in 0..16 {
        let index = start_index + offset as i64;
        let member_key = LuaMemberKey::Integer(index);
        let Ok(field_expected_type) = semantic_model.infer_member_type(expected_type, &member_key)
        else {
            break;
        };

        let actual_type = match &field_expected_type {
            LuaType::Variadic(_) => {
                LuaType::Variadic(actual_variadic.get_new_variadic_from(offset).into())
            }
            _ => {
                let Some(actual_type) = actual_variadic.get_type(offset) else {
                    break;
                };
                actual_type.clone()
            }
        };
        let AssignabilityResult::NotAssignable(mismatch) =
            semantic_model.check_assignable(&actual_type, &field_expected_type)
        else {
            if matches!(field_expected_type, LuaType::Variadic(_)) {
                break;
            }
            continue;
        };

        return context.add_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            range,
            t!(
                "Cannot assign `%{value}` (the %{index}-th value of the variable-length value) to `%{source}` at index `%{source_index}`. %{reason}",
                index = offset + 1,
                source_index = index,
                value = humanize_lint_type(db, &actual_type),
                source = humanize_lint_type(db, &field_expected_type),
                reason = render_type_mismatch(db, &mismatch)
            )
            .to_string(),
            None,
        );
    }

    false
}
