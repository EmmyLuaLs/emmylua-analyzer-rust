use std::{collections::HashSet, sync::Arc, vec};

use emmylua_code_analysis::{
    AsyncState, DbIndex, InferGuard, LuaDocReturnInfo, LuaFunctionType, LuaMember, LuaMemberOwner,
    LuaSemanticDeclId, LuaType, RenderLevel, TypeSubstitutor, VariadicType, humanize_type,
    infer_call_expr_func, instantiate_doc_function, try_extract_signature_id_from_field,
};

use crate::handlers::hover::{
    HoverBuilder,
    humanize_types::{
        DescriptionInfo, extract_description_from_property_owner, extract_owner_name_from_element,
        extract_parent_type_from_element, hover_humanize_type,
    },
    infer_prefix_global_name,
};

pub fn build_function_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
) -> Option<()> {
    let (function_name, is_local) = {
        let (semantic_decl, _) = semantic_decls.first()?;
        match semantic_decl {
            LuaSemanticDeclId::LuaDecl(id) => {
                let decl = db.get_decl_index().get_decl(id)?;
                (decl.get_name().to_string(), decl.is_local())
            }
            LuaSemanticDeclId::Member(id) => {
                let member = db.get_member_index().get_member(id)?;
                (member.get_key().to_path(), false)
            }
            _ => {
                return None;
            }
        }
    };

    // 如果是函数调用, 那么我们需要根据上下文实例化出实际类型
    if let Some(call_expr) = builder.get_call_expr() {
        build_function_call_hover(
            builder,
            db,
            semantic_decls,
            &call_expr,
            &function_name,
            is_local,
        );
    } else {
        build_function_define_hover(builder, db, semantic_decls, &function_name, is_local);
    }

    Some(())
}

fn build_function_call_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
    call_expr: &emmylua_parser::LuaCallExpr,
    function_name: &str,
    is_local: bool,
) -> Option<()> {
    let final_type = infer_call_expr_func(
        db,
        &mut builder.semantic_model.get_cache().borrow_mut(),
        call_expr.clone(),
        semantic_decls.last()?.1.clone(),
        &InferGuard::new(),
        None,
    )
    .ok()?;

    // 根据推断出来的类型确定哪个 semantic_decl 是匹配的
    let mut match_semantic_decl = &semantic_decls.last()?.0;
    for (semantic_decl, typ) in semantic_decls.iter() {
        if let LuaType::DocFunction(f) = typ {
            if f == &final_type {
                match_semantic_decl = semantic_decl;
                break;
            }
        }
    }

    let function_member = match match_semantic_decl {
        LuaSemanticDeclId::Member(id) => {
            let member = db.get_member_index().get_member(&id)?;
            Some(member)
        }
        _ => None,
    };

    let is_field = function_member_is_field(db, semantic_decls);
    let contents = process_function_type(
        builder,
        db,
        &LuaType::DocFunction(final_type),
        function_member,
        function_name,
        is_local,
        is_field,
    )?;
    let description = get_function_description(builder, db, &match_semantic_decl);
    builder.set_type_description(contents.first()?.clone());
    builder.add_description_from_info(description);

    Some(())
}

#[derive(Debug, Clone)]
struct HoverFunctionInfo {
    primary: String,
    overloads: Option<Vec<String>>,
    description: Option<DescriptionInfo>,
}

#[allow(unused)]
fn build_function_define_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
    function_name: &str,
    is_local: bool,
) -> Option<()> {
    let is_field = function_member_is_field(db, semantic_decls);
    let mut function_infos = Vec::new();
    for (semantic_decl_id, typ) in semantic_decls {
        let mut typ = typ.clone();
        let function_member = match semantic_decl_id {
            LuaSemanticDeclId::Member(id) => {
                let member = db.get_member_index().get_member(&id)?;
                Some(member)
            }
            _ => None,
        };

        if let Some(substitutor) = &builder.substitutor {
            if let Some(lua_func) = hover_instantiate_function_type(db, &typ, substitutor) {
                typ = LuaType::DocFunction(lua_func);
            }
        }

        let Some(contents) = process_function_type(
            builder,
            db,
            &typ,
            function_member,
            function_name,
            is_local,
            is_field,
        ) else {
            continue;
        };
        if contents.is_empty() {
            continue;
        }
        let description = get_function_description(builder, db, &semantic_decl_id);
        function_infos.push(HoverFunctionInfo {
            primary: contents.first()?.clone(),
            overloads: if contents.len() > 1 {
                Some(contents[1..].to_vec())
            } else {
                None
            },
            description,
        });
    }

    // 去重, 这是必须的
    function_infos.dedup_by_key(|info| info.primary.clone());

    // 需要显示重载的情况
    match function_infos.len() {
        0 => {
            return None;
        }
        1 => {
            builder.set_type_description(function_infos[0].primary.clone());
            builder.add_description_from_info(function_infos[0].description.clone());
        }
        _ => {
            let main_type = function_infos.pop()?;
            builder.set_type_description(main_type.primary.clone());
            builder.add_description_from_info(main_type.description.clone());

            for type_desc in function_infos {
                builder.add_signature_overload(type_desc.primary.clone());
                if let Some(overloads) = &type_desc.overloads {
                    for overload in overloads {
                        builder.add_signature_overload(overload.clone());
                    }
                }
                builder.add_description_from_info(type_desc.description.clone());
            }
        }
    }
    Some(())
}

