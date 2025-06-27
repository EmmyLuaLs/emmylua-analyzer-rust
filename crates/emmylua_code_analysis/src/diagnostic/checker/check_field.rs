use std::collections::HashSet;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaElseIfClauseStat, LuaForRangeStat, LuaForStat, LuaIfStat, LuaIndexExpr,
    LuaIndexKey, LuaRepeatStat, LuaSyntaxKind, LuaTokenKind, LuaVarExpr, LuaWhileStat,
};

use crate::{
    enum_variable_is_param, parse_require_module_info, DiagnosticCode, InferFailReason,
    LuaMemberKey, LuaSemanticDeclId, LuaType, ModuleInfo, SemanticModel,
};

use super::{humanize_lint_type, Checker, DiagnosticContext};

pub struct CheckFieldChecker;

impl Checker for CheckFieldChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InjectField, DiagnosticCode::UndefinedField];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        let mut checked_index_expr = HashSet::new();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaAssignStat(assign) => {
                    let (vars, _) = assign.get_var_and_expr_list();
                    for var in vars.iter() {
                        if let LuaVarExpr::IndexExpr(index_expr) = var {
                            checked_index_expr.insert(index_expr.syntax().clone());
                            check_index_expr(
                                context,
                                semantic_model,
                                index_expr,
                                DiagnosticCode::InjectField,
                            );
                        }
                    }
                }
                LuaAst::LuaIndexExpr(index_expr) => {
                    if checked_index_expr.contains(index_expr.syntax()) {
                        continue;
                    }
                    check_index_expr(
                        context,
                        semantic_model,
                        &index_expr,
                        DiagnosticCode::UndefinedField,
                    );
                }
                _ => {}
            }
        }
    }
}

fn check_index_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index_expr: &LuaIndexExpr,
    code: DiagnosticCode,
) -> Option<()> {
    let db = context.db;
    let prefix_typ = semantic_model
        .infer_expr(index_expr.get_prefix_expr()?)
        .unwrap_or(LuaType::Unknown);
    let mut module_info = None;

    if is_invalid_prefix_type(&prefix_typ) {
        if matches!(prefix_typ, LuaType::TableConst(_)) {
            // 如果导入了被 @export 标记的表常量, 那么不应该跳过检查
            module_info = check_import_table_const(semantic_model, index_expr);
            if module_info.is_none() {
                return Some(());
            }
        } else {
            return Some(());
        }
    }

    let index_key = index_expr.get_index_key()?;

    if is_valid_member(
        semantic_model,
        &prefix_typ,
        index_expr,
        &index_key,
        code,
        module_info,
    )
    .is_some()
    {
        return Some(());
    }

    let index_name = index_key.get_path_part();
    match code {
        DiagnosticCode::InjectField => {
            context.add_diagnostic(
                DiagnosticCode::InjectField,
                index_key.get_range()?,
                t!(
                    "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                    class = humanize_lint_type(&db, &prefix_typ),
                    field = index_name,
                )
                .to_string(),
                None,
            );
        }
        DiagnosticCode::UndefinedField => {
            context.add_diagnostic(
                DiagnosticCode::UndefinedField,
                index_key.get_range()?,
                t!("Undefined field `%{field}`. ", field = index_name,).to_string(),
                None,
            );
        }
        _ => {}
    }

    Some(())
}

fn is_invalid_prefix_type(typ: &LuaType) -> bool {
    let mut current_typ = typ;
    loop {
        match current_typ {
            LuaType::Any
            | LuaType::Unknown
            | LuaType::Table
            | LuaType::TplRef(_)
            | LuaType::StrTplRef(_)
            | LuaType::TableConst(_) => return true,
            LuaType::Instance(instance_typ) => {
                current_typ = instance_typ.get_base();
            }
            _ => return false,
        }
    }
}

