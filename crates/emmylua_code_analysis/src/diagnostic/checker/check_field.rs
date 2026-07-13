//! Check field checker — salsa-native.
//!
//! 检查字段注入（InjectField）和未定义字段（UndefinedField）。

use std::collections::HashSet;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaElseIfClauseStat, LuaExpr, LuaForRangeStat, LuaForStat, LuaIfStat,
    LuaIndexExpr, LuaIndexKey, LuaRepeatStat, LuaSyntaxKind, LuaTokenKind, LuaVarExpr,
    LuaWhileStat,
};

use crate::compilation::{SalsaDeclId, SalsaDeclKindSummary};
use crate::diagnostic::checker::humanize_lint_type_salsa;
use crate::semantic_model::{DeclPosition, SemanticModel};
use crate::{DiagnosticCode, LuaMemberKey, LuaSemanticDeclId, LuaType};

use super::DiagnosticContext;

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

    // 对 UndefinedField，无效前缀直接跳过
    if code == DiagnosticCode::UndefinedField && is_invalid_prefix(&prefix_type) {
        return;
    }

    let Some(index_key) = index_expr.get_index_key() else {
        return;
    };
    let Some(key) = index_key_to_member_key(model, &index_key, code) else {
        return;
    };

    // ── 核心：通过 infer_member_type 检查字段是否存在 ──
    if is_valid_access(model, &prefix_type, index_expr, &key, code) {
        return;
    }

    // InjectField: 即使前缀类型无效，如果属性已声明也不报
    if code == DiagnosticCode::InjectField && is_invalid_prefix(&prefix_type) {
        if model.infer_member_type(&prefix_type, &key).is_ok() {
            return;
        }
    }

    let Some(range) = index_key.get_range() else {
        return;
    };
    let index_name = index_key.get_path_part();
    match code {
        DiagnosticCode::InjectField => {
            context.add_diagnostic(
                DiagnosticCode::InjectField,
                range,
                t!(
                    "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                    class = humanize_lint_type_salsa(
                        context.get_salsa_db(),
                        context.get_file_id(),
                        &prefix_type
                    ),
                    field = index_name,
                )
                .to_string(),
                None,
            );
        }
        DiagnosticCode::UndefinedField => {
            context.add_diagnostic(
                DiagnosticCode::UndefinedField,
                range,
                t!("Undefined field `%{field}`. ", field = index_name).to_string(),
                None,
            );
        }
        _ => {}
    }
}

/// 将 index_key 转换为 LuaMemberKey，处理表达式类型的回退。
fn index_key_to_member_key(
    model: &SemanticModel,
    index_key: &LuaIndexKey,
    code: DiagnosticCode,
) -> Option<LuaMemberKey> {
    match index_key {
        LuaIndexKey::Name(name) => Some(LuaMemberKey::Name(smol_str::SmolStr::new(
            name.get_name_text(),
        ))),
        LuaIndexKey::String(s) => Some(LuaMemberKey::Name(smol_str::SmolStr::new(s.get_value()))),
        LuaIndexKey::Integer(i) => match i.get_number_value() {
            emmylua_parser::NumberResult::Int(n) => Some(LuaMemberKey::Integer(n)),
            _ => None,
        },
        LuaIndexKey::Idx(i) => Some(LuaMemberKey::Integer(*i as i64)),
        LuaIndexKey::Expr(expr) => {
            let key_type = model.infer_expr(expr.clone()).unwrap_or(LuaType::Unknown);
            match &key_type {
                LuaType::Any
                | LuaType::Unknown
                | LuaType::Table
                | LuaType::TplRef(_)
                | LuaType::StrTplRef(_) => {
                    // 宽松类型 — 不报告错误
                    if code == DiagnosticCode::UndefinedField {
                        None
                    } else {
                        Some(LuaMemberKey::TypeKey(key_type))
                    }
                }
                // 常量折叠
                LuaType::StringConst(s) => Some(LuaMemberKey::Name((**s).clone())),
                LuaType::DocStringConst(s) => Some(LuaMemberKey::Name((**s).clone())),
                LuaType::IntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
                LuaType::DocIntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
                // 非具体类型 → 模糊匹配
                _ => Some(LuaMemberKey::TypeKey(key_type)),
            }
        }
    }
}

