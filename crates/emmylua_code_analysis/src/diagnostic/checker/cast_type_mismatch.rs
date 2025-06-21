use emmylua_parser::{LuaAst, LuaAstNode, LuaDocTagCast};
use itertools::Itertools;
use rowan::TextRange;
use std::collections::HashSet;

use crate::diagnostic::checker::generic::infer_doc_type::infer_doc_type;
use crate::{
    get_real_type, DbIndex, DiagnosticCode, LuaType, LuaUnionType, SemanticModel,
    TypeCheckFailReason, TypeCheckResult,
};

use super::{humanize_lint_type, Checker, DiagnosticContext};

pub struct CastTypeMismatchChecker;

impl Checker for CastTypeMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::CastTypeMismatch];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        // dbg!(&semantic_model.get_root());
        for node in semantic_model.get_root().descendants::<LuaAst>() {
            if let LuaAst::LuaDocTagCast(cast_tag) = node {
                check_cast_tag(context, semantic_model, &cast_tag);
            }
        }
    }
}

fn check_cast_tag(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    cast_tag: &LuaDocTagCast,
) -> Option<()> {
    let key_expr = cast_tag.get_key_expr()?;
    let origin_type = {
        let typ = semantic_model.infer_expr(key_expr).ok()?;
        expand_type(semantic_model.get_db(), &typ).unwrap_or(typ)
    };

    // 检查每个 cast 操作类型
    for op_type in cast_tag.get_op_types() {
        // 如果具有操作符, 则不检查
        if let Some(_) = op_type.get_op() {
            continue;
        }
        if let Some(target_doc_type) = op_type.get_type() {
            let target_type = {
                let typ = infer_doc_type(semantic_model, &target_doc_type);
                expand_type(semantic_model.get_db(), &typ).unwrap_or(typ)
            };
            check_cast_compatibility(
                context,
                semantic_model,
                op_type.get_range(),
                &origin_type,
                &target_type,
            );
        }
    }

    Some(())
}

fn check_cast_compatibility(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    origin_type: &LuaType,
    target_type: &LuaType,
) -> Option<()> {
    if origin_type == target_type {
        return Some(());
    }

    // 检查是否可以从原始类型转换为目标类型
    let result = match origin_type {
        LuaType::Union(union_type) => {
            for member_type in union_type.get_types() {
                // 不检查 nil 类型
                if member_type.is_nil() {
                    continue;
                }
                if cast_type_check(semantic_model, member_type, target_type, 0).is_ok() {
                    return Some(());
                }
            }
            Err(TypeCheckFailReason::TypeNotMatch)
        }
        _ => cast_type_check(semantic_model, origin_type, target_type, 0),
    };

    if !result.is_ok() {
        add_cast_type_mismatch_diagnostic(
            context,
            semantic_model,
            range,
            origin_type,
            target_type,
            result,
        );
    }

    Some(())
}

fn add_cast_type_mismatch_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    origin_type: &LuaType,
    target_type: &LuaType,
    result: TypeCheckResult,
) {
    let db = semantic_model.get_db();
    match result {
        Ok(_) => return,
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason,
                TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::DonotCheck => {
                    "".to_string()
                }
                TypeCheckFailReason::TypeRecursion => t!("type recursion").to_string(),
            };

            context.add_diagnostic(
                DiagnosticCode::CastTypeMismatch,
                range,
                t!(
                    "Cannot cast `%{original}` to `%{target}`. %{reason}",
                    original = humanize_lint_type(db, origin_type),
                    target = humanize_lint_type(db, target_type),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}

fn cast_type_check(
    semantic_model: &SemanticModel,
    origin_type: &LuaType,
    target_type: &LuaType,
    recursion_depth: u32,
) -> TypeCheckResult {
    const MAX_RECURSION_DEPTH: u32 = 100;
    if recursion_depth >= MAX_RECURSION_DEPTH {
        return Err(TypeCheckFailReason::TypeRecursion);
    }

    if origin_type == target_type {
        return Ok(());
    }

    // cast 规则非常宽松
    match (origin_type, target_type) {
        (LuaType::Any | LuaType::Nil, _) => Ok(()),
        (LuaType::Number, LuaType::Integer) => Ok(()),
        (LuaType::Userdata, target_type)
            if target_type.is_table() || target_type.is_custom_type() =>
        {
            Ok(())
        }
        (_, LuaType::Union(union)) => {
            // 通常来说这个的原始类型为 alias / enum-field 的集合
            for member_type in union.get_types() {
                match cast_type_check(
                    semantic_model,
                    origin_type,
                    member_type,
                    recursion_depth + 1,
                ) {
                    Ok(_) => {}
                    Err(reason) => {
                        return Err(reason);
                    }
                }
            }
            return Ok(());
        }
        _ => {
            if origin_type.is_table() {
                if target_type.is_table() || target_type.is_custom_type() {
                    return Ok(());
                }
            } else if origin_type.is_custom_type() {
                if target_type.is_table() {
                    return Ok(());
                }
            } else if origin_type.is_string() {
                if target_type.is_string() {
                    return Ok(());
                }
            } else if origin_type.is_number() {
                if target_type.is_number() {
                    return Ok(());
                }
            }

            semantic_model.type_check(target_type, origin_type)
        }
    }
}

fn expand_type(db: &DbIndex, typ: &LuaType) -> Option<LuaType> {
    let mut visited = HashSet::new();
    expand_type_recursive(db, typ, &mut visited)
}

fn expand_type_recursive(
    db: &DbIndex,
    typ: &LuaType,
    visited: &mut HashSet<LuaType>,
) -> Option<LuaType> {
    // TODO: 优化性能
    // 防止无限递归, 性能很有问题, 但 @cast 使用频率不高, 这是可以接受的
    if visited.contains(typ) {
        return Some(typ.clone());
    }
    visited.insert(typ.clone());

    // 展开类型, 如果具有多种类型将尽量返回 union
    match get_real_type(db, &typ).unwrap_or(&typ) {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let type_decl = db.get_type_index().get_type_decl(id)?;
            if type_decl.is_enum() {
                if let Some(typ) = type_decl.get_enum_field_type(db) {
                    return expand_type_recursive(db, &typ, visited);
                }
            };
        }
        LuaType::Instance(inst) => {
            let base = inst.get_base();
            return Some(base.clone());
        }
        LuaType::MultiLineUnion(multi_union) => {
            let union = multi_union.to_union();
            return expand_type_recursive(db, &union, visited);
        }
        LuaType::Union(union_type) => {
            // 递归展开 union 中的每个类型
            let mut expanded_types = Vec::new();
            for inner_type in union_type.get_types() {
                if let Some(expanded) = expand_type_recursive(db, inner_type, visited) {
                    match expanded {
                        LuaType::Union(inner_union) => {
                            // 如果展开后还是 union，则将其成员类型添加到结果中
                            expanded_types.extend(inner_union.get_types().iter().cloned());
                        }
                        _ => {
                            expanded_types.push(expanded);
                        }
                    }
                } else {
                    expanded_types.push(inner_type.clone());
                }
            }

            let expanded_types = expanded_types.into_iter().unique().collect::<Vec<_>>();
            return match expanded_types.len() {
                0 => Some(LuaType::Unknown),
                1 => Some(expanded_types[0].clone().into()),
                _ => Some(LuaType::Union(LuaUnionType::new(expanded_types).into())),
            };
        }
        LuaType::TypeGuard(_) => return Some(LuaType::Boolean),
        _ => {}
    }
    Some(typ.clone())
}