fn is_valid_member(
    semantic_model: &SemanticModel,
    prefix_typ: &LuaType,
    index_expr: &LuaIndexExpr,
    index_key: &LuaIndexKey,
    code: DiagnosticCode,
    module_info: Option<&ModuleInfo>,
) -> Option<()> {
    match prefix_typ {
        LuaType::Global | LuaType::Userdata => return Some(()),
        LuaType::Array(typ) => {
            if typ.is_unknown() {
                return Some(());
            }
        }
        LuaType::Ref(_) => {
            // 如果类型是 Ref 的 enum, 那么需要检查变量是否为参数, 因为作为参数的 enum 本质上是 value 而不是 enum
            if check_enum_is_param(semantic_model, prefix_typ, index_expr).is_some() {
                return None;
            }
        }
        _ => {}
    }

    // 如果位于检查语句中, 则可以做一些宽泛的检查
    if matches!(code, DiagnosticCode::UndefinedField) && in_conditional_statement(index_expr) {
        for child in index_expr.syntax().children_with_tokens() {
            if child.kind() == LuaTokenKind::TkLeftBracket.into() {
                // 此时为 [] 访问, 大部分类型都可以直接通行
                match prefix_typ {
                    LuaType::Ref(id) | LuaType::Def(id) => {
                        if let Some(decl) =
                            semantic_model.get_db().get_type_index().get_type_decl(&id)
                        {
                            // enum 仍然需要检查
                            if decl.is_enum() {
                                break;
                            } else {
                                return Some(());
                            }
                        }
                    }
                    _ => return Some(()),
                }
            }
        }
    }

    // 检查 member_info
    let need_add_diagnostic =
        match semantic_model.get_semantic_info(index_expr.syntax().clone().into()) {
            Some(info) => {
                let need = info.semantic_decl.is_none() && info.typ.is_unknown();
                // TODO: 元组类型的检查或许需要独立出来
                if !need && matches!(code, DiagnosticCode::InjectField) {
                    // if let LuaType::Tuple(tuple) = prefix_typ {
                    //     if tuple.is_infer_resolve() {
                    //         return Some(());
                    //     } else {
                    //         // 元组类型禁止修改
                    //         return None;
                    //     }
                    // }
                    // 前缀是导入的表常量, 检查定义的文件是否与导入的表常量相同, 不同则认为是非法的
                    if let Some(module_info) = module_info {
                        if let Some(LuaSemanticDeclId::Member(member_id)) = info.semantic_decl {
                            if module_info.file_id != member_id.file_id {
                                return None;
                            }
                        }
                    }
                }
                need
            }
            None => true,
        };

    if !need_add_diagnostic {
        return Some(());
    }

    let key_type = if let LuaIndexKey::Expr(expr) = index_key {
        match semantic_model.infer_expr(expr.clone()) {
            Ok(
                LuaType::Any
                | LuaType::Unknown
                | LuaType::Table
                | LuaType::TplRef(_)
                | LuaType::StrTplRef(_),
            ) => {
                return Some(());
            }
            Ok(typ) => typ,
            // 解析失败时认为其是合法的, 因为他可能没有标注类型
            Err(InferFailReason::UnResolveDeclType(_)) => {
                return Some(());
            }
            Err(_) => {
                return None;
            }
        }
    } else {
        return None;
    };

    // 一些类型组合需要特殊处理
    match (prefix_typ, &key_type) {
        // (LuaType::Tuple(tuple), LuaType::Integer | LuaType::IntegerConst(_)) => {
        //     if tuple.is_infer_resolve() {
        //         return Some(());
        //     } else {
        //         // 元组类型禁止修改
        //         return None;
        //     }
        // }
        (LuaType::Def(id), _) => {
            if let Some(decl) = semantic_model.get_db().get_type_index().get_type_decl(id) {
                if decl.is_class() {
                    if code == DiagnosticCode::InjectField {
                        return Some(());
                    }
                    if index_key.is_string() || matches!(key_type, LuaType::String) {
                        return Some(());
                    }
                }
            }
        }
        _ => {}
    }

    /*
    允许这种写法
            ---@type string?
            local field
            local a = Class[field]
    */
    let key_types = get_key_types(&key_type);
    if key_types.is_empty() {
        return None;
    }

    let prefix_types = get_prefix_types(prefix_typ);
    for prefix_type in prefix_types {
        if let Some(members) = semantic_model.get_member_infos(&prefix_type) {
            for info in &members {
                match &info.key {
                    LuaMemberKey::ExprType(typ) => {
                        if typ.is_string() {
                            if key_types
                                .iter()
                                .any(|typ| typ.is_string() || typ.is_str_tpl_ref())
                            {
                                return Some(());
                            }
                        } else if typ.is_integer() {
                            if key_types.iter().any(|typ| typ.is_integer()) {
                                return Some(());
                            }
                        }
                    }
                    LuaMemberKey::Name(_) => {
                        if key_types
                            .iter()
                            .any(|typ| typ.is_string() || typ.is_str_tpl_ref())
                        {
                            return Some(());
                        }
                    }
                    LuaMemberKey::Integer(_) => {
                        if key_types.iter().any(|typ| typ.is_integer()) {
                            return Some(());
                        }
                    }
                    _ => {}
                }
            }
            if members.is_empty() {
                // 当没有任何成员信息且是 enum 类型时, 需要检查参数是否为自己
                if let LuaType::Ref(id) | LuaType::Def(id) = prefix_type {
                    if let Some(decl) = semantic_model.get_db().get_type_index().get_type_decl(&id)
                    {
                        if decl.is_enum() {
                            if key_types.iter().any(|typ| match typ {
                                LuaType::Ref(key_id) | LuaType::Def(key_id) => id == *key_id,
                                _ => false,
                            }) {
                                return Some(());
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn get_prefix_types(prefix_typ: &LuaType) -> HashSet<LuaType> {
    let mut type_set = HashSet::new();
    let mut stack = vec![prefix_typ.clone()];
    let mut visited = HashSet::new();

    while let Some(current_type) = stack.pop() {
        if visited.contains(&current_type) {
            continue;
        }
        visited.insert(current_type.clone());
        match &current_type {
            LuaType::Union(union_typ) => {
                for t in union_typ.get_types() {
                    stack.push(t.clone());
                }
            }
            LuaType::Any | LuaType::Unknown | LuaType::Nil => {}
            _ => {
                type_set.insert(current_type.clone());
            }
        }
    }
    type_set
}

fn get_key_types(typ: &LuaType) -> HashSet<LuaType> {
    let mut type_set = HashSet::new();
    let mut stack = vec![typ.clone()];
    let mut visited = HashSet::new();

    while let Some(current_type) = stack.pop() {
        if visited.contains(&current_type) {
            continue;
        }
        visited.insert(current_type.clone());
        match &current_type {
            LuaType::String => {
                type_set.insert(current_type);
            }
            LuaType::Integer => {
                type_set.insert(current_type);
            }
            LuaType::Union(union_typ) => {
                for t in union_typ.get_types() {
                    stack.push(t.clone());
                }
            }
            LuaType::StrTplRef(_) | LuaType::Ref(_) => {
                type_set.insert(current_type);
            }
            _ => {}
        }
    }
    type_set
}

/// 判断给定的AST节点是否位于判断语句的条件表达式中
///
/// 该函数检查节点是否位于以下语句的条件部分：
/// - if语句的条件表达式
/// - while循环的条件表达式
/// - for循环的迭代表达式
/// - repeat循环的条件表达式
/// - elseif子句的条件表达式
///
/// # 参数
/// * `node` - 要检查的AST节点
///
/// # 返回值
/// * `true` - 节点位于判断语句的条件表达式中
/// * `false` - 节点不在判断语句的条件表达式中
fn in_conditional_statement<T: LuaAstNode>(node: &T) -> bool {
    let node_range = node.get_range();

    // 遍历所有祖先节点，查找条件语句
    for ancestor in node.syntax().ancestors() {
        match ancestor.kind().into() {
            LuaSyntaxKind::IfStat => {
                if let Some(if_stat) = LuaIfStat::cast(ancestor) {
                    if let Some(condition_expr) = if_stat.get_condition_expr() {
                        if condition_expr.get_range().contains_range(node_range) {
                            return true;
                        }
                    }
                }
            }
            LuaSyntaxKind::WhileStat => {
                if let Some(while_stat) = LuaWhileStat::cast(ancestor) {
                    if let Some(condition_expr) = while_stat.get_condition_expr() {
                        if condition_expr.get_range().contains_range(node_range) {
                            return true;
                        }
                    }
                }
            }
            LuaSyntaxKind::ForStat => {
                if let Some(for_stat) = LuaForStat::cast(ancestor) {
                    for iter_expr in for_stat.get_iter_expr() {
                        if iter_expr.get_range().contains_range(node_range) {
                            return true;
                        }
                    }
                }
            }
            LuaSyntaxKind::ForRangeStat => {
                if let Some(for_range_stat) = LuaForRangeStat::cast(ancestor) {
                    for expr in for_range_stat.get_expr_list() {
                        if expr.get_range().contains_range(node_range) {
                            return true;
                        }
                    }
                }
            }
            LuaSyntaxKind::RepeatStat => {
                if let Some(repeat_stat) = LuaRepeatStat::cast(ancestor) {
                    if let Some(condition_expr) = repeat_stat.get_condition_expr() {
                        if condition_expr.get_range().contains_range(node_range) {
                            return true;
                        }
                    }
                }
            }
            LuaSyntaxKind::ElseIfClauseStat => {
                if let Some(elseif_clause) = LuaElseIfClauseStat::cast(ancestor) {
                    if let Some(condition_expr) = elseif_clause.get_condition_expr() {
                        if condition_expr.get_range().contains_range(node_range) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn check_enum_is_param(
    semantic_model: &SemanticModel,
    prefix_typ: &LuaType,
    index_expr: &LuaIndexExpr,
) -> Option<()> {
    enum_variable_is_param(
        semantic_model.get_db(),
        &mut semantic_model.get_config().borrow_mut(),
        index_expr,
        prefix_typ,
    )
}

/// 检查导入的表常量
fn check_import_table_const<'a>(
    semantic_model: &'a SemanticModel,
    index_expr: &LuaIndexExpr,
) -> Option<&'a ModuleInfo> {
    // 获取前缀表达式的语义信息
    let prefix_expr = index_expr.get_prefix_expr()?;
    let semantic_info = semantic_model.get_semantic_info(prefix_expr.syntax().clone().into())?;

    // 检查是否是声明引用
    let decl_id = match semantic_info.semantic_decl? {
        LuaSemanticDeclId::LuaDecl(decl_id) => decl_id,
        _ => return None,
    };

    // 获取声明
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;

    parse_require_module_info(semantic_model, &decl)
}
