use emmylua_parser::{LuaAstNode, LuaTableExpr};
use rowan::TextRange;

use crate::{
    AssignabilityResult, DbIndex, DiagnosticCode, LuaMemberKey, LuaType, LuaUnionType, RenderLevel,
    SemanticModel, VariadicType, get_real_type, humanize_type,
};

use super::{
    super::{DiagnosticContext, DiagnosticMessage, render_diagnostic_detail},
    TableAssignmentOutcome,
};

struct TableCheckState {
    remaining_fields: usize,
    budget_exhausted: bool,
}

impl TableCheckState {
    const MAX_FIELD_CHECK_COUNT: usize = 512;

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
    source: &LuaType,
    target: &LuaType,
) -> TableAssignmentOutcome {
    // 整表兼容时不访问任何字段 AST. 无法完成的类型关系同样按保守兼容处理.
    if semantic_model.is_assignable(source, target) {
        return TableAssignmentOutcome::Assignable;
    }

    let mut state = TableCheckState::new();
    if let Some(table_target) = get_table_field_target(semantic_model.get_db(), target)
        && check_table_fields(
            context,
            semantic_model,
            table_target,
            table_expr,
            &mut state,
        )
    {
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
    target: &LuaType,
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
        let value_expr_type = semantic_model
            .infer_expr(value_expr.clone())
            .unwrap_or(LuaType::Any);

        let Some(member_key) = semantic_model.get_member_key(&field_key) else {
            continue;
        };

        let Ok(field_target) = semantic_model.infer_member_type(target, &member_key) else {
            continue;
        };

        // 最后的顺序字段可以展开函数调用的多返回值.
        if field.is_value_field()
            && fields.peek().is_none()
            && let LuaMemberKey::Integer(start_index) = &member_key
            && let LuaType::Variadic(variadic) = &value_expr_type
        {
            has_diagnostic |= check_table_last_variadic_type(
                context,
                semantic_model,
                target,
                *start_index,
                variadic,
                field.get_range(),
            );
            continue;
        }

        if semantic_model.is_assignable(&value_expr_type, &field_target) {
            continue;
        }

        // 此时不匹配, 如果右侧仍然是表字面量, 则需要递归检查其字段.
        // 期望类型可能是可空 union(如 `Foo?`), 此时若真实值是表字面量仍需递归检查字段.
        let nested_expected_type = get_table_field_target(semantic_model.get_db(), &field_target);

        if let Some(nested_expected_type) = nested_expected_type
            && let Some(child_table) = LuaTableExpr::cast(value_expr.syntax().clone())
        {
            let field_has_diagnostic = if state.check_field_budget() {
                add_table_type_mismatch(
                    context,
                    semantic_model,
                    field.get_range(),
                    &field_target,
                    &value_expr_type,
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
                            &field_target,
                            &value_expr_type,
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
            &field_target,
            &value_expr_type,
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
        DiagnosticMessage::with_detail(
            t!(
                "Cannot assign `%{value}` to `%{source}`.",
                value = humanize_type(db, actual_type, RenderLevel::Simple),
                source = humanize_type(db, expected_type, RenderLevel::Simple),
            )
            .to_string(),
            render_diagnostic_detail(db, &mismatch, actual_type, expected_type),
        ),
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
            DiagnosticMessage::with_detail(
                t!(
                    "Cannot assign `%{value}` (the %{index}-th value of the variable-length value) to `%{source}` at index `%{source_index}`.",
                    index = offset + 1,
                    source_index = index,
                    value = humanize_type(db, &actual_type, RenderLevel::Simple),
                    source = humanize_type(db, &field_expected_type, RenderLevel::Simple),
                )
                .to_string(),
                render_diagnostic_detail(db, &mismatch, &actual_type, &field_expected_type),
            ),
            None,
        );
    }

    false
}

fn get_table_field_target<'a>(db: &'a DbIndex, typ: &'a LuaType) -> Option<&'a LuaType> {
    // 此处只做宽泛结构判断, 泛型 alias 回退到整表诊断.
    let typ = get_real_type(db, typ).unwrap_or(typ);
    let typ = match typ {
        LuaType::Union(union) => match union.as_ref() {
            LuaUnionType::Nullable(inner) => get_real_type(db, inner).unwrap_or(inner),
            _ => typ,
        },
        _ => typ,
    };
    if is_table_field_target(db, typ) {
        Some(typ)
    } else {
        None
    }
}

fn is_table_field_target(db: &DbIndex, typ: &LuaType) -> bool {
    let typ = get_real_type(db, typ).unwrap_or(typ);
    if typ.is_table() || matches!(typ, LuaType::Object(_)) {
        return true;
    }

    match typ {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => db
            .get_type_index()
            .get_type_decl(type_id)
            .is_some_and(|type_decl| type_decl.is_class()),
        LuaType::Generic(generic) => {
            let type_id = generic.get_base_type_id_ref();
            db.get_type_index()
                .get_type_decl(type_id)
                .is_some_and(|type_decl| type_decl.is_class())
        }
        LuaType::Union(union) => {
            let non_nil: Vec<_> = union
                .into_vec()
                .into_iter()
                .filter(|t| !t.is_nil())
                .collect();
            !non_nil.is_empty() && non_nil.iter().all(|t| is_table_field_target(db, t))
        }
        _ => false,
    }
}