fn process_function_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: &LuaType,
    function_member: Option<&LuaMember>,
    function_name: &str,
    is_local: bool,
    is_field: bool,
) -> Option<Vec<String>> {
    match typ {
        LuaType::DocFunction(lua_func) => {
            let content = hover_doc_function_type(
                builder,
                db,
                lua_func,
                function_member,
                &function_name,
                is_local,
                is_field,
                convert_function_return_to_docs(lua_func),
            );
            Some(vec![content])
        }
        LuaType::Signature(signature_id) => {
            let signature = db.get_signature_index().get(&signature_id)?;
            let mut new_overloads = signature.overloads.clone();
            let fake_doc_function = Arc::new(LuaFunctionType::new(
                signature.async_state,
                signature.is_colon_define,
                signature.is_vararg,
                signature.get_type_params(),
                signature.get_return_type(),
            ));
            new_overloads.insert(0, fake_doc_function);
            let mut contents = Vec::with_capacity(new_overloads.len());
            for (i, overload) in new_overloads.iter().enumerate() {
                contents.push(hover_doc_function_type(
                    builder,
                    db,
                    overload,
                    function_member,
                    function_name,
                    is_local,
                    is_field,
                    if i == 0 {
                        signature.return_docs.clone()
                    } else {
                        convert_function_return_to_docs(overload)
                    },
                ));
            }
            Some(contents)
        }
        LuaType::Union(union) => {
            let mut contents = Vec::new();
            for typ in union.into_vec() {
                if let Some(content) = process_function_type(
                    builder,
                    db,
                    &typ,
                    function_member,
                    function_name,
                    is_local,
                    is_field,
                ) {
                    contents.extend(content);
                }
            }
            Some(contents)
        }
        _ => None,
    }
}

fn hover_doc_function_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    func: &LuaFunctionType,
    owner_member: Option<&LuaMember>,
    func_name: &str,
    is_local: bool,
    is_field: bool,                     /* 是否为类字段 */
    return_docs: Vec<LuaDocReturnInfo>, /* 返回值以此为准 */
) -> String {
    let async_label = match func.get_async_state() {
        AsyncState::Async => "async ",
        AsyncState::Sync => "sync ",
        _ => "",
    };
    let mut is_method = func.is_colon_define();
    let mut type_label = if is_local && owner_member.is_none() {
        "local function "
    } else {
        "function "
    };

    // 有可能来源于类. 例如: `local add = class.add`, `add()`应被视为类方法
    let full_name = if let Some(owner_member) = owner_member {
        if is_field {
            type_label = "(field) ";
        }

        let member_key = owner_member.get_key().to_path();
        let mut name = String::with_capacity(member_key.len() + 16);

        let mut push_typed_owner_prefix = |prefix: &str, type_decl_id| {
            name.push_str(prefix);
            let owner_ty = LuaType::Ref(type_decl_id);
            is_method = func.is_method(builder.semantic_model, Some(&owner_ty));
            if is_method {
                type_label = "(method) ";
            }
            name.push(if is_method { ':' } else { '.' });
        };

        let parent_owner = db
            .get_member_index()
            .get_current_owner(&owner_member.get_id());
        if let Some(parent_owner) = parent_owner {
            match parent_owner {
                LuaMemberOwner::Type(type_decl_id) => {
                    let prefix = infer_prefix_global_name(builder.semantic_model, owner_member)
                        .unwrap_or_else(|| type_decl_id.get_simple_name());
                    push_typed_owner_prefix(prefix, type_decl_id.clone());
                }
                LuaMemberOwner::Element(element_id) => {
                    if let Some(LuaType::Ref(type_decl_id) | LuaType::Def(type_decl_id)) =
                        extract_parent_type_from_element(builder.semantic_model, element_id)
                    {
                        push_typed_owner_prefix(
                            type_decl_id.get_simple_name(),
                            type_decl_id.clone(),
                        );
                    } else if let Some(owner_name) =
                        extract_owner_name_from_element(builder.semantic_model, element_id)
                    {
                        name.push_str(&owner_name);
                        if is_method {
                            type_label = "(method) ";
                        }
                        name.push(if is_method { ':' } else { '.' });
                    }
                }
                _ => {}
            }
        }

        name.push_str(&member_key);
        name
    } else {
        func_name.to_string()
    };

    let params = func
        .get_params()
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let name = param.0.clone();
            if index == 0 && is_method && !func.is_colon_define() {
                "".to_string()
            } else if let Some(ty) = &param.1 {
                format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
            } else {
                name.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let ret_detail = build_function_returns(builder, return_docs);
    format_function_type(type_label, async_label, full_name, params, ret_detail)
}

fn convert_function_return_to_docs(func: &LuaFunctionType) -> Vec<LuaDocReturnInfo> {
    match func.get_ret() {
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Base(base) => vec![LuaDocReturnInfo {
                name: None,
                type_ref: base.clone(),
                description: None,
                attributes: None,
            }],
            VariadicType::Multi(types) => types
                .iter()
                .map(|ty| LuaDocReturnInfo {
                    name: None,
                    type_ref: ty.clone(),
                    description: None,
                    attributes: None,
                })
                .collect(),
        },
        _ => vec![LuaDocReturnInfo {
            name: None,
            type_ref: func.get_ret().clone(),
            description: None,
            attributes: None,
        }],
    }
}

