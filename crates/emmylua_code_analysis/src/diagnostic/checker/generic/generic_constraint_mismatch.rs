//! Check generic constraint mismatch — pure salsa with GenericChecker.

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaComment, LuaDocTagAlias, LuaDocTagClass,
    LuaDocTagGeneric, LuaDocTagType, LuaDocType,
};
use rowan::TextRange;

use crate::diagnostic::checker::DiagnosticContext;
use crate::semantic_model::humanize::humanize_type_salsa;
use crate::semantic_model::offset_types::OwnerPosition;
use crate::semantic_model::{
    InferQuery, SemanticModel, SigQuery,
    generic_checker::{ConstraintViolation, GenericChecker},
};
use crate::{
    DiagnosticCode, LuaSignatureId, LuaType, RenderLevel, SalsaMemberRootSummary,
    SalsaMemberTargetSummary,
};

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    let sig_query = model.sigs();
    let infer = model.infer();

    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaCallExpr(call_expr) => {
                check_call(context, model, &call_expr, &sig_query, &infer);
            }
            LuaAst::LuaDocTagClass(tag) => {
                check_class(context, model, tag, &sig_query);
            }
            LuaAst::LuaDocTagAlias(tag) => {
                check_alias(context, model, tag, &sig_query);
            }
            LuaAst::LuaDocTagGeneric(tag) => {
                check_generic_func(context, model, tag, &sig_query);
            }
            LuaAst::LuaDocTagType(tag) => {
                check_type_use(context, model, tag, &sig_query);
            }
            _ => {}
        }
    }
}