/// 核心判断：这个字段访问是否合法。
fn is_valid_access(
    model: &SemanticModel,
    prefix_type: &LuaType,
    index_expr: &LuaIndexExpr,
    key: &LuaMemberKey,
    code: DiagnosticCode,
) -> bool {
    // 前置快速通过
    if let LuaType::Global | LuaType::Userdata = prefix_type {
        return true;
    }

    // Enum/class 类型成员访问限制：非声明自身的变量不能直接访问 enum 字段
    if let LuaType::Ref(_) | LuaType::Def(_) = prefix_type {
        if check_enum_type_access(model, prefix_type, index_expr) {
            return false;
        }
    }

    // Intersection：任一分支合法即合法
    if let LuaType::Intersection(inter) = prefix_type {
        let mut any_ok = false;
        for comp in inter.get_types() {
            if model.infer_member_type(comp, key).is_ok() {
                any_ok = true;
            }
        }
        if !any_ok {
            any_ok = model.infer_member_type(prefix_type, key).is_ok();
        }
        if any_ok {
            return true;
        }
    }

    // 条件语句中的 [] 访问可以宽松
    if code == DiagnosticCode::UndefinedField && in_conditional(index_expr) {
        for child in index_expr.syntax().children_with_tokens() {
            if child.kind() == LuaTokenKind::TkLeftBracket.into()
                && !is_enum_type(model, prefix_type)
            {
                return true;
            }
        }
    }

    // InjectField 特殊处理
    if code == DiagnosticCode::InjectField
        && matches!(
            prefix_type,
            LuaType::Any | LuaType::Unknown | LuaType::Table | LuaType::Global
        )
    {
        return false;
    }

    // 主检查：infer_member_type
    if model.infer_member_type(prefix_type, key).is_ok() {
        return true;
    }

    // Def type + class → string key 注入总是可以
    if code == DiagnosticCode::InjectField
        && let LuaType::Def(id) = prefix_type
        && model.is_class_type(id)
    {
        return true;
    }

    // 启发式1：index_expr 整体有类型声明（准确度最高，优先运行）
    // infer_expr 对不存在的字段返回 Never——由 !is_never() 过滤
    let index_expr_ty = model.infer_expr(LuaExpr::IndexExpr(index_expr.clone()));
    if index_expr_ty.is_ok_and(|t| !t.is_unknown() && !t.is_never()) {
        return true;
    }

    // 启发式2：find_decl 能找到声明 → 合法
    // （暂保留，待 find_decl_covers_node 语义稳定后替换）
    if model
        .find_decl_by_node(index_expr.syntax().clone(), Default::default())
        .is_some()
    {
        return true;
    }

    // ── UndefinedField 最终裁决：has_member 确认成员存在 ──
    if code == DiagnosticCode::UndefinedField {
        return model.has_member(prefix_type, key);
    }

    false
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

fn is_enum_type(model: &SemanticModel, prefix_type: &LuaType) -> bool {
    match prefix_type {
        LuaType::Ref(id) | LuaType::Def(id) => model.is_enum_type(id),
        _ => false,
    }
}

fn check_enum_type_access(
    model: &SemanticModel,
    prefix_type: &LuaType,
    index_expr: &LuaIndexExpr,
) -> bool {
    let type_id = match prefix_type {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return false,
    };
    if !model.is_enum_type(type_id) {
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
    // enum 定义自身（被 @enum 标注的变量，非 Param）的 named_type_names 包含 enum 名，
    // 允许字段访问；Param 和其他持有 enum 类型的变量则拒绝字段访问
    let Some(decl_tree) = model.decl_tree() else {
        return false;
    };
    let is_enum_decl = decl_tree.decls.iter().any(|d| {
        d.id.as_text_size() == decl_id.position
            && !matches!(d.kind, SalsaDeclKindSummary::Param { .. })
    });
    if !is_enum_decl {
        return true;
    }
    let db = model.salsa_db();
    let salsa_did = SalsaDeclId(DeclPosition(decl_id.position));
    if let Some(dt) = db.types().decl(model.get_file_id(), salsa_did) {
        if dt
            .named_type_names
            .iter()
            .any(|n| n.as_str() == type_id.get_name())
        {
            return false;
        }
    }
    true
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
