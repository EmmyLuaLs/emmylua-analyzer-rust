use emmylua_parser::{LuaAstNode, LuaTableExpr};
use hashbrown::HashSet;
use itertools::Itertools;
use rowan::TextRange;
use std::borrow::Cow;
use std::sync::Arc;

use crate::{
    AssignabilityResult, DbIndex, DiagnosticCode, LuaMemberKey, LuaType, LuaUnionType, RenderLevel,
    SemanticModel, TypeSubstitutor, VariadicType, get_real_type, humanize_type,
};

use super::{
    super::{DiagnosticContext, DiagnosticMessage, humanize_lint_type, render_diagnostic_detail},
    TableAssignmentOutcome,
};

struct TableCheckState {
    remaining_fields: usize,
    /// 当前已验证字段
    current_verified_fields: HashSet<String>,
    ///是否已产生类型分配错误, 用于抑制字段缺少检查.
    has_type_error: bool,
}

impl TableCheckState {
    const MAX_FIELD_CHECK_COUNT: usize = 2048;

    fn new() -> Self {
        Self {
            remaining_fields: Self::MAX_FIELD_CHECK_COUNT,
            current_verified_fields: HashSet::new(),
            has_type_error: false,
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

    /// 记录已提供字段
    fn insert_verified(&mut self, name: String) {
        if !self.has_type_error {
            self.current_verified_fields.insert(name);
        }
    }

    fn is_verified(&self, name: &str) -> bool {
        self.current_verified_fields.contains(name)
    }

    /// 标记类型分配错误
    fn mark_type_error(&mut self) {
        if !self.has_type_error {
            self.has_type_error = true;
            self.current_verified_fields = HashSet::new();
        }
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

    // 先展开目标中的别名
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
        table_expr,
        &table_target,
        &mut state,
    ) {
        return TableAssignmentOutcome::Reported;
    }

    if context.has_diagnostic_codes_in_range(
        table_expr.get_range(),
        &[
            DiagnosticCode::AssignTypeMismatch,
            DiagnosticCode::MissingFields,
        ],
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
                state,
            );
            continue;
        }

        if semantic_model.is_assignable(&value_expr_type, &field_target) {
            if let Some(name) = member_key_to_field_name(&member_key) {
                state.insert_verified(name);
            }
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
            // 已验证字段集按层隔离
            let parent_verified_fields = std::mem::take(&mut state.current_verified_fields);
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
                        &child_table,
                        &nested_expected_type,
                        state,
                    );
                }
            }

            // 恢复父层集合.
            state.current_verified_fields = if state.has_type_error {
                HashSet::new()
            } else {
                parent_verified_fields
            };

            if child_has_err {
                has_diagnostic = true;
            } else if deep_check && !state.is_exhausted() {
                // 子表检查通过, 标记为已验证字段, 避免缺失检查误报.
                if let Some(name) = member_key_to_field_name(&member_key) {
                    state.insert_verified(name);
                }
            } else {
                // 回退整字段粗粒度诊断确保不静默.
                has_diagnostic |= add_table_type_mismatch(
                    context,
                    semantic_model,
                    state,
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
            state,
            field.get_range(),
            &field_target,
            &value_expr_type,
        );
    }

    has_diagnostic
}

fn check_table_missing_fields(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    table_expr: &LuaTableExpr,
    table_target: &LuaType,
    state: &mut TableCheckState,
) -> bool {
    // 类型分配检查优先级高于缺失检查.
    if state.is_exhausted() || state.has_type_error {
        return false;
    }

    let db = semantic_model.get_db();

    // 联合目标按分支可满足性判定: 提供的字段完整满足任一分支即视为通过.
    if let LuaType::Union(union) = get_real_type(db, table_target).unwrap_or(table_target) {
        return check_union_missing_fields(
            context,
            semantic_model,
            table_expr,
            table_target,
            union,
            state,
        );
    }

    if !can_check_missing_fields(db, table_target) {
        return false;
    }

    let Some(unverified_required_fields) =
        collect_unverified_required_fields(context, semantic_model, state, table_target)
    else {
        return false;
    };

    report_missing_fields(
        context,
        db,
        table_expr.get_range(),
        table_target,
        unverified_required_fields,
    )
}