fn check_call(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    call_expr: &LuaCallExpr,
    sig_query: &SigQuery,
    infer: &InferQuery,
) {
    let resolve_name = |name: &str| model.resolve_doc_type_name(name);

    // 路径 A: GenericChecker（总是运行）
    if let Some(checker) = GenericChecker::from_call(call_expr, sig_query, infer, &resolve_name) {
        checker.check_actuals(model, |v| {
            add_diagnostic(context, call_expr.syntax().text_range(), v);
        });
    }

    // 路径 B: 补充检查 — 处理两种调用：
    //   B1: 方法调用 → callee_member → class generics
    //   B2: 普通函数调用 → resolved_signature generics
    let call = model.get_call_explain(call_expr.get_position());
    let Some(call) = call else { return };

    // 获取泛型约束（class generics 或 signature generics）
    let constraints: Vec<(Option<LuaType>, Option<LuaType>)> =
        if let Some(lexical) = &call.lexical_call {
            if let Some(callee) = &lexical.callee_member {
                // B1: 方法调用 → class 泛型
                if let Some(class_name) =
                    get_class_name_from_member(callee.as_summary(), model, sig_query)
                {
                    if let Some(td) = model.types().get_def(&class_name)
                        && !td.generic_params.is_empty()
                    {
                        sig_query.type_generic_constraints(&td, model.get_file_id())
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            } else {
                return;
            }
        } else if let Some(sig) = &call.resolved_signature {
            // B2: 普通函数调用 → signature generics
            if sig.generics.is_empty() {
                return;
            }
            sig_query.signature_generic_constraints(sig)
        } else {
            return;
        };

    let Some(args) = call_expr.get_args_list() else {
        return;
    };

    for (i, arg) in args.get_args().enumerate() {
        if let Some(type_name) = extract_string_literal(&arg) {
            // 字符串参数 → 尝试解析为类型名
            let resolved =
                resolve_builtin(&type_name).or_else(|| model.resolve_doc_type_name(&type_name));
            match resolved {
                Some(actual) => {
                    // 有约束 → 检查约束
                    if let Some((Some(constraint), _)) = constraints.get(i) {
                        if model.type_check_detail(constraint, &actual).is_err() {
                            add_diagnostic(
                                context,
                                arg.syntax().text_range(),
                                &ConstraintViolation {
                                    param_name: "T".into(),
                                    constraint: constraint.clone(),
                                    actual,
                                },
                            );
                        }
                    }
                }
                None => {
                    // 字符串无法解析为类型 → 总是违规
                    add_diagnostic(
                        context,
                        arg.syntax().text_range(),
                        &ConstraintViolation {
                            param_name: "T".into(),
                            constraint: LuaType::Any,
                            actual: LuaType::String,
                        },
                    );
                }
            }
        } else {
            let arg_type = infer.infer_expr(arg.clone()).ok();
            if let Some((Some(constraint), _)) = constraints.get(i)
                && let Some(arg_type) = arg_type
            {
                if model.type_check_detail(constraint, &arg_type).is_err() {
                    add_diagnostic(
                        context,
                        arg.syntax().text_range(),
                        &ConstraintViolation {
                            param_name: "T".into(),
                            constraint: constraint.clone(),
                            actual: arg_type,
                        },
                    );
                }
            }
        }
    }
}

fn extract_string_literal(expr: &emmylua_parser::LuaExpr) -> Option<String> {
    use emmylua_parser::LuaLiteralToken;
    match expr {
        emmylua_parser::LuaExpr::LiteralExpr(lit) => match lit.get_literal()? {
            LuaLiteralToken::String(s) => Some(s.get_value()),
            _ => None,
        },
        _ => None,
    }
}

/// 从 member target 追溯 class 名。
fn get_class_name_from_member(
    target: &SalsaMemberTargetSummary,
    model: &SemanticModel,
    sig_query: &SigQuery,
) -> Option<String> {
    match &target.root {
        SalsaMemberRootSummary::LocalDecl { name, decl_id } => {
            let db = sig_query.db();
            if let Some(name_info) = db.types().name(model.get_file_id(), decl_id.as_text_size()) {
                if let Some(dt) = &name_info.decl_type {
                    if !dt.named_type_names.is_empty() {
                        return Some(dt.named_type_names[0].to_string());
                    }
                }
            }
            // 回退：通过 type_defs（@class/@alias）查找 class 名
            // type_tags 只含 @type，不含 @class；@class 存在 type_defs 中
            let dp = decl_id.0;
            // 遍历当前文件所有 type_def，匹配 owner 位置
            if let Some(summary) = db.doc().summary(model.get_file_id()) {
                for type_def in &summary.type_defs {
                    if let Some(def_owner_offset) = type_def.owner.syntax_offset {
                        let op = OwnerPosition(def_owner_offset);
                        let dist = u32::from(dp.0) as i64 - u32::from(op.0) as i64;
                        if (0..100).contains(&dist) {
                            return Some(type_def.name.to_string());
                        }
                    }
                }
            }
            name.to_string().into()
        }
        _ => None,
    }
}

fn resolve_builtin(name: &str) -> Option<LuaType> {
    match name {
        "nil" => Some(LuaType::Nil),
        "any" | "unknown" => Some(LuaType::Any),
        "boolean" | "bool" => Some(LuaType::Boolean),
        "string" => Some(LuaType::String),
        "number" => Some(LuaType::Number),
        "integer" | "int" => Some(LuaType::Integer),
        "function" => Some(LuaType::Function),
        "table" => Some(LuaType::Table),
        "thread" => Some(LuaType::Thread),
        "userdata" => Some(LuaType::Userdata),
        _ => None,
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 2. @class 声明
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_class(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    tag: LuaDocTagClass,
    sig_query: &SigQuery,
) {
    let generic_decl_list = match tag.get_generic_decl() {
        Some(g) => g,
        None => return,
    };
    let name = match tag.get_name_token().map(|t| t.get_name_text().to_string()) {
        Some(n) => n,
        None => return,
    };
    let (type_def, def_file_id) = match model.types().get_def_with_file(&name) {
        Some(t) => t,
        None => return,
    };
    let checker = GenericChecker::from_type_def(&type_def, def_file_id, sig_query);
    checker.check_defaults(model, |v| {
        let idx = checker.index_of(&v.param_name).unwrap_or(0);
        if let Some(decl) = generic_decl_list.get_generic_decl().nth(idx) {
            if let Some(dt) = decl.get_default_type() {
                add_diagnostic(context, dt.get_range(), v);
            }
        }
    });
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 3. @alias 声明
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_alias(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    tag: LuaDocTagAlias,
    sig_query: &SigQuery,
) {
    let generic_decl_list = match tag.get_generic_decl_list() {
        Some(g) => g,
        None => return,
    };
    let name = match tag.get_name_token().map(|t| t.get_name_text().to_string()) {
        Some(n) => n,
        None => return,
    };
    let (type_def, def_file_id) = match model.types().get_def_with_file(&name) {
        Some(t) => t,
        None => return,
    };
    let checker = GenericChecker::from_type_def(&type_def, def_file_id, sig_query);
    checker.check_defaults(model, |v| {
        let idx = checker.index_of(&v.param_name).unwrap_or(0);
        if let Some(decl) = generic_decl_list.get_generic_decl().nth(idx) {
            if let Some(dt) = decl.get_default_type() {
                add_diagnostic(context, dt.get_range(), v);
            }
        }
    });
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 4. @generic 函数声明
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_generic_func(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    tag: LuaDocTagGeneric,
    sig_query: &SigQuery,
) {
    let generic_decl_list = match tag.get_generic_decl_list() {
        Some(g) => g,
        None => return,
    };
    let closure = match find_owner_closure(&tag) {
        Some(c) => c,
        None => return,
    };
    let sig_id = LuaSignatureId::from_closure(model.get_file_id(), &closure);
    let explain = match sig_query.explain(model.get_file_id(), sig_id.get_position()) {
        Some(e) => e,
        None => return,
    };
    let checker = GenericChecker::from_signature(&explain);
    checker.check_defaults(model, |v| {
        let idx = checker.index_of(&v.param_name).unwrap_or(0);
        if let Some(decl) = generic_decl_list.get_generic_decl().nth(idx) {
            if let Some(dt) = decl.get_default_type() {
                add_diagnostic(context, dt.get_range(), v);
            }
        }
    });
}

fn find_owner_closure(tag: &LuaDocTagGeneric) -> Option<LuaClosureExpr> {
    let comment = tag.get_parent::<LuaComment>()?;
    match comment.get_owner()? {
        LuaAst::LuaFuncStat(func) => func.get_closure(),
        LuaAst::LuaLocalFuncStat(local) => local.get_closure(),
        owner => owner.descendants::<LuaClosureExpr>().next(),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 5. @type 使用
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn check_type_use(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    tag: LuaDocTagType,
    sig_query: &SigQuery,
) {
    for doc_type in tag.get_type_list() {
        let explicit_args = match &doc_type {
            LuaDocType::Generic(g) => g
                .get_generic_types()
                .map(|tl| tl.get_types().collect::<Vec<_>>())
                .unwrap_or_default(),
            _ => continue,
        };
        if explicit_args.is_empty() {
            continue;
        }

        let type_name = match &doc_type {
            LuaDocType::Generic(g) => {
                let nt = match g.get_name_type() {
                    Some(n) => n,
                    None => continue,
                };
                match nt.get_name_text() {
                    Some(t) => t,
                    None => continue,
                }
            }
            LuaDocType::Name(n) => match n.get_name_text() {
                Some(t) => t,
                None => continue,
            },
            _ => continue,
        };
        let (type_def, def_file_id) = match model.types().get_def_with_file(&type_name) {
            Some(t) => t,
            None => continue,
        };
        let _checker = GenericChecker::from_type_def(&type_def, def_file_id, sig_query);

        // 把显式泛型参数填入 checker 的 actual，然后检查
        // from_type_def 创建的 generic_checker 没有 actual，我们手动填
        // 这里直接用 type_check_detail 做检查更简单
        let constraints = sig_query.type_generic_constraints(&type_def, def_file_id);
        for (i, arg_doc_type) in explicit_args.iter().enumerate() {
            if let Some((Some(constraint), _)) = constraints.get(i) {
                if let Some(arg_type) = resolve_doc_type_arg(model, arg_doc_type) {
                    if model.type_check_detail(constraint, &arg_type).is_err() {
                        add_diagnostic(
                            context,
                            arg_doc_type.get_range(),
                            &ConstraintViolation {
                                param_name: format!("param_{}", i),
                                constraint: constraint.clone(),
                                actual: arg_type,
                            },
                        );
                    }
                }
            }
        }
    }
}

fn resolve_doc_type_arg(model: &SemanticModel, dt: &LuaDocType) -> Option<LuaType> {
    match dt {
        LuaDocType::Name(n) => {
            let name = n.get_name_text()?;
            match name.as_str() {
                "nil" => Some(LuaType::Nil),
                "any" => Some(LuaType::Any),
                "boolean" | "bool" => Some(LuaType::Boolean),
                "string" => Some(LuaType::String),
                "number" => Some(LuaType::Number),
                "integer" | "int" => Some(LuaType::Integer),
                "function" => Some(LuaType::Function),
                "table" => Some(LuaType::Table),
                "thread" => Some(LuaType::Thread),
                "userdata" => Some(LuaType::Userdata),
                _ => model.resolve_doc_type_name(&name),
            }
        }
        LuaDocType::Literal(lit) => match lit.get_literal()? {
            emmylua_parser::LuaLiteralToken::Bool(b) => Some(LuaType::BooleanConst(b.is_true())),
            emmylua_parser::LuaLiteralToken::Nil(_) => Some(LuaType::Nil),
            emmylua_parser::LuaLiteralToken::Number(n) => match n.get_number_value() {
                emmylua_parser::NumberResult::Int(i) => Some(LuaType::IntegerConst(i)),
                emmylua_parser::NumberResult::Float(f) => Some(LuaType::FloatConst(f)),
                _ => Some(LuaType::Number),
            },
            emmylua_parser::LuaLiteralToken::String(s) => Some(LuaType::DocStringConst(
                smol_str::SmolStr::new(s.get_value()).into(),
            )),
            _ => Some(LuaType::Any),
        },
        _ => None,
    }
}

fn add_diagnostic(context: &mut DiagnosticContext, range: TextRange, v: &ConstraintViolation) {
    context.add_diagnostic(
        DiagnosticCode::GenericConstraintMismatch,
        range,
        t!(
            "type `%{found}` does not satisfy the constraint `%{source}`",
            source = humanize_type_salsa(
                context.get_salsa_db(),
                context.get_file_id(),
                &v.constraint,
                RenderLevel::Simple
            ),
            found = humanize_type_salsa(
                context.get_salsa_db(),
                context.get_file_id(),
                &v.actual,
                RenderLevel::Simple
            ),
        )
        .to_string(),
        None,
    );
}
