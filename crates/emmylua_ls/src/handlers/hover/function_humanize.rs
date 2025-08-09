use std::collections::HashSet;

use emmylua_code_analysis::{
    DbIndex, LuaDocReturnInfo, LuaFunctionType, LuaMember, LuaMemberKey, LuaMemberOwner,
    LuaSemanticDeclId, LuaSignature, LuaSignatureId, LuaType, RenderLevel, humanize_type,
    try_extract_signature_id_from_field,
};

use crate::handlers::{
    definition::extract_semantic_decl_from_signature,
    hover::{
        HoverBuilder,
        hover_humanize::{
            DescriptionInfo, extract_description_from_property_owner,
            extract_owner_name_from_element, hover_humanize_type,
        },
        infer_prefix_global_name,
    },
};

#[derive(Debug, Clone)]
struct HoverFunctionInfo {
    type_description: String,
    overloads: Option<Vec<String>>,
    description: Option<DescriptionInfo>,
    is_call_function: bool,
}

pub fn hover_function_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
    verbose: bool,
) -> Option<()> {
    let (name, is_local) = {
        let Some((semantic_decl, _)) = semantic_decls.first() else {
            return None;
        };
        match semantic_decl {
            LuaSemanticDeclId::LuaDecl(id) => {
                let decl = db.get_decl_index().get_decl(&id)?;
                (decl.get_name().to_string(), decl.is_local())
            }
            LuaSemanticDeclId::Member(id) => {
                let member = db.get_member_index().get_member(&id)?;
                (member.get_key().to_path(), false)
            }
            _ => {
                return None;
            }
        }
    };

    let call_function = builder.get_call_function();
    // 已处理过的 semantic_decl_id, 用于解决`test_issue_499_3`
    let mut handled_semantic_decl_ids = HashSet::new();
    let mut type_descs: Vec<HoverFunctionInfo> = Vec::with_capacity(semantic_decls.len());
    // 记录已处理过的类型, 用于在 Union 中跳过重复类型.
    // 这是为了解决最后一个类型可能是前面所有类型的联合类型的情况
    let mut processed_types = HashSet::new();

    for (semantic_decl_id, typ) in semantic_decls {
        let is_new = handled_semantic_decl_ids.insert(semantic_decl_id);
        let mut function_info = HoverFunctionInfo {
            type_description: String::new(),
            overloads: None,
            description: if is_new {
                extract_description_from_property_owner(&builder.semantic_model, semantic_decl_id)
            } else {
                None
            },
            is_call_function: false,
        };

        let function_member = match semantic_decl_id {
            LuaSemanticDeclId::Member(id) => {
                let member = db.get_member_index().get_member(&id)?;
                // 以 @field 定义的 function 描述信息绑定的 id 并不是 member, 需要特殊处理
                if is_new && function_info.description.is_none() {
                    if let Some(signature_id) = try_extract_signature_id_from_field(db, &member) {
                        function_info.description = extract_description_from_property_owner(
                            &builder.semantic_model,
                            &LuaSemanticDeclId::Signature(signature_id),
                        );
                    }
                }
                Some(member)
            }
            _ => None,
        };

        // 如果函数定义来自于其他文件, 我们需要添加原始的注释信息. 参考`test_other_file_function`
        if let LuaType::Signature(signature_id) = typ {
            if let Some(semantic_id) =
                extract_semantic_decl_from_signature(builder.compilation, &signature_id)
            {
                if semantic_id != *semantic_decl_id {
                    // signature 的原始定义的描述信息
                    if let Some(origin_description) = extract_description_from_property_owner(
                        &builder.semantic_model,
                        &semantic_id,
                    ) {
                        match &mut function_info.description {
                            Some(current_description) => {
                                // 如果描述不为空, 则合并描述
                                if let Some(description) = origin_description.description {
                                    if current_description.description.is_none() {
                                        current_description.description = Some(description);
                                    } else {
                                        current_description.description = Some(format!(
                                            "{}\n{}",
                                            current_description.description.take()?,
                                            description
                                        ));
                                    }
                                }
                            }
                            None => {
                                function_info.description = Some(origin_description);
                            }
                        }
                    }
                }
            }
        }

        // 如果当前类型是 Union, 传入已处理的类型集合
        let result = match typ {
            LuaType::Union(_) => process_single_function_type_with_exclusions(
                builder,
                db,
                typ,
                function_member,
                &name,
                is_local,
                call_function.as_ref(),
                &processed_types,
                verbose,
            ),
            _ => {
                // 记录非 Union 类型
                processed_types.insert(typ.clone());
                process_single_function_type(
                    builder,
                    db,
                    typ,
                    function_member,
                    &name,
                    is_local,
                    call_function.as_ref(),
                    verbose,
                )
            }
        };

        match result {
            ProcessFunctionTypeResult::Single(mut info) => {
                // 合并描述信息
                if function_info.description.is_some() && info.description.is_none() {
                    info.description = function_info.description;
                }
                function_info = info;
            }
            ProcessFunctionTypeResult::Multiple(infos) => {
                // 对于 Union 类型, 将每个子类型的结果都添加到 type_descs 中
                let infos_len = infos.len();
                for (index, mut info) in infos.into_iter().enumerate() {
                    // 合并描述信息, 只有最后一个才设置描述
                    if function_info.description.is_some()
                        && info.description.is_none()
                        && index == infos_len - 1
                    {
                        info.description = function_info.description.clone();
                    }
                    if info.is_call_function {
                        type_descs.clear();
                        type_descs.push(info);
                        break;
                    } else {
                        type_descs.push(info);
                    }
                }
                continue;
            }
            ProcessFunctionTypeResult::Skip => {
                continue;
            }
        }

        if function_info.is_call_function {
            type_descs.clear();
            type_descs.push(function_info);
            break;
        } else {
            type_descs.push(function_info);
        }
    }

    // 此时是函数调用且具有完全匹配的签名, 那么只需要显示对应的签名, 不需要显示重载
    if let Some(info) = type_descs.first() {
        if info.is_call_function {
            builder.signature_overload = None;
            builder.set_type_description(info.type_description.clone());

            builder.add_description_from_info(info.description.clone());
            return Some(());
        }
    }

    // 去重
    type_descs.dedup_by_key(|info| info.type_description.clone());

    // 需要显示重载的情况
    match type_descs.len() {
        0 => {
            return None;
        }
        1 => {
            builder.set_type_description(type_descs[0].type_description.clone());
            builder.add_description_from_info(type_descs[0].description.clone());
        }
        _ => {
            // 将最后一个作为 type_description
            let main_type = type_descs.pop()?;
            builder.set_type_description(main_type.type_description.clone());
            builder.add_description_from_info(main_type.description.clone());

            for type_desc in type_descs {
                builder.add_signature_overload(type_desc.type_description);
                if let Some(overloads) = type_desc.overloads {
                    for overload in overloads {
                        builder.add_signature_overload(overload);
                    }
                }
                builder.add_description_from_info(type_desc.description);
            }
        }
    }

    Some(())
}

