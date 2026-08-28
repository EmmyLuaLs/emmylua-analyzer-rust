use emmylua_parser::{LuaAstNode, LuaTableExpr};
use rowan::TextRange;
use std::borrow::Cow;

use crate::{
    AssignabilityResult, DbIndex, DiagnosticCode, LuaMemberKey, LuaType, LuaUnionType, RenderLevel,
    SemanticModel, TypeMismatchKind, TypeSubstitutor, VariadicType, get_real_type, humanize_type,
};

use super::{
    super::{
        DiagnosticContext, DiagnosticMessage, format_missing_fields, render_diagnostic_detail,
    },
    TableAssignmentOutcome,
};

struct TableCheckState {
    remaining_fields: usize,
}

impl TableCheckState {
    const MAX_FIELD_CHECK_COUNT: usize = 2048;

    fn new() -> Self {
        Self {
            remaining_fields: Self::MAX_FIELD_CHECK_COUNT,
        }
    }

    /// 预算是否已耗尽
    fn is_exhausted(&self) -> bool {
        self.remaining_fields == 0
    }

    fn enter_field(&mut self) -> bool {
        if self.remaining_fields == 0 {
            return false;
        }
        self.remaining_fields -= 1;
        true
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

    // 泛型条件类型直接放弃细化回退到整体诊断. 因为其复杂度不可控.
    if is_generic_conditional_type(semantic_model.get_db(), target) {
        return TableAssignmentOutcome::Fallback;
    }

    // 展开目标中的别名
    let Some(canonical_target) = expand_field_check_type(semantic_model.get_db(), target) else {
        return TableAssignmentOutcome::Fallback;
    };

    let Some(table_target) =
        get_table_field_target(semantic_model.get_db(), table_expr, &canonical_target)
    else {
        return TableAssignmentOutcome::Fallback;
    };

    let mut state = TableCheckState::new();

    if check_table_fields(
        context,
        semantic_model,
        &table_target,
        table_expr,
        &mut state,
    ) {
        return TableAssignmentOutcome::Reported;
    }

    // 回退检查是否缺失必填字段.
    if check_table_missing_fields(
        context,
        semantic_model,
        source,
        &table_target,
        table_expr.get_range(),
    ) {
        return TableAssignmentOutcome::Reported;
    }

    TableAssignmentOutcome::Fallback
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

        // 先展开别名
        let canonical_target = expand_field_check_type(semantic_model.get_db(), &field_target);

        if let Some(child_table) = LuaTableExpr::cast(value_expr.syntax().clone())
            && let Some(nested_expected_type) = canonical_target.as_deref().and_then(|canonical| {
                get_table_field_target(semantic_model.get_db(), &child_table, canonical)
            })
        {
            let deep_check = !state.is_exhausted();
            let mut child_has_err = false;
            if deep_check {
                child_has_err = check_table_fields(
                    context,
                    semantic_model,
                    &nested_expected_type,
                    &child_table,
                    state,
                );
                if !child_has_err {
                    child_has_err = check_table_missing_fields(
                        context,
                        semantic_model,
                        &value_expr_type,
                        &nested_expected_type,
                        child_table.get_range(),
                    );
                }
            }

            if child_has_err {
                has_diagnostic = true;
            } else if !deep_check || state.is_exhausted() {
                // 回退整字段粗粒度诊断确保不静默.
                has_diagnostic |= add_table_type_mismatch(
                    context,
                    semantic_model,
                    field.get_range(),
                    &field_target,
                    &value_expr_type,
                );
            }
            continue;
        }

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

/// 缺失必填字段检查: 委托类型检查层收集全部缺失字段, 仅在其结果为缺失字段时上报.
fn check_table_missing_fields(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    source: &LuaType,
    target: &LuaType,
    range: TextRange,
) -> bool {
    let db = semantic_model.get_db();
    // Def 目标(类定义表, 其字段由后续赋值填充)与不可枚举成员的目标不做缺失检查;
    // 联合目标交由类型检查层按分支可满足性处理.
    let real_target = get_real_type(db, target).unwrap_or(target);
    if !matches!(real_target, LuaType::Union(_)) && !can_check_missing_fields(db, target) {
        return false;
    }

    // 无法完成的类型关系按保守处理, 不上报缺失字段.
    let AssignabilityResult::NotAssignable(mismatch) =
        semantic_model.check_assignable(source, target)
    else {
        return false;
    };
    let TypeMismatchKind::MissingMembers { keys } = mismatch.reason() else {
        return false;
    };
    report_missing_fields(
        context,
        semantic_model.get_db(),
        range,
        source,
        target,
        keys,
    )
}

fn report_missing_fields(
    context: &mut DiagnosticContext,
    db: &DbIndex,
    range: TextRange,
    source: &LuaType,
    target: &LuaType,
    keys: &[LuaMemberKey],
) -> bool {
    let Some(message) = format_missing_fields(db, source, target, keys) else {
        return false;
    };
    context.add_diagnostic(DiagnosticCode::MissingFields, range, message, None)
}

/// 该类型是否具有可枚举的声明成员(类/对象/含此类成员的交叉类型), 可对其进行缺失必填字段检查.
fn can_check_missing_fields(db: &DbIndex, table_type: &LuaType) -> bool {
    let table_type = get_real_type(db, table_type).unwrap_or(table_type);
    match table_type {
        LuaType::Object(_) => true,
        LuaType::Ref(type_id) => db
            .get_type_index()
            .get_type_decl(type_id)
            .is_some_and(|type_decl| type_decl.is_class()),
        LuaType::Generic(generic) => {
            let type_id = generic.get_base_type_id_ref();
            db.get_type_index()
                .get_type_decl(type_id)
                .is_some_and(|type_decl| type_decl.is_class())
        }
        LuaType::Intersection(intersection) => intersection
            .get_types()
            .iter()
            .any(|t| can_check_missing_fields(db, t)),
        _ => false,
    }
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

fn get_table_field_target<'a>(
    db: &'a DbIndex,
    table_expr: &LuaTableExpr,
    typ: &'a LuaType,
) -> Option<&'a LuaType> {
    let typ = get_real_type(db, typ).unwrap_or(typ);
    match typ {
        LuaType::Union(union) => match union.as_ref() {
            LuaUnionType::Nullable(inner) => get_table_field_target(db, table_expr, inner),
            LuaUnionType::Basic(_) => None,
            LuaUnionType::Multi(types) => {
                let non_nil: Vec<_> = types
                    .iter()
                    .map(|t| get_real_type(db, t).unwrap_or(t))
                    .filter(|t| !t.is_nil())
                    .collect();
                if non_nil.is_empty() {
                    return None;
                }
                if non_nil.iter().all(|t| is_table_field_target(db, t)) {
                    // 如果字面量是具名表且联合类型中包含数组类型（如 `Foo | Foo[]`），优先筛选结构体候选
                    if !table_expr.is_array()
                        && non_nil
                            .iter()
                            .any(|t| matches!(t, LuaType::Array(_) | LuaType::Tuple(_)))
                    {
                        let struct_candidates: Vec<_> = non_nil
                            .iter()
                            .copied()
                            .filter(|t| can_check_missing_fields(db, t))
                            .collect();
                        if struct_candidates.len() == 1 {
                            return Some(struct_candidates[0]);
                        }
                    }
                    return Some(typ);
                }
                if !table_expr.is_array() {
                    let mut candidate = None;
                    for t in non_nil {
                        if is_table_field_target(db, t) {
                            if candidate.is_some() {
                                return None;
                            }
                            candidate = Some(t);
                        }
                    }
                    return candidate;
                }
                None
            }
        },
        _ => {
            if is_table_field_target(db, typ) {
                Some(typ)
            } else {
                None
            }
        }
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
        LuaType::Intersection(intersection) => intersection
            .get_types()
            .iter()
            .any(|t| is_table_field_target(db, t)),
        LuaType::Union(union) => match union.as_ref() {
            LuaUnionType::Nullable(inner) => is_table_field_target(db, inner),
            LuaUnionType::Basic(_) => false,
            LuaUnionType::Multi(types) => {
                let non_nil: Vec<_> = types.iter().filter(|t| !t.is_nil()).collect();
                !non_nil.is_empty() && non_nil.iter().all(|t| is_table_field_target(db, t))
            }
        },
        _ => false,
    }
}

fn expand_field_check_type<'a>(db: &DbIndex, typ: &'a LuaType) -> Option<Cow<'a, LuaType>> {
    const MAX_EXPAND_DEPTH: u32 = 32;

    let needs_expand = match typ {
        LuaType::Ref(type_id) => db
            .get_type_index()
            .get_type_decl(type_id)
            .is_some_and(|type_decl| type_decl.is_alias()),
        LuaType::Generic(generic) => db
            .get_type_index()
            .get_type_decl(generic.get_base_type_id_ref())
            .is_some_and(|type_decl| type_decl.is_alias()),
        _ => false,
    };
    if !needs_expand {
        return Some(Cow::Borrowed(typ));
    }

    let mut current = typ.clone();
    for _ in 0..MAX_EXPAND_DEPTH {
        let next = match &current {
            LuaType::Ref(type_id) => {
                let type_decl = db.get_type_index().get_type_decl(type_id)?;
                if !type_decl.is_alias() {
                    return Some(Cow::Owned(current));
                }
                type_decl.get_alias_origin(db, None)?
            }
            LuaType::Generic(generic) => {
                let base_type_id = generic.get_base_type_id_ref();
                let type_decl = db.get_type_index().get_type_decl(base_type_id)?;
                if !type_decl.is_alias() {
                    return Some(Cow::Owned(current));
                }
                let substitutor = TypeSubstitutor::from_alias(
                    generic.get_params().clone(),
                    generic.get_base_type_id(),
                );
                type_decl.get_alias_origin(db, Some(&substitutor))?
            }
            _ => return Some(Cow::Owned(current)),
        };
        current = next.into_owned();
    }
    None
}

/// 检查类型是否为泛型条件类型(或指向条件类型的别名).
fn is_generic_conditional_type(db: &DbIndex, typ: &LuaType) -> bool {
    const MAX_DEPTH: u32 = 32;
    let mut current = typ;
    for _ in 0..MAX_DEPTH {
        match current {
            LuaType::Conditional(_) => return true,
            LuaType::Generic(generic) => {
                let base_type_id = generic.get_base_type_id_ref();
                let Some(type_decl) = db.get_type_index().get_type_decl(base_type_id) else {
                    return false;
                };
                if !type_decl.is_alias() {
                    return false;
                }
                let Some(origin) = type_decl.get_alias_ref() else {
                    return false;
                };
                current = origin;
            }
            LuaType::Ref(type_id) | LuaType::Def(type_id) => {
                let Some(type_decl) = db.get_type_index().get_type_decl(type_id) else {
                    return false;
                };
                if !type_decl.is_alias() {
                    return false;
                }
                let Some(origin) = type_decl.get_alias_ref() else {
                    return false;
                };
                current = origin;
            }
            _ => return false,
        }
    }
    false
}
