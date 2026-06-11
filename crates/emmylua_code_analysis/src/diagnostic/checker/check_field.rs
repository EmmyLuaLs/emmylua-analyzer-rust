//! Check field checker — salsa-native.
//!
//! 检查字段注入（InjectField）和未定义字段（UndefinedField）。

use std::collections::HashSet;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaElseIfClauseStat, LuaExpr, LuaForRangeStat, LuaForStat, LuaIfStat,
    LuaIndexExpr, LuaIndexKey, LuaRepeatStat, LuaSyntaxKind, LuaTokenKind, LuaVarExpr,
    LuaWhileStat,
};

use crate::semantic_model::{InferFailReason, SemanticModel};
use crate::{DiagnosticCode, LuaMemberKey, LuaSemanticDeclId, LuaType};

use super::{DiagnosticContext, humanize_lint_type};

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    let mut checked = HashSet::new();
    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaAssignStat(assign) => {
                let (vars, _) = assign.get_var_and_expr_list();
                for var in vars.iter() {
                    if let LuaVarExpr::IndexExpr(ix) = var {
                        checked.insert(ix.syntax().clone());
                        check_index(context, model, ix, DiagnosticCode::InjectField);
                    }
                }
            }
            LuaAst::LuaIndexExpr(ix) => {
                if !checked.contains(ix.syntax()) {
                    check_index(context, model, &ix, DiagnosticCode::UndefinedField);
                }
            }
            _ => {}
        }
    }
}

fn check_index(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    index_expr: &LuaIndexExpr,
    code: DiagnosticCode,
) {
    let Some(prefix_expr) = index_expr.get_prefix_expr() else {
        return;
    };
    let prefix_type = model
        .infer_expr(prefix_expr.clone())
        .unwrap_or(LuaType::Unknown);

    if is_invalid_prefix(&prefix_type) {
        return;
    }

    let Some(index_key) = index_expr.get_index_key() else {
        return;
    };

    if is_valid_member(model, &prefix_type, index_expr, &index_key, code) {
        return;
    }

    let index_name = index_key.get_path_part();
    let db = context.db;
    match code {
        DiagnosticCode::InjectField => {
            context.add_diagnostic(
                DiagnosticCode::InjectField,
                index_key
                    .get_range()
                    .expect("Index key should have a range"),
                t!(
                    "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                    class = humanize_lint_type(db, &prefix_type),
                    field = index_name,
                )
                .to_string(),
                None,
            );
        }
        DiagnosticCode::UndefinedField => {
            context.add_diagnostic(
                DiagnosticCode::UndefinedField,
                index_key
                    .get_range()
                    .expect("Index key should have a range"),
                t!("Undefined field `%{field}`. ", field = index_name).to_string(),
                None,
            );
        }
        _ => {}
    }
}

fn is_invalid_prefix(typ: &LuaType) -> bool {
    let mut cur = typ;
    loop {
        match cur {
            LuaType::Any
            | LuaType::Unknown
            | LuaType::Table
            | LuaType::TplRef(_)
            | LuaType::StrTplRef(_)
            | LuaType::TableConst(_) => return true,
            LuaType::Instance(inst) => cur = inst.get_base(),
            LuaType::Intersection(inter) => return inter.get_types().iter().any(is_invalid_prefix),
            _ => return false,
        }
    }
}