/// 联合目标逐分支收集未满足的必填字段: 任一分支被完整提供即通过;
/// 全部分支都不满足时合并各分支缺失字段上报.
fn check_union_missing_fields(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    table_expr: &LuaTableExpr,
    table_target: &LuaType,
    union: &LuaUnionType,
    state: &mut TableCheckState,
) -> bool {
    let LuaUnionType::Multi(types) = union else {
        return false;
    };

    let db = semantic_model.get_db();
    let mut missing_fields = HashSet::new();
    for ty in types {
        let ty = get_real_type(db, ty).unwrap_or(ty);
        if ty.is_nil() {
            continue;
        }
        // 无法做必填分析的分支无法断定其不满足, 放行.
        if !can_check_missing_fields(db, ty) {
            return false;
        }
        if state.is_exhausted() {
            return false;
        }
        let Some(missing) = collect_unverified_required_fields(context, semantic_model, state, ty)
        else {
            // 分支检查被预算截断, 无法确认所有分支都不满足, 放弃本次报告.
            return false;
        };
        if missing.is_empty() {
            // 该分支已被完整提供.
            return false;
        }
        missing_fields.extend(missing);
    }

    if missing_fields.is_empty() {
        return false;
    }

    report_missing_fields(
        context,
        db,
        table_expr.get_range(),
        table_target,
        missing_fields,
    )
}

/// 收集目标类型中尚未提供的必填字段并缓存.
fn collect_unverified_required_fields(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    state: &mut TableCheckState,
    table_target: &LuaType,
) -> Option<HashSet<String>> {
    let cached = context
        .required_fields_cache
        .get(table_target)
        .map(Arc::clone);

    let required_field_names = match cached {
        Some(required_field_names) => required_field_names,
        None => {
            let mut names = Vec::new();
            if let Some(members) = semantic_model.get_member_infos(table_target) {
                for member in members {
                    let Some(name) = member_key_to_field_name(&member.key) else {
                        continue;
                    };
                    // 只要有任意声明不可赋值为 nil 即为必填字段.
                    if !semantic_model.is_assignable(&LuaType::Nil, &member.typ) {
                        names.push(name);
                    }
                }
            }

            let names = Arc::new(names);
            context
                .required_fields_cache
                .insert(table_target.clone(), Arc::clone(&names));
            names
        }
    };

    let mut unverified_required_fields = HashSet::new();
    for name in required_field_names.iter() {
        // 在字段检查中已验证通过
        if state.is_verified(name) {
            continue;
        }
        // 在此才消耗预算
        if !state.enter_field() {
            return None;
        }
        unverified_required_fields.insert(name.clone());
    }
    Some(unverified_required_fields)
}

fn report_missing_fields(
    context: &mut DiagnosticContext,
    db: &DbIndex,
    range: TextRange,
    table_type: &LuaType,
    missing_fields: HashSet<String>,
) -> bool {
    if missing_fields.is_empty() {
        return false;
    }

    let missing = missing_fields
        .into_iter()
        .sorted_unstable()
        .map(|name| format!("`{name}`"))
        .join(", ");

    context.add_diagnostic(
        DiagnosticCode::MissingFields,
        range,
        t!(
            "Missing required fields in type `%{typ}`: %{fields}",
            typ = humanize_lint_type(db, table_type),
            fields = missing
        )
        .to_string(),
        None,
    )
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

fn member_key_to_field_name(key: &LuaMemberKey) -> Option<String> {
    match key {
        LuaMemberKey::Name(name) => Some(name.to_string()),
        LuaMemberKey::Integer(index) => Some(format!("[{}]", index)),
        // TypeKey 是索引器, 因此不计入检查
        LuaMemberKey::None | LuaMemberKey::TypeKey(_) => None,
    }
}

fn add_table_type_mismatch(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    state: &mut TableCheckState,
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
    let reported = context.add_diagnostic(
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
    );
    if reported {
        state.mark_type_error();
    }
    reported
}

fn check_table_last_variadic_type(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    expected_type: &LuaType,
    start_index: i64,
    actual_variadic: &VariadicType,
    range: TextRange,
    state: &mut TableCheckState,
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
            state.insert_verified(format!("[{}]", index));
            if matches!(field_expected_type, LuaType::Variadic(_)) {
                break;
            }
            continue;
        };

        let reported = context.add_diagnostic(
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
        if reported {
            state.mark_type_error();
        }
        return reported;
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
    const MAX_EXPAND_DEPTH: u32 = 8;

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
        current = next;
    }
    None
}