fn format_function_type(
    type_label: &str,
    async_label: &str,
    full_name: String,
    params: String,
    rets: String,
) -> String {
    let prefix = if type_label.starts_with("function") {
        format!("{}{}", async_label, type_label)
    } else {
        format!("{}{}", type_label, async_label)
    };
    format!("{}{}({}){}", prefix, full_name, params, rets)
}

fn get_function_description(
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

fn build_function_returns(
    builder: &mut HoverBuilder,
    return_docs: Vec<LuaDocReturnInfo>,
) -> String {
    let mut result = String::new();
    // 如果不是补全且存在名称, 我们需要多行显示
    let has_multiline = !builder.is_completion
        && return_docs
            .iter()
            .any(|return_info| return_info.name.is_some());

    for (i, return_info) in return_docs.iter().enumerate() {
        if i == 0 && return_info.type_ref.is_nil() {
            continue;
        }
        let type_text = build_function_return_type(builder, return_info, i);

        if has_multiline {
            // 存在返回值名称时使用多行模式
            let prefix = if i == 0 {
                result.push('\n');
                "-> ".to_string()
            } else {
                format!("{}. ", i + 1)
            };
            let name = return_info.name.clone().unwrap_or_default();

            result.push_str(&format!(
                "  {}{}{}\n",
                prefix,
                if !name.is_empty() {
                    format!("{}: ", name)
                } else {
                    "".to_string()
                },
                type_text,
            ));
        } else {
            // 不存在返回值名称时使用单行模式
            if i == 0 {
                result.push_str(&format!(" -> {}", type_text));
            } else {
                result.push_str(&format!(", {}", type_text));
            }
        }
    }

    result
}

fn build_function_return_type(
    builder: &mut HoverBuilder,
    ret_info: &LuaDocReturnInfo,
    i: usize,
) -> String {
    let type_expansion_count = builder.get_type_expansion_count();
    // 在这个过程中可能会设置`type_expansion`
    let type_text = hover_humanize_type(builder, &ret_info.type_ref, Some(RenderLevel::Simple));
    if builder.get_type_expansion_count() > type_expansion_count {
        // 重新设置`type_expansion`
        if let Some(pop_type_expansion) =
            builder.pop_type_expansion(type_expansion_count, builder.get_type_expansion_count())
        {
            let mut new_type_expansion = format!("return #{}", i + 1);
            let mut seen = HashSet::new();
            for type_expansion in pop_type_expansion {
                for line in type_expansion.lines().skip(1) {
                    if seen.insert(line.to_string()) {
                        new_type_expansion.push('\n');
                        new_type_expansion.push_str(line);
                    }
                }
            }
            builder.add_type_expansion(new_type_expansion);
        }
    };
    type_text
}

// 函数是否为类字段, 任意一个为类字段我们都认为全部为类字段
fn function_member_is_field(db: &DbIndex, semantic_decls: &[(LuaSemanticDeclId, LuaType)]) -> bool {
    semantic_decls.iter().any(|(semantic_decl, _)| {
        if let LuaSemanticDeclId::Member(id) = semantic_decl {
            let member = db.get_member_index().get_member(id);
            member.is_some() && member.unwrap().is_field()
        } else {
            false
        }
    })
}

fn hover_instantiate_function_type(
    db: &DbIndex,
    typ: &LuaType,
    substitutor: &TypeSubstitutor,
) -> Option<Arc<LuaFunctionType>> {
    if !typ.contain_tpl() {
        return None;
    }
    match typ {
        LuaType::DocFunction(f) => {
            if let LuaType::DocFunction(f) = instantiate_doc_function(db, f, substitutor) {
                Some(f)
            } else {
                None
            }
        }
        _ => None,
    }
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