fn hover_doc_function_type(
    builder: &HoverBuilder,
    db: &DbIndex,
    lua_func: &LuaFunctionType,
    owner_member: Option<&LuaMember>,
    func_name: &str,
    verbose: bool,
) -> String {
    let render_level = if verbose {
        RenderLevel::Simple
    } else {
        RenderLevel::Normal
    };

    let async_label = if lua_func.is_async() { "async " } else { "" };
    let mut is_method = lua_func.is_colon_define();
    let mut type_label = "function ";
    // 有可能来源于类. 例如: `local add = class.add`, `add()`应被视为类方法
    let full_name = if let Some(owner_member) = owner_member {
        let global_name = infer_prefix_global_name(builder.semantic_model, owner_member);
        let mut name = String::new();
        let parent_owner = db
            .get_member_index()
            .get_current_owner(&owner_member.get_id());
        if let Some(parent_owner) = parent_owner {
            match parent_owner {
                LuaMemberOwner::Type(type_decl_id) => {
                    // 如果是全局定义, 则使用定义时的名称
                    if let Some(global_name) = global_name {
                        name.push_str(global_name);
                    } else {
                        name.push_str(type_decl_id.get_simple_name());
                    }
                    if owner_member.is_field() {
                        type_label = "(field) ";
                    }
                    is_method = lua_func.is_method(
                        builder.semantic_model,
                        Some(&LuaType::Ref(type_decl_id.clone())),
                    );
                }
                LuaMemberOwner::Element(element_id) => {
                    if let Some(owner_name) =
                        extract_owner_name_from_element(builder.semantic_model, element_id)
                    {
                        name.push_str(&owner_name);
                    }
                }
                _ => {}
            }
        }

        if is_method {
            type_label = "(method) ";
            name.push_str(":");
        } else {
            name.push_str(".");
        }
        if let LuaMemberKey::Name(n) = owner_member.get_key() {
            name.push_str(n.as_str());
        }
        name
    } else {
        func_name.to_string()
    };
    let params = lua_func
        .get_params()
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let name = param.0.clone();
            if index == 0 && is_method && !lua_func.is_colon_define() {
                "".to_string()
            } else if let Some(ty) = &param.1 {
                format!("{}: {}", name, humanize_type(db, ty, render_level))
            } else {
                name.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let ret_detail = {
        let ret_type = lua_func.get_ret();
        match ret_type {
            LuaType::Nil => "".to_string(),
            _ => {
                format!(" -> {}", humanize_type(db, ret_type, render_level))
            }
        }
    };
    format_function_type(type_label, async_label, full_name, params, ret_detail)
}

struct HoverSignatureResult {
    type_description: String,
    overloads: Option<Vec<String>>,
    call_function: Option<LuaFunctionType>,
}

fn hover_signature_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    signature_id: LuaSignatureId,
    owner_member: Option<&LuaMember>,
    func_name: &str,
    is_local: bool,
    call_function: Option<&LuaFunctionType>,
    verbose: bool,
) -> Option<HoverSignatureResult> {
    let render_level = if verbose {
        RenderLevel::Simple
    } else {
        RenderLevel::Normal
    };

    let signature = db.get_signature_index().get(&signature_id)?;

    let mut is_method = signature.is_colon_define;
    let mut self_real_type = LuaType::SelfInfer;
    let mut type_label = "function ";
    // 有可能来源于类. 例如: `local add = class.add`, `add()`应被视为类定义的内容
    let full_name = if let Some(owner_member) = owner_member {
        let global_name = infer_prefix_global_name(builder.semantic_model, owner_member);
        let mut name = String::new();
        let parent_owner = db
            .get_member_index()
            .get_current_owner(&owner_member.get_id());
        match parent_owner {
            Some(LuaMemberOwner::Type(type_decl_id)) => {
                self_real_type = LuaType::Ref(type_decl_id.clone());
                // 如果是全局定义, 则使用定义时的名称
                if let Some(global_name) = global_name {
                    name.push_str(global_name);
                } else {
                    name.push_str(type_decl_id.get_simple_name());
                }
                if owner_member.is_field() {
                    type_label = "(field) ";
                }
                // `field`定义的function也被视为`signature`, 因此这里需要额外处理
                is_method = signature.is_method(builder.semantic_model, Some(&self_real_type));
                if is_method {
                    type_label = "(method) ";
                    name.push_str(":");
                } else {
                    name.push_str(".");
                }
            }
            Some(LuaMemberOwner::Element(element_id)) => {
                if let Some(owner_name) =
                    extract_owner_name_from_element(builder.semantic_model, element_id)
                {
                    name.push_str(&owner_name);
                    name.push_str(".");
                }
            }
            _ => {}
        }
        if let LuaMemberKey::Name(n) = owner_member.get_key() {
            name.push_str(n.as_str());
        }
        name
    } else {
        if is_local {
            type_label = "local function ";
        }
        func_name.to_string()
    };

    // 构建 signature
    let signature_info: String = {
        let async_label = db
            .get_signature_index()
            .get(&signature_id)
            .map(|signature| if signature.is_async { "async " } else { "" })
            .unwrap_or("");
        let params = signature
            .get_type_params()
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let name = param.0.clone();
                if index == 0 && !signature.is_colon_define && is_method {
                    "".to_string()
                } else if let Some(ty) = &param.1 {
                    format!("{}: {}", name, humanize_type(db, ty, render_level))
                } else {
                    name
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        let rets = build_signature_rets(builder, signature, builder.is_completion, None, verbose);
        let result = format_function_type(type_label, async_label, full_name.clone(), params, rets);
        // 由于 @field 定义的`docfunction`会被视为`signature`, 因此这里额外处理
        if let Some(call_function) = call_function {
            if call_function.get_params() == signature.get_type_params() {
                // 如果具有完全匹配的签名, 那么将其设置为当前签名, 且不显示重载
                return Some(HoverSignatureResult {
                    type_description: result,
                    overloads: None,
                    call_function: Some(call_function.clone()),
                });
            }
        }
        result
    };
    // 构建所有重载
    let overloads: Vec<String> = {
        let mut overloads = Vec::new();
        for (_, overload) in signature.overloads.iter().enumerate() {
            let async_label = if overload.is_async() { "async " } else { "" };
            let params = overload
                .get_params()
                .iter()
                .enumerate()
                .map(|(index, param)| {
                    let name = param.0.clone();
                    if index == 0
                        && param.1.is_some()
                        && overload.is_method(builder.semantic_model, Some(&self_real_type))
                    {
                        "".to_string()
                    } else if let Some(ty) = &param.1 {
                        format!("{}: {}", name, humanize_type(db, ty, render_level))
                    } else {
                        name
                    }
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            let rets = build_signature_rets(
                builder,
                signature,
                builder.is_completion,
                Some(overload),
                verbose,
            );
            let result =
                format_function_type(type_label, async_label, full_name.clone(), params, rets);

            if let Some(call_function) = call_function {
                if *call_function == **overload {
                    // 如果具有完全匹配的签名, 那么将其设置为当前签名, 且不显示重载
                    return Some(HoverSignatureResult {
                        type_description: result,
                        overloads: None,
                        call_function: Some(call_function.clone()),
                    });
                }
            };
            overloads.push(result);
        }
        overloads
    };

    Some(HoverSignatureResult {
        type_description: signature_info,
        overloads: Some(overloads),
        call_function: None,
    })
}

fn build_signature_rets(
    builder: &mut HoverBuilder,
    signature: &LuaSignature,
    is_completion: bool,
    overload: Option<&LuaFunctionType>,
    verbose: bool,
) -> String {
    let render_level = if verbose {
        RenderLevel::Simple
    } else {
        RenderLevel::Normal
    };

    let db = builder.semantic_model.get_db();
    let mut result = String::new();
    // overload 的返回值固定为单行
    let overload_rets_string = if let Some(overload) = overload {
        let ret_type = overload.get_ret();
        match ret_type {
            LuaType::Nil => "".to_string(),
            _ => {
                format!(" -> {}", humanize_type(db, ret_type, render_level))
            }
        }
    } else {
        "".to_string()
    };

    if is_completion {
        let rets = if !overload_rets_string.is_empty() {
            overload_rets_string
        } else {
            let rets = &signature.return_docs;
            if rets.is_empty() || signature.get_return_type().is_nil() {
                "".to_string()
            } else {
                format!(
                    " -> {}",
                    rets.iter()
                        .enumerate()
                        .map(|(i, ret)| build_signature_ret_type(builder, ret, i, verbose))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        };
        result.push_str(rets.as_str());
        return result;
    }

    let rets = if !overload_rets_string.is_empty() {
        overload_rets_string
    } else {
        let rets = &signature.return_docs;
        if rets.is_empty() || signature.get_return_type().is_nil() {
            "".to_string()
        } else {
            let mut rets_string_multiline = String::new();
            rets_string_multiline.push_str("\n");

            for (i, ret) in rets.iter().enumerate() {
                let type_text = build_signature_ret_type(builder, ret, i, verbose);
                let prefix = if i == 0 {
                    "-> ".to_string()
                } else {
                    format!("{}. ", i + 1)
                };
                let name = ret.name.clone().unwrap_or_default();

                rets_string_multiline.push_str(&format!(
                    "  {}{}{}\n",
                    prefix,
                    if !name.is_empty() {
                        format!("{}: ", name)
                    } else {
                        "".to_string()
                    },
                    type_text,
                ));
            }
            rets_string_multiline
        }
    };
    result.push_str(rets.as_str());
    result
}

fn build_signature_ret_type(
    builder: &mut HoverBuilder,
    ret_info: &LuaDocReturnInfo,
    i: usize,
    verbose: bool,
) -> String {
    let render_level = if verbose {
        RenderLevel::Simple
    } else {
        RenderLevel::Normal
    };

    let type_expansion_count = builder.get_type_expansion_count();
    let type_text = hover_humanize_type(builder, &ret_info.type_ref, verbose, render_level);
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

#[derive(Debug, Clone)]
enum ProcessFunctionTypeResult {
    Single(HoverFunctionInfo),
    Multiple(Vec<HoverFunctionInfo>),
    Skip,
}

fn process_single_function_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: &LuaType,
    function_member: Option<&LuaMember>,
    name: &str,
    is_local: bool,
    call_function: Option<&LuaFunctionType>,
    verbose: bool,
) -> ProcessFunctionTypeResult {
    match typ {
        LuaType::Function => ProcessFunctionTypeResult::Single(HoverFunctionInfo {
            type_description: format!("function {}()", name),
            overloads: None,
            description: None,
            is_call_function: false,
        }),
        LuaType::DocFunction(lua_func) => {
            let type_description =
                hover_doc_function_type(builder, db, &lua_func, function_member, &name, verbose);
            let is_call_function = if let Some(call_function) = call_function {
                call_function.get_params() == lua_func.get_params()
            } else {
                false
            };

            ProcessFunctionTypeResult::Single(HoverFunctionInfo {
                type_description,
                overloads: None,
                description: None,
                is_call_function,
            })
        }
        LuaType::Signature(signature_id) => {
            let signature_result = hover_signature_type(
                builder,
                db,
                signature_id.clone(),
                function_member,
                name,
                is_local,
                call_function,
                verbose,
            )
            .unwrap_or_else(|| HoverSignatureResult {
                type_description: format!("function {}", name),
                overloads: None,
                call_function: None,
            });

            let is_call_function = signature_result.call_function.is_some();

            ProcessFunctionTypeResult::Single(HoverFunctionInfo {
                type_description: signature_result.type_description,
                overloads: signature_result.overloads,
                description: None,
                is_call_function,
            })
        }
        LuaType::Union(union) => {
            let mut results = Vec::new();
            for union_type in union.into_vec() {
                match process_single_function_type(
                    builder,
                    db,
                    &union_type,
                    function_member,
                    name,
                    is_local,
                    call_function,
                    verbose,
                ) {
                    ProcessFunctionTypeResult::Single(info) => {
                        results.push(info);
                    }
                    ProcessFunctionTypeResult::Multiple(infos) => {
                        results.extend(infos);
                    }
                    ProcessFunctionTypeResult::Skip => {}
                }
            }

            if results.is_empty() {
                ProcessFunctionTypeResult::Skip
            } else {
                ProcessFunctionTypeResult::Multiple(results)
            }
        }
        _ => ProcessFunctionTypeResult::Single(HoverFunctionInfo {
            type_description: format!("function {}", name),
            overloads: None,
            description: None,
            is_call_function: false,
        }),
    }
}

fn process_single_function_type_with_exclusions(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: &LuaType,
    function_member: Option<&LuaMember>,
    name: &str,
    is_local: bool,
    call_function: Option<&LuaFunctionType>,
    processed_types: &HashSet<LuaType>,
    verbose: bool,
) -> ProcessFunctionTypeResult {
    match typ {
        LuaType::Union(union) => {
            let mut results = Vec::new();
            for union_type in union.into_vec() {
                // 跳过已经处理过的类型
                if processed_types.contains(&union_type) {
                    continue;
                }

                match process_single_function_type_with_exclusions(
                    builder,
                    db,
                    &union_type,
                    function_member,
                    name,
                    is_local,
                    call_function,
                    processed_types,
                    verbose,
                ) {
                    ProcessFunctionTypeResult::Single(info) => {
                        results.push(info);
                    }
                    ProcessFunctionTypeResult::Multiple(infos) => {
                        results.extend(infos);
                    }
                    ProcessFunctionTypeResult::Skip => {}
                }
            }

            if results.is_empty() {
                ProcessFunctionTypeResult::Skip
            } else {
                ProcessFunctionTypeResult::Multiple(results)
            }
        }
        _ => {
            // 对于非 Union 类型, 直接调用原函数
            process_single_function_type(
                builder,
                db,
                typ,
                function_member,
                name,
                is_local,
                call_function,
                verbose,
            )
        }
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
