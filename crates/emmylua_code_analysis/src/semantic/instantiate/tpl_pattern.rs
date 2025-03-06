use std::ops::Deref;

use emmylua_parser::{LuaAstNode, LuaSyntaxId, LuaSyntaxNode, LuaTableExpr};
use smol_str::SmolStr;

use crate::{
    db_index::{DbIndex, LuaGenericType, LuaType},
    semantic::{infer_expr, LuaInferConfig},
    LuaFunctionType, LuaTupleType, LuaUnionType,
};

use super::type_substitutor::TypeSubstitutor;

pub fn tpl_pattern_match(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    root: &LuaSyntaxNode,
    pattern: &LuaType,
    target: &LuaType,
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    match pattern {
        LuaType::TplRef(tpl) => {
            if tpl.get_tpl_id().is_func() {
                substitutor.insert_type(tpl.get_tpl_id(), target.clone());
            }
        }
        LuaType::StrTplRef(str_tpl) => match target {
            LuaType::StringConst(s) => {
                let prefix = str_tpl.get_prefix();
                let type_name = if prefix.is_empty() {
                    s.deref().clone()
                } else {
                    SmolStr::new(format!("{}{}", prefix, s))
                };
                substitutor.insert_type(str_tpl.get_tpl_id(), type_name.into());
            }
            _ => {}
        },
        LuaType::Array(base) => {
            array_tpl_pattern_match(db, config, root, base, target, substitutor);
        }
        LuaType::TableGeneric(table_generic_params) => {
            table_tpl_pattern_match(db, config, root, table_generic_params, target, substitutor);
        }
        LuaType::Nullable(origin) => {
            tpl_pattern_match(db, config, root, &origin, target, substitutor);
        }
        LuaType::Generic(generic) => {
            generic_tpl_pattern_match(db, config, root, generic, target, substitutor);
        }
        LuaType::Union(union) => {
            union_tpl_pattern_match(db, config, root, union, target, substitutor);
        }
        LuaType::DocFunction(doc_func) => {
            func_tpl_pattern_match(db, config, root, doc_func, target, substitutor);
        }
        LuaType::Tuple(tuple) => {
            tuple_tpl_pattern_match(db, config, root, tuple, target, substitutor);
        }
        _ => {}
    }

    Some(())
}

fn array_tpl_pattern_match(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    root: &LuaSyntaxNode,
    base: &LuaType,
    target: &LuaType,
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    match target {
        LuaType::Array(target_base) => {
            tpl_pattern_match(db, config, root, base, target_base, substitutor);
        }
        LuaType::Tuple(target_tuple) => {
            let target_base = target_tuple.cast_down_array_base();
            tpl_pattern_match(db, config, root, base, &target_base, substitutor);
        }
        LuaType::Object(target_object) => {
            let target_base = target_object.cast_down_array_base()?;
            tpl_pattern_match(db, config, root, base, &target_base, substitutor);
        }
        _ => {}
    }

    Some(())
}

fn table_tpl_pattern_match(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    root: &LuaSyntaxNode,
    table_generic_params: &Vec<LuaType>,
    target: &LuaType,
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    if table_generic_params.len() != 2 {
        return None;
    }

    match target {
        LuaType::TableGeneric(target_table_generic_params) => {
            let min_len = table_generic_params
                .len()
                .min(target_table_generic_params.len());
            for i in 0..min_len {
                tpl_pattern_match(
                    db,
                    config,
                    root,
                    &table_generic_params[i],
                    &target_table_generic_params[i],
                    substitutor,
                );
            }
        }
        LuaType::TableConst(target_range) => {
            let node = LuaSyntaxId::to_node_at_range(root, target_range.value)?;
            let table_node = LuaTableExpr::cast(node)?;
            let t1 = &table_generic_params[0];
            let t2 = &table_generic_params[1];
            if table_node.is_array() {
                tpl_pattern_match(db, config, root, &t1, &LuaType::Integer, substitutor);
            } else {
                tpl_pattern_match(db, config, root, &t1, &LuaType::String, substitutor);
            }

            let first_field = table_node.get_fields().next()?;
            let expr_type = infer_expr(db, config, first_field.get_value_expr()?)?;
            tpl_pattern_match(db, config, root, t2, &expr_type, substitutor);
        }
        _ => {}
    }

    Some(())
}

fn generic_tpl_pattern_match(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    root: &LuaSyntaxNode,
    generic: &LuaGenericType,
    target: &LuaType,
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    match target {
        LuaType::Generic(target_generic) => {
            let base = generic.get_base_type();
            let target_base = target_generic.get_base_type();
            if target_base != base {
                return None;
            }

            let params = generic.get_params();
            let target_params = target_generic.get_params();
            let min_len = params.len().min(target_params.len());
            for i in 0..min_len {
                tpl_pattern_match(db, config, root, &params[i], &target_params[i], substitutor);
            }
        }
        _ => {}
    }

    Some(())
}

fn union_tpl_pattern_match(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    root: &LuaSyntaxNode,
    union: &LuaUnionType,
    target: &LuaType,
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    for u in union.get_types() {
        tpl_pattern_match(db, config, root, u, target, substitutor);
    }

    Some(())
}

