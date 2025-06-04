use std::collections::HashSet;

use crate::handlers::hover::function_humanize::try_extract_signature_id_from_field;

use super::std_hover::{hover_std_description, is_std};
use emmylua_code_analysis::{
    format_union_type, DbIndex, LuaDocReturnInfo, LuaFunctionType, LuaMember, LuaMemberKey,
    LuaMemberOwner, LuaMultiLineUnion, LuaSemanticDeclId, LuaSignature, LuaSignatureId, LuaType,
    LuaUnionType, RenderLevel, SemanticDeclLevel, SemanticModel,
};

use emmylua_code_analysis::humanize_type;
use emmylua_parser::{LuaAstNode, LuaIndexExpr, LuaSyntaxKind};

use super::hover_builder::HoverBuilder;

pub fn hover_const_type(db: &DbIndex, typ: &LuaType) -> String {
    let const_value = humanize_type(db, typ, RenderLevel::Detailed);

    match typ {
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
            format!("integer = {}", const_value)
        }
        LuaType::FloatConst(_) => format!("number = {}", const_value),
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => format!("string = {}", const_value),
        _ => const_value,
    }
}

#[allow(unused)]
pub fn hover_function_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: &LuaType,
    function_member: Option<&LuaMember>,
    func_name: &str,
    is_local: bool,
) {
    match typ {
        LuaType::Function => builder.set_type_description(format!("function {}()", func_name)),
        LuaType::DocFunction(lua_func) => builder.set_type_description(hover_doc_function_type(
            builder,
            db,
            &lua_func,
            function_member,
            func_name,
        )),
        LuaType::Signature(signature_id) => {
            let (type_description, signature_overloads) = hover_signature_type(
                builder,
                db,
                signature_id.clone(),
                function_member,
                func_name,
                is_local,
                false,
            )
            .unwrap_or_else(|| (format!("function {}", func_name), None));
            builder.set_type_description(type_description);
            if let Some(signature_overloads) = signature_overloads {
                for overload in signature_overloads {
                    builder.add_signature_overload(overload);
                }
            }
        }
        LuaType::Union(union) => {
            hover_union_function_type(builder, db, union, function_member, func_name)
        }
        _ => builder.set_type_description(format!("function {}", func_name)),
    }
}

#[derive(Debug, Clone)]
struct HoverFunctionInfo {
    type_description: String,
    overloads: Option<Vec<String>>,
    description: Option<DescriptionInfo>,
    is_call_function: bool,
}

