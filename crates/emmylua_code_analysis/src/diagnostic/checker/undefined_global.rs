//! Undefined global checker — salsa-native.

use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaFuncStat, LuaNameExpr, LuaVarExpr};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let use_ranges = collect_use_ranges(model);
    let root = model.get_root().clone();
    for name_expr in root.descendants::<LuaNameExpr>() {
        check_name(context, model, &use_ranges, name_expr);
    }
}

fn collect_use_ranges(model: &SemanticModel) -> HashSet<TextRange> {
    let Some(decl_tree) = model.decl_tree() else {
        return HashSet::new();
    };
    let mut ranges = HashSet::new();
    for decl in &decl_tree.decls {
        if let Some(refs) = model.decl_references(decl.id) {
            for r in &refs {
                ranges.insert(r.syntax_id.text_range());
            }
        }
    }
    ranges
}

fn check_name(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    use_ranges: &HashSet<TextRange>,
    name_expr: LuaNameExpr,
) {
    let name_range = name_expr.get_range();
    if use_ranges.contains(&name_range) {
        return;
    }

    let Some(name_text) = name_expr.get_name_text() else {
        return;
    };
    if name_text == "_" {
        return;
    }

    // 全局声明存在？
    let db = model.salsa_db();
    if db.types().global(model.get_file_id(), &name_text).is_some() {
        return;
    }

    // 配置中的例外
    if context
        .config
        .global_disable_set
        .contains(name_text.as_str())
    {
        return;
    }
    if context
        .config
        .global_disable_glob
        .iter()
        .any(|re| re.is_match(&name_text))
    {
        return;
    }

    // self 检查
    if name_text == "self" && is_self_valid(name_expr) {
        return;
    }

    context.add_diagnostic(
        DiagnosticCode::UndefinedGlobal,
        name_range,
        t!("undefined global variable: %{name}", name = name_text).to_string(),
        None,
    );
}

/// self 在 `function t:method()` 的 body 中总是合法。
fn is_self_valid(name_expr: LuaNameExpr) -> bool {
    let closures = name_expr.ancestors::<LuaClosureExpr>();
    for closure in closures {
        let Some(func_stat) = closure.get_parent::<LuaFuncStat>() else {
            continue;
        };
        let Some(func_name) = func_stat.get_func_name() else {
            continue;
        };
        if matches!(func_name, LuaVarExpr::IndexExpr(_)) {
            return true;
        }
    }
    false
}