fn func_tpl_pattern_match(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    root: &LuaSyntaxNode,
    tpl_func: &LuaFunctionType,
    target: &LuaType,
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    match target {
        LuaType::DocFunction(target_doc_func) => {
            func_tpl_pattern_match_doc_func(
                db,
                config,
                root,
                tpl_func,
                target_doc_func,
                substitutor,
            );
        }
        LuaType::Signature(signature_id) => {
            let signature = db.get_signature_index().get(&signature_id)?;
            let typed_params = signature.get_type_params();
            let rets = signature.get_return_types();
            let fake_doc_func =
                LuaFunctionType::new(false, signature.is_colon_define, typed_params, rets);
            func_tpl_pattern_match_doc_func(
                db,
                config,
                root,
                tpl_func,
                &fake_doc_func,
                substitutor,
            );
        }
        _ => {}
    }

    Some(())
}

fn func_tpl_pattern_match_doc_func(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    root: &LuaSyntaxNode,
    tpl_func: &LuaFunctionType,
    target_func: &LuaFunctionType,
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    let tpl_func_params = tpl_func.get_params();
    let target_func_params = target_func.get_params();
    let param_len = tpl_func_params.len();
    for i in 0..param_len {
        let tpl_param_tuple = tpl_func_params.get(i)?;
        let target_param_tuple = match target_func_params.get(i) {
            Some(t) => t,
            None => break,
        };

        let tpl_param_type = tpl_param_tuple.1.clone().unwrap_or(LuaType::Any);

        // T ... match all other params
        if tpl_param_tuple.0 == "..." {
            let target_rest_params = &target_func_params[i..];
            if let LuaType::Variadic(inner) = tpl_param_type {
                func_varargs_tpl_pattern_match(&inner, target_rest_params, substitutor);
            }

            break;
        }

        let target_param_type = target_param_tuple.1.clone().unwrap_or(LuaType::Any);
        tpl_pattern_match(
            db,
            config,
            root,
            &tpl_param_type,
            &target_param_type,
            substitutor,
        );
    }

    let tpl_rets = tpl_func.get_ret();
    let target_rets = target_func.get_ret();
    let ret_len = tpl_rets.len();
    for i in 0..ret_len {
        let tpl_ret_type = &tpl_rets[i];

        if let LuaType::Variadic(inner) = tpl_ret_type {
            let target_rest_rets = &target_rets[i..];
            variadic_tpl_pattern_match(&inner, target_rest_rets, substitutor);
            break;
        }

        let target_ret_type = match target_rets.get(i) {
            Some(t) => t,
            None => return None,
        };
        tpl_pattern_match(db, config, root, tpl_ret_type, target_ret_type, substitutor);
    }

    Some(())
}

fn func_varargs_tpl_pattern_match(
    tpl: &LuaType,
    target_rest_params: &[(String, Option<LuaType>)],
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    if let LuaType::TplRef(tpl_ref) = tpl {
        let tpl_id = tpl_ref.get_tpl_id();
        substitutor.insert_params(
            tpl_id,
            target_rest_params
                .iter()
                .map(|(n, t)| (n.clone(), t.clone()))
                .collect(),
        );
    }

    Some(())
}

pub fn variadic_tpl_pattern_match(
    tpl: &LuaType,
    target_rest_types: &[LuaType],
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    if let LuaType::TplRef(tpl_ref) = tpl {
        let tpl_id = tpl_ref.get_tpl_id();
        substitutor.insert_multi_types(tpl_id, target_rest_types.to_vec());
    }

    Some(())
}

fn tuple_tpl_pattern_match(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    root: &LuaSyntaxNode,
    tpl_tuple: &LuaTupleType,
    target: &LuaType,
    substitutor: &mut TypeSubstitutor,
) -> Option<()> {
    match target {
        LuaType::Tuple(target_tuple) => {
            let tpl_tuple_types = tpl_tuple.get_types();
            let target_tuple_types = target_tuple.get_types();
            let tpl_tuple_len = tpl_tuple_types.len();
            for i in 0..tpl_tuple_len {
                let tpl_type = &tpl_tuple_types[i];

                if let LuaType::Variadic(inner) = tpl_type {
                    let target_rest_types = &target_tuple_types[i..];
                    variadic_tpl_pattern_match(inner, target_rest_types, substitutor);
                    break;
                }

                let target_type = match target_tuple_types.get(i) {
                    Some(t) => t,
                    None => break,
                };

                tpl_pattern_match(db, config, root, tpl_type, target_type, substitutor);
            }
        }
        LuaType::Array(target_array_base) => {
            let tupl_tuple_types = tpl_tuple.get_types();
            let last_type = tupl_tuple_types.last()?;
            if let LuaType::Variadic(inner) = last_type {
                if let LuaType::TplRef(tpl_ref) = inner.deref() {
                    let tpl_id = tpl_ref.get_tpl_id();
                    substitutor.insert_multi_base(tpl_id, target_array_base.deref().clone());
                }
            }
        }
        _ => {}
    }

    Some(())
}
