mod call_hover;
mod define_hover;
mod render;
mod table_field;

use std::sync::Arc;

use emmylua_code_analysis::{
    DbIndex, LuaDocReturnOverloadInfo, LuaFunctionType, LuaMember, LuaSemanticDeclId, LuaSignature,
    LuaType, TypeSubstitutor, VariadicType, infer_call_generic, infer_table_should_be,
    instantiate_type_generic, try_extract_signature_id_from_field,
};
use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaSyntaxToken, LuaTableExpr, LuaTableField};

use crate::handlers::hover::{
    HoverBuilder,
    humanize_types::{DescriptionInfo, extract_description_from_property_owner},
};

use call_hover::build_function_call_hover;
use define_hover::build_function_define_hover;
use table_field::build_table_field_hover;

pub fn build_function_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
) -> Option<()> {
    if let Some(token) = builder.get_trigger_token() {
        if let Some(call_expr) = get_call_expr(&token) {
            return build_function_call_hover(builder, db, semantic_decls, &call_expr);
        }

        if let Some(parent_table_type) = infer_table_field_parent_type(builder, db, &token) {
            return build_table_field_hover(builder, db, semantic_decls, &parent_table_type);
        }
    }

    build_function_define_hover(builder, db, semantic_decls)
}

fn get_call_expr(token: &LuaSyntaxToken) -> Option<LuaCallExpr> {
    let token_start = token.text_range().start();
    let call_expr = token.parent()?.ancestors().find_map(LuaCallExpr::cast)?;
    let prefix_expr = call_expr.get_prefix_expr()?;
    if prefix_expr.syntax().text_range().contains(token_start) {
        Some(call_expr)
    } else {
        None
    }
}

fn get_table_field_expr(token: &LuaSyntaxToken) -> Option<LuaTableExpr> {
    token
        .parent()
        .and_then(LuaTableField::cast)?
        .get_parent::<LuaTableExpr>()
}

fn infer_table_field_parent_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    token: &LuaSyntaxToken,
) -> Option<LuaType> {
    let table_expr = get_table_field_expr(token)?;
    infer_table_should_be(
        db,
        &mut builder.semantic_model.get_cache().borrow_mut(),
        table_expr,
    )
    .ok()
}

/// 从 semantic_decl 中提取 function_member
pub(super) fn extract_function_member<'a>(
    db: &'a DbIndex,
    semantic_decl: &LuaSemanticDeclId,
) -> Option<&'a LuaMember> {
    match semantic_decl {
        LuaSemanticDeclId::Member(id) => db.get_member_index().get_member(id),
        _ => None,
    }
}

pub(super) fn hover_instantiate_function_type(
    db: &DbIndex,
    typ: &LuaType,
    substitutor: &TypeSubstitutor,
) -> Option<Arc<LuaFunctionType>> {
    if !typ.contain_tpl() {
        return None;
    }
    match typ {
        LuaType::DocFunction(_) => {
            if let LuaType::DocFunction(f) = instantiate_type_generic(db, typ, substitutor) {
                Some(f)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn get_function_description(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decl_id: &LuaSemanticDeclId,
) -> Option<DescriptionInfo> {
    let mut description =
        extract_description_from_property_owner(builder.semantic_model, semantic_decl_id);
    match semantic_decl_id {
        LuaSemanticDeclId::Member(id) => {
            let member = db.get_member_index().get_member(id)?;
            // 以 @field 定义的 function 描述信息绑定的 id 并不是 member, 需要特殊处理
            if description.is_none()
                && let Some(signature_id) = try_extract_signature_id_from_field(db, member)
            {
                description = extract_description_from_property_owner(
                    builder.semantic_model,
                    &LuaSemanticDeclId::Signature(signature_id),
                );
            }
            Some(member)
        }
        _ => None,
    };
    description
}

pub(super) fn instantiate_call_return_overloads(
    builder: &HoverBuilder,
    db: &DbIndex,
    call_expr: &LuaCallExpr,
    signature: &LuaSignature,
) -> Vec<LuaDocReturnOverloadInfo> {
    let mut cache = builder.semantic_model.get_cache().borrow_mut();

    signature
        .return_overloads
        .iter()
        .map(|row| {
            let row_return_type = match row.type_refs.len() {
                0 => LuaType::Nil,
                1 => row.type_refs[0].clone(),
                _ => LuaType::Variadic(VariadicType::Multi(row.type_refs.clone()).into()),
            };
            let row_function = LuaFunctionType::new(
                signature.async_state,
                signature.is_colon_define,
                signature.is_vararg,
                signature.get_type_params(),
                row_return_type,
                Some(signature.get_function_generic_params()),
            );
            let instantiated_row =
                infer_call_generic(db, &mut cache, &row_function, call_expr.clone())
                    .ok()
                    .map(|func| match func.get_ret() {
                        LuaType::Variadic(variadic) => match variadic.as_ref() {
                            VariadicType::Multi(types) => types.clone(),
                            VariadicType::Base(_) => vec![LuaType::Variadic(variadic.clone())],
                        },
                        typ => vec![typ.clone()],
                    })
                    .unwrap_or_else(|| row.type_refs.clone());

            LuaDocReturnOverloadInfo {
                type_refs: instantiated_row,
                description: row.description.clone(),
            }
        })
        .collect()
}

pub fn is_function(typ: &LuaType) -> bool {
    typ.is_function()
        || match &typ {
            LuaType::Union(union) => union
                .into_vec()
                .iter()
                .all(|t| matches!(t, LuaType::DocFunction(_) | LuaType::Signature(_))),
            _ => false,
        }
}