fn is_valid_member(
    model: &SemanticModel,
    prefix_type: &LuaType,
    index_expr: &LuaIndexExpr,
    index_key: &LuaIndexKey,
    code: DiagnosticCode,
) -> bool {
    match prefix_type {
        LuaType::Global | LuaType::Userdata => return true,
        LuaType::Array(arr) => {
            if arr.get_base().is_unknown() {
                return true;
            }
        }
        LuaType::Ref(_) => {
            if check_enum_is_param(model, prefix_type, index_expr) {
                return false;
            }
        }
        LuaType::Intersection(inter) => {
            for comp in inter.get_types() {
                if is_valid_member(model, comp, index_expr, index_key, code) {
                    return true;
                }
            }
            if model
                .infer_expr(LuaExpr::IndexExpr(index_expr.clone()))
                .is_ok()
            {
                return true;
            }
        }
        _ => {}
    }

    // 条件语句中的 `[]` 访问可以宽松处理
    if code == DiagnosticCode::UndefinedField && in_conditional(index_expr) {
        for child in index_expr.syntax().children_with_tokens() {
            if child.kind() == LuaTokenKind::TkLeftBracket.into() {
                match prefix_type {
                    LuaType::Ref(id) | LuaType::Def(id) => {
                        if !model.is_enum_type(id) {
                            return true;
                        }
                    }
                    _ => return true,
                }
            }
        }
    }

    // 检查是否有 semantic declaration
    let has_semantic_decl = model
        .find_decl_by_node(index_expr.syntax().clone(), Default::default())
        .is_some();
    if !has_semantic_decl {
        let index_type = model.infer_expr(LuaExpr::IndexExpr(index_expr.clone()));
        if index_type.is_ok_and(|t| !t.is_unknown()) {
            return true;
        }
    }
    if has_semantic_decl {
        return true;
    }

    // 获取 key 类型
    let key_type = match index_key {
        LuaIndexKey::Expr(expr) => match model.infer_expr(expr.clone()) {
            Ok(
                LuaType::Any
                | LuaType::Unknown
                | LuaType::Table
                | LuaType::TplRef(_)
                | LuaType::StrTplRef(_),
            ) => {
                return true;
            }
            Ok(t) => t,
            Err(InferFailReason::UnResolveDeclType(_)) => return true,
            Err(_) => return false,
        },
        _ => return false,
    };

    // Def type + class → string key 总是合法
    if let LuaType::Def(id) = prefix_type {
        if model.is_class_type(id) {
            if code == DiagnosticCode::InjectField {
                return true;
            }
            if index_key.is_string() || key_type.is_string() {
                return true;
            }
        }
    }

    // 通过 member infos 验证
    let key_types = get_key_types(model, &key_type);
    if key_types.is_empty() {
        return false;
    }

    let prefix_types = expand_prefix_types(prefix_type);
    for pt in &prefix_types {
        if let Some(member_keys) = model.get_member_infos(pt) {
            for mk in &member_keys {
                match mk {
                    LuaMemberKey::ExprType(t) => {
                        if t.is_string()
                            && key_types
                                .iter()
                                .any(|kt| kt.is_string() || kt.is_str_tpl_ref())
                        {
                            return true;
                        }
                        if t.is_integer() && key_types.iter().any(|kt| kt.is_integer()) {
                            return true;
                        }
                    }
                    LuaMemberKey::Name(_) => {
                        if key_types
                            .iter()
                            .any(|kt| kt.is_string() || kt.is_str_tpl_ref())
                        {
                            return true;
                        }
                    }
                    LuaMemberKey::Integer(_) => {
                        if key_types.iter().any(|kt| kt.is_integer()) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            if member_keys.is_empty() {
                if check_enum_self_ref(model, pt, &key_types) {
                    return true;
                }
            }
        } else if check_enum_self_ref(model, pt, &key_types) {
            return true;
        }
    }

    false
}

fn check_enum_is_param(
    model: &SemanticModel,
    prefix_type: &LuaType,
    index_expr: &LuaIndexExpr,
) -> bool {
    let LuaType::Ref(id) = prefix_type else {
        return false;
    };
    if !model.is_enum_type(id) {
        return false;
    }
    let Some(prefix) = index_expr.get_prefix_expr() else {
        return false;
    };
    let Some(decl) = model.find_decl_by_node(prefix.syntax().clone(), Default::default()) else {
        return false;
    };
    let LuaSemanticDeclId::LuaDecl(decl_id) = decl else {
        return false;
    };
    // 检查声明是否为参数：通过 decl_tree 查找
    let Some(decl_tree) = model.decl_tree() else {
        return false;
    };
    decl_tree.decls.iter().any(|d| {
        d.id.0 == decl_id.position
            && matches!(
                d.kind,
                crate::compilation::SalsaDeclKindSummary::Param { .. }
            )
    })
}

fn check_enum_self_ref(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key_types: &HashSet<LuaType>,
) -> bool {
    match prefix_type {
        LuaType::Ref(id) | LuaType::Def(id) => {
            model.is_enum_type(id)
                && key_types
                    .iter()
                    .any(|kt| matches!(kt, LuaType::Ref(kid) | LuaType::Def(kid) if id == kid))
        }
        _ => false,
    }
}

fn expand_prefix_types(prefix_type: &LuaType) -> HashSet<LuaType> {
    let mut set = HashSet::new();
    let mut stack = vec![prefix_type.clone()];
    let mut visited = HashSet::new();
    while let Some(cur) = stack.pop() {
        if !visited.insert(cur.clone()) {
            continue;
        }
        match &cur {
            LuaType::Union(u) => {
                for t in u.into_vec() {
                    stack.push(t.clone());
                }
            }
            LuaType::Any | LuaType::Unknown | LuaType::Nil => {}
            _ => {
                set.insert(cur);
            }
        }
    }
    set
}

fn get_key_types(_model: &SemanticModel, typ: &LuaType) -> HashSet<LuaType> {
    let mut set = HashSet::new();
    let mut stack = vec![typ.clone()];
    let mut visited = HashSet::new();
    while let Some(cur) = stack.pop() {
        if !visited.insert(cur.clone()) {
            continue;
        }
        match &cur {
            LuaType::String
            | LuaType::Integer
            | LuaType::StrTplRef(_)
            | LuaType::Ref(_)
            | LuaType::DocStringConst(_)
            | LuaType::DocIntegerConst(_) => {
                set.insert(cur);
            }
            LuaType::Union(u) => {
                for t in u.into_vec() {
                    stack.push(t.clone());
                }
            }
            // keyof(T) 暂时跳过（后续实现）
            _ => {}
        }
    }
    set
}

fn in_conditional<T: LuaAstNode>(node: &T) -> bool {
    let node_range = node.get_range();
    for ancestor in node.syntax().ancestors() {
        match ancestor.kind().into() {
            LuaSyntaxKind::IfStat => {
                if let Some(s) = LuaIfStat::cast(ancestor) {
                    if s.get_condition_expr()
                        .is_some_and(|e| e.get_range().contains_range(node_range))
                    {
                        return true;
                    }
                }
            }
            LuaSyntaxKind::WhileStat => {
                if let Some(s) = LuaWhileStat::cast(ancestor) {
                    if s.get_condition_expr()
                        .is_some_and(|e| e.get_range().contains_range(node_range))
                    {
                        return true;
                    }
                }
            }
            LuaSyntaxKind::ForStat => {
                if let Some(s) = LuaForStat::cast(ancestor) {
                    if s.get_iter_expr()
                        .any(|e| e.get_range().contains_range(node_range))
                    {
                        return true;
                    }
                }
            }
            LuaSyntaxKind::ForRangeStat => {
                if let Some(s) = LuaForRangeStat::cast(ancestor) {
                    if s.get_expr_list()
                        .any(|e| e.get_range().contains_range(node_range))
                    {
                        return true;
                    }
                }
            }
            LuaSyntaxKind::RepeatStat => {
                if let Some(s) = LuaRepeatStat::cast(ancestor) {
                    if s.get_condition_expr()
                        .is_some_and(|e| e.get_range().contains_range(node_range))
                    {
                        return true;
                    }
                }
            }
            LuaSyntaxKind::ElseIfClauseStat => {
                if let Some(s) = LuaElseIfClauseStat::cast(ancestor) {
                    if s.get_condition_expr()
                        .is_some_and(|e| e.get_range().contains_range(node_range))
                    {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}