pub fn hover_function_type_new(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
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
                    if let Some(signature_id) =
                        try_extract_signature_id_from_field(builder.semantic_model, &member)
                    {
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

        match typ {
            LuaType::Function => {
                function_info.type_description = format!("function {}()", name);
            }
            LuaType::DocFunction(lua_func) => {
                function_info.type_description =
                    hover_doc_function_type(builder, db, &lua_func, function_member, &name);
                if let Some(call_function) = &call_function {
                    if call_function.get_params() == lua_func.get_params() {
                        function_info.is_call_function = true;
                    }
                }
            }
            LuaType::Signature(signature_id) => {
                let signature_result = hover_signature_type_new(
                    builder,
                    db,
                    signature_id.clone(),
                    function_member,
                    &name,
                    is_local,
                )
                .unwrap_or_else(|| HoverSignatureResult {
                    type_description: format!("function {}", name),
                    overloads: None,
                    call_function: None,
                });
                function_info.type_description = signature_result.type_description;
                function_info.overloads = signature_result.overloads;

                if let Some(_) = signature_result.call_function {
                    function_info.is_call_function = true;
                }
            }
            LuaType::Union(_) => {
                continue;
            }
            _ => {
                function_info.type_description = format!("function {}", name);
            }
        };
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
            let last_type_desc = type_descs.pop()?;
            builder.set_type_description(last_type_desc.type_description.clone());
            builder.add_description_from_info(last_type_desc.description.clone());
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

fn hover_union_function_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    union: &LuaUnionType,
    function_member: Option<&LuaMember>,
    func_name: &str,
) {
    // 泛型处理
    if let Some(call) = builder.get_call_function() {
        builder.set_type_description(hover_doc_function_type(
            builder,
            db,
            &call,
            function_member,
            func_name,
        ));
        return;
    }
    let mut overloads = Vec::new();

    let types = union.get_types();
    for typ in types {
        match typ {
            LuaType::DocFunction(lua_func) => {
                overloads.push(hover_doc_function_type(
                    builder,
                    db,
                    &lua_func,
                    function_member,
                    func_name,
                ));
            }
            LuaType::Signature(signature_id) => {
                if let Some((type_description, signature_overloads)) = hover_signature_type(
                    builder,
                    db,
                    signature_id.clone(),
                    function_member,
                    func_name,
                    false,
                    true,
                ) {
                    if let Some(signature_overloads) = signature_overloads {
                        for overload in signature_overloads {
                            overloads.push(overload);
                        }
                    }
                    overloads.push(type_description);
                }
            }
            _ => {}
        }
    }
    // 将最后一个作为 type_description
    if let Some(type_description) = overloads.pop() {
        builder.set_type_description(type_description);
        for overload in overloads {
            builder.add_signature_overload(overload);
        }
    }
}

fn hover_doc_function_type(
    builder: &HoverBuilder,
    db: &DbIndex,
    lua_func: &LuaFunctionType,
    owner_member: Option<&LuaMember>,
    func_name: &str,
) -> String {
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
        if let Some(LuaMemberOwner::Type(type_decl_id)) = parent_owner {
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
            if index == 0 && is_method {
                "".to_string()
            } else if let Some(ty) = &param.1 {
                format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Normal))
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
                format!(" -> {}", humanize_type(db, ret_type, RenderLevel::Simple))
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

fn hover_signature_type_new(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    signature_id: LuaSignatureId,
    owner_member: Option<&LuaMember>,
    func_name: &str,
    is_local: bool,
) -> Option<HoverSignatureResult> {
    let signature = db.get_signature_index().get(&signature_id)?;

    let call_function = builder.get_call_function();
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
        if let Some(LuaMemberOwner::Type(type_decl_id)) = parent_owner {
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
                    format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
                } else {
                    name
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        let rets = build_signature_rets(builder, signature, builder.is_completion, None);
        let result = format_function_type(type_label, async_label, full_name.clone(), params, rets);
        // 由于 @field 定义的`docfunction`会被视为`signature`, 因此这里额外处理
        if let Some(call_function) = &call_function {
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
                        format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
                    } else {
                        name
                    }
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            let rets =
                build_signature_rets(builder, signature, builder.is_completion, Some(overload));
            let result =
                format_function_type(type_label, async_label, full_name.clone(), params, rets);

            if let Some(call_function) = &call_function {
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

fn hover_signature_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    signature_id: LuaSignatureId,
    owner_member: Option<&LuaMember>,
    func_name: &str,
    is_local: bool,
    is_form_union: bool,
) -> Option<(String, Option<Vec<String>>)> {
    let signature = db.get_signature_index().get(&signature_id)?;

    if is_form_union && signature.param_docs.is_empty() {
        return None;
    }
    let call_signature = builder.get_call_function();
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
        if let Some(LuaMemberOwner::Type(type_decl_id)) = parent_owner {
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
    let signature_info = {
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
                    format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
                } else {
                    name
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        let rets = build_signature_rets(builder, signature, builder.is_completion, None);
        let result = format_function_type(type_label, async_label, full_name.clone(), params, rets);
        // 由于 @field 定义的`docfunction`会被视为`signature`, 因此这里额外处理
        if let Some(call_signature) = &call_signature {
            if call_signature.get_params() == signature.get_type_params() {
                // 如果具有完全匹配的签名, 那么将其设置为当前签名, 且不显示重载
                return Some((result, None));
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
                        format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
                    } else {
                        name
                    }
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            let rets =
                build_signature_rets(builder, signature, builder.is_completion, Some(overload));
            let result =
                format_function_type(type_label, async_label, full_name.clone(), params, rets);

            if let Some(call_signature) = &call_signature {
                if *call_signature == **overload {
                    // 如果具有完全匹配的签名, 那么将其设置为当前签名, 且不显示重载
                    return Some((result, None));
                }
            };
            overloads.push(result);
        }
        overloads
    };

    Some((signature_info, Some(overloads)))
}

fn build_signature_rets(
    builder: &mut HoverBuilder,
    signature: &LuaSignature,
    is_completion: bool,
    overload: Option<&LuaFunctionType>,
) -> String {
    let db = builder.semantic_model.get_db();
    let mut result = String::new();
    // overload 的返回值固定为单行
    let overload_rets_string = if let Some(overload) = overload {
        let ret_type = overload.get_ret();
        match ret_type {
            LuaType::Nil => "".to_string(),
            _ => {
                format!(" -> {}", humanize_type(db, ret_type, RenderLevel::Simple))
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
                        .map(|(i, ret)| build_signature_ret_type(builder, ret, i))
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
                let type_text = build_signature_ret_type(builder, ret, i);
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
) -> String {
    let type_expansion_count = builder.get_type_expansion_count();
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

pub fn hover_humanize_type(
    builder: &mut HoverBuilder,
    ty: &LuaType,
    fallback_level: Option<RenderLevel>, // 当有值时, 若获取类型描述为空会回退到使用`humanize_type()`
) -> String {
    let db = builder.semantic_model.get_db();
    match ty {
        LuaType::Ref(type_decl_id) => {
            if let Some(type_decl) = db.get_type_index().get_type_decl(type_decl_id) {
                if let Some(LuaType::MultiLineUnion(multi_union)) =
                    type_decl.get_alias_origin(db, None)
                {
                    return hover_multi_line_union_type(
                        builder,
                        db,
                        multi_union.as_ref(),
                        Some(type_decl.get_full_name()),
                    )
                    .unwrap_or_default();
                }
            }
            humanize_type(db, ty, fallback_level.unwrap_or(RenderLevel::Simple))
        }
        LuaType::MultiLineUnion(multi_union) => {
            hover_multi_line_union_type(builder, db, multi_union.as_ref(), None).unwrap_or_default()
        }
        LuaType::Union(union) => hover_union_type(builder, union, RenderLevel::Detailed),
        _ => humanize_type(db, ty, fallback_level.unwrap_or(RenderLevel::Simple)),
    }
}

fn hover_union_type(
    builder: &mut HoverBuilder,
    union: &LuaUnionType,
    level: RenderLevel,
) -> String {
    format_union_type(union, level, |ty, level| {
        hover_humanize_type(builder, ty, Some(level))
    })
}

fn hover_multi_line_union_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    multi_union: &LuaMultiLineUnion,
    ty_name: Option<&str>,
) -> Option<String> {
    let members = multi_union.get_unions();
    let type_name = if ty_name.is_none() {
        let members = multi_union.get_unions();
        let type_str = members
            .iter()
            .take(10)
            .map(|(ty, _)| humanize_type(db, ty, RenderLevel::Simple))
            .collect::<Vec<_>>()
            .join("|");
        Some(format!("({})", type_str))
    } else {
        ty_name.map(|name| name.to_string())
    };
    let mut text = format!("{}:\n", type_name.clone().unwrap_or_default());
    for (typ, description) in members {
        let type_humanize_text = humanize_type(db, &typ, RenderLevel::Minimal);
        if let Some(description) = description {
            text.push_str(&format!(
                "    | {} -- {}\n",
                type_humanize_text, description
            ));
        } else {
            text.push_str(&format!("    | {}\n", type_humanize_text));
        }
    }
    builder.add_type_expansion(text);
    type_name
}

/// 推断前缀是否为全局定义, 如果是, 则返回全局名称, 否则返回 None
pub fn infer_prefix_global_name<'a>(
    semantic_model: &'a SemanticModel,
    member: &LuaMember,
) -> Option<&'a str> {
    let root = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&member.get_file_id())?
        .get_red_root();
    let cur_node = member.get_syntax_id().to_node_from_root(&root)?;

    match cur_node.kind().into() {
        LuaSyntaxKind::IndexExpr => {
            let index_expr = LuaIndexExpr::cast(cur_node)?;
            let semantic_decl = semantic_model.find_decl(
                index_expr
                    .get_prefix_expr()?
                    .get_syntax_id()
                    .to_node_from_root(&root)
                    .unwrap()
                    .into(),
                SemanticDeclLevel::default(),
            );
            if let Some(property_owner) = semantic_decl {
                if let LuaSemanticDeclId::LuaDecl(id) = property_owner {
                    if let Some(decl) = semantic_model.get_db().get_decl_index().get_decl(&id) {
                        if decl.is_global() {
                            return Some(decl.get_name());
                        }
                    }
                }
            }
        }
        _ => {}
    }
    None
}

/// 描述信息结构体
#[derive(Debug, Clone)]
pub struct DescriptionInfo {
    pub description: Option<String>,
    pub see_content: Option<String>,
    pub other_content: Option<String>,
}

impl DescriptionInfo {
    pub fn new() -> Self {
        Self {
            description: None,
            see_content: None,
            other_content: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.description.is_none() && self.see_content.is_none() && self.other_content.is_none()
    }
}

/// 从属性所有者获取描述信息
pub fn extract_description_from_property_owner(
    semantic_model: &SemanticModel,
    property_owner: &LuaSemanticDeclId,
) -> Option<DescriptionInfo> {
    let property = semantic_model
        .get_db()
        .get_property_index()
        .get_property(&property_owner)?;

    let mut result = DescriptionInfo::new();

    if let Some(detail) = &property.description {
        let mut description = detail.to_string();

        match property_owner {
            LuaSemanticDeclId::Member(id) => {
                if let Some(member) = semantic_model.get_db().get_member_index().get_member(&id) {
                    if let Some(LuaMemberOwner::Type(ty)) = semantic_model
                        .get_db()
                        .get_member_index()
                        .get_current_owner(&id)
                    {
                        if is_std(semantic_model.get_db(), member.get_file_id()) {
                            let std_desc =
                                hover_std_description(ty.get_name(), member.get_key().get_name());
                            if !std_desc.is_empty() {
                                description = std_desc;
                            }
                        }
                    }
                }
            }
            LuaSemanticDeclId::LuaDecl(id) => {
                if let Some(decl) = semantic_model.get_db().get_decl_index().get_decl(&id) {
                    if is_std(semantic_model.get_db(), decl.get_file_id()) {
                        let std_desc = hover_std_description(decl.get_name(), None);
                        if !std_desc.is_empty() {
                            description = std_desc;
                        }
                    }
                }
            }
            _ => {}
        }

        result.description = Some(description);
    }

    if let Some(see) = &property.see_content {
        result.see_content = Some(see.to_string());
    }
    if let Some(other) = &property.other_content {
        result.other_content = Some(other.to_string());
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}
